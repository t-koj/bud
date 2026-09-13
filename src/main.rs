//! LEGO に搭載する ESP32-Pico ベースのモーター制御アプリケーション `bud`。

#[cfg(not(any(feature = "matrix", feature = "lite")))]
compile_error!(
    "feature \"matrix\" か \"lite\" のどちらか一方を指定してください（例: cargo build --features matrix）"
);

#[cfg(all(feature = "matrix", feature = "lite"))]
compile_error!("feature \"matrix\" と \"lite\" は同時に指定できません");

mod connecting_animation;
mod gamepad;
mod led;
mod motor;

use std::time::Instant;

use anyhow::Result;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::i2c::{I2cConfig, I2cDriver};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::prelude::*;

use connecting_animation::ConnectingAnimation;
use gamepad::bt_hid::{self, Ds4Gamepad};
use gamepad::Gamepad;
use led::Led;
use motor::AtomicMotion;

/// メインループの周期。
const LOOP_INTERVAL_MS: u32 = 33;

/// PS4 (DualShock 4) のBluetoothデバイス名。
const PS4_CONTROLLER_NAME_PREFIX: &str = "Wireless Controller";

/// 未接続時に1回のスキャンで待機する秒数。接続待機ループはこれを繰り返す。
const SCAN_SECONDS: u32 = 5;

/// サーボへのI2C書き込みが失敗した際、次に再試行するまでのフレーム数。
const SERVO_RETRY_INTERVAL_FRAMES: u32 = 20;

/// サーボチャンネル(0〜3 = S1〜S4)ごとのニュートラル点トリム（度）。
/// S1(0)/S3(2)は180度サーボのため0.0のままでよい。S2(1)/S4(3)は360度連続回転
/// サーボで、実際の停止点が90度からずれている個体があるため、スティック中央で
/// 回転が止まるように実機で調整する（詳細は[design/motor.md](../docs/design/motor.md)参照）。
const SERVO_NEUTRAL_TRIM_DEG: [f32; 4] = [0.0, 0.0, 0.0, 0.0];

/// コントローラー接続後、`gamepad.is_operation_ready()`がtrueになるまでの
/// 待ち時間の上限（ミリ秒）。BluedroidスタックのSniffモード遷移イベントは接続後
/// 約30秒（ESP-IDF内部定数`BTA_DM_PM_HH_OPEN_DELAY`）で届く想定だが、万一届かない
/// 場合に備えてタイムアウトでフォールバックする。
const OPERATION_READY_TIMEOUT_MS: u32 = 45_000;

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    // DS4接続直後に ACL キューが一時的に輻輳し、"ACL queue high watermark" 警告が多発する。
    // このログが UART 出力で CPU を占有し、watchdog で再起動するため、影響タグの出力を抑止する。
    unsafe {
        esp_idf_svc::sys::esp_log_level_set(
            c"BT_HCI".as_ptr(),
            esp_idf_svc::sys::esp_log_level_t_ESP_LOG_ERROR,
        );
        esp_idf_svc::sys::esp_log_level_set(
            c"BT_BTC".as_ptr(),
            esp_idf_svc::sys::esp_log_level_t_ESP_LOG_ERROR,
        );
    }

    let peripherals = Peripherals::take()?;
    let pins = peripherals.pins;

    // ATOM Matrix/Lite はオンボードLEDの画素数が異なるため、ビルド時に指定したfeatureで
    // 固定する（main.rs冒頭のcompile_error!によりmatrix/liteのどちらか一方の指定を
    // 必須にしている）。
    #[cfg(feature = "matrix")]
    let led_pixel_count = {
        log::info!("ATOM model: Matrix");
        25
    };
    #[cfg(feature = "lite")]
    let led_pixel_count = {
        log::info!("ATOM model: Lite");
        1
    };

    // ATOMIC Motionベースとの接続はATOM Matrix/Lite共通でG25(SDA)/G21(SCL)固定
    // （M5Stack公式ドキュメント参照）。ATOM MatrixのGroveポート配線(GPIO32/26)を
    // 使うと誤って想定していたが、ATOMICシリーズの拡張ベースは底面のHY2.0-4P
    // スタッキングコネクタ経由で接続され、機種に依らずG25/G21固定であることが
    // 実機のI2Cバス全アドレススキャン（応答皆無）を受けて判明した。
    let i2c_config = I2cConfig::new().baudrate(100.kHz().into());
    let i2c = I2cDriver::new(peripherals.i2c0, pins.gpio25, pins.gpio21, &i2c_config)?;
    let mut motion = AtomicMotion::new(i2c);

    // ATOM Matrix/Lite のオンボードRGB LEDはGPIO27固定配線。LEDの画素数は機種に応じて切り替える。
    let mut led = Led::new(peripherals.rmt.channel0, pins.gpio27, led_pixel_count)?;
    led.on()?;

    let rx = bt_hid::init()?;
    let mut gamepad = Ds4Gamepad::new(rx);

    log::info!("waiting for PS4 controller (SHARE+PS pairing, or PS button if already paired)...");
    // scan_and_connectは1回あたり数秒ブロックするため、待機中はLEDアニメーションを
    // 別スレッドに任せる。
    let animation = ConnectingAnimation::start(led);
    while !gamepad.is_connected() {
        bt_hid::scan_and_connect(PS4_CONTROLLER_NAME_PREFIX, SCAN_SECONDS)?;
        gamepad.poll();
    }
    let mut led = animation.stop()?;
    log::info!("PS4 controller connected");

    // S1(channel 0)は左スティック上下、S2(channel 1)は左スティック左右、
    // S3(channel 2)は右スティック上下、S4(channel 3)は右スティック左右に割り当てる。
    // 書き込みが失敗しても`SERVO_RETRY_INTERVAL_FRAMES`フレームごとに再試行する
    // （ATOMIC Motionベースが後から接続される、または起動直後で応答できないケースに対応するため）。
    let mut servo_retry_countdown = [0u32; 4];
    let mut servo_error = [false; 4];

    // 接続直後はBluetoothスタックのリンクポリシー・ネゴシエーションが未完了で、
    // 実際の操作が安定しない期間があるため、それを示す専用のLED表示を挟む。
    let mut operation_ready = false;
    let mut operation_ready_wait_ms: u32 = 0;

    loop {
        let loop_start = Instant::now();

        let state = gamepad.poll();
        let servo_targets = [
            (0u8, state.left_stick_y),
            (1u8, state.left_stick_x),
            (2u8, state.right_stick_y),
            (3u8, state.right_stick_x),
        ];
        for (idx, &(channel, stick)) in servo_targets.iter().enumerate() {
            if servo_retry_countdown[idx] > 0 {
                servo_retry_countdown[idx] -= 1;
                continue;
            }

            let angle = motor::stick_to_servo_angle_with_trim(
                motor::apply_stick_deadzone(stick),
                SERVO_NEUTRAL_TRIM_DEG[idx],
            );
            match motion.set_servo_angle(channel, angle) {
                Ok(()) => {
                    if servo_error[idx] {
                        log::info!("set_servo_angle({channel}) recovered (angle={angle})");
                    }
                    servo_error[idx] = false;
                }
                Err(e) => {
                    log::warn!(
                        "set_servo_angle({channel}, angle={angle}) failed (ATOMIC Motionベース未接続の可能性): {e}"
                    );
                    servo_error[idx] = true;
                    servo_retry_countdown[idx] = SERVO_RETRY_INTERVAL_FRAMES;
                }
            }
        }

        if !operation_ready {
            if gamepad.is_operation_ready() {
                operation_ready = true;
                log::info!("BT stack mode change event received; controller operation is now ready");
            } else if operation_ready_wait_ms >= OPERATION_READY_TIMEOUT_MS {
                operation_ready = true;
                log::warn!(
                    "BT stack mode change event not received within {OPERATION_READY_TIMEOUT_MS}ms; proceeding anyway"
                );
            } else {
                operation_ready_wait_ms += LOOP_INTERVAL_MS;
            }
        }

        let led_status = if !operation_ready {
            led.set_preparing()
        } else if servo_error.iter().any(|&e| e) {
            led.set_error()
        } else {
            led.set_ok()
        };
        if let Err(e) = led_status {
            log::warn!("led status update failed: {e}");
        }

        // 処理に要した時間を差し引いた残り時間だけ待機し、ループ周期をLOOP_INTERVAL_MSに
        // 近づける（処理時間が周期を超えた場合は待機せず即座に次周期へ進む）。
        let elapsed_ms = loop_start.elapsed().as_millis() as u32;
        FreeRtos::delay_ms(LOOP_INTERVAL_MS.saturating_sub(elapsed_ms));
    }
}
