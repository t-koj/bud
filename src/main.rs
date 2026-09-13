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

    // ATOM Matrix/Lite はGPIO配線が異なるため、ビルド時に指定したfeatureで固定する
    // （main.rs冒頭のcompile_error!によりmatrix/liteのどちらか一方の指定を必須にしている）。
    #[cfg(feature = "matrix")]
    let (i2c_sda, i2c_scl, led_pixel_count) = {
        log::info!("ATOM model: Matrix (GPIO32/SDA, GPIO26/SCL)");
        (pins.gpio32, pins.gpio26, 25)
    };
    #[cfg(feature = "lite")]
    let (i2c_sda, i2c_scl, led_pixel_count) = {
        log::info!("ATOM model: Lite (GPIO25/SDA, GPIO21/SCL)");
        (pins.gpio25, pins.gpio21, 1)
    };

    // ATOM Matrix/Lite の I2C 配線はモデルごとに固定されているため、ここで切り替える。
    let i2c_config = I2cConfig::new().baudrate(100.kHz().into());
    let mut i2c = I2cDriver::new(peripherals.i2c0, i2c_sda, i2c_scl, &i2c_config)?;

    // ATOMIC Motionベース(I2Cアドレス0x38)への書き込みが実機で常に失敗する問題の
    // 切り分けのため、起動時にI2Cバス上の全アドレスをスキャンし応答の有無をログに残す。
    // 原因判明後は削除する想定の一時的な診断コード。
    let mut i2c_scan_found = Vec::new();
    for addr in 1u8..=127 {
        if i2c.write(addr, &[], 20).is_ok() {
            i2c_scan_found.push(addr);
        }
    }
    if i2c_scan_found.is_empty() {
        log::warn!("I2C scan: no device responded (配線/電源/プルアップの可能性)");
    } else {
        log::info!("I2C scan: device(s) found at {i2c_scan_found:#04x?}");
    }

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

    // S1(channel 0)は左スティック上下、S3(channel 2)は右スティック上下に割り当てる。
    // 書き込みが失敗しても`SERVO_RETRY_INTERVAL_FRAMES`フレームごとに再試行する
    // （ATOMIC Motionベースが後から接続される、または起動直後で応答できないケースに対応するため）。
    let mut servo_retry_countdown = [0u32; 2];
    let mut servo_error = [false; 2];

    loop {
        let state = gamepad.poll();
        let servo_targets = [(0u8, state.left_stick_y), (2u8, state.right_stick_y)];
        for (idx, &(channel, stick)) in servo_targets.iter().enumerate() {
            if servo_retry_countdown[idx] > 0 {
                servo_retry_countdown[idx] -= 1;
                continue;
            }

            let angle = motor::stick_to_servo_angle(stick);
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

        let led_status = if servo_error.iter().any(|&e| e) {
            led.set_error()
        } else {
            led.set_ok()
        };
        if let Err(e) = led_status {
            log::warn!("led status update failed: {e}");
        }

        FreeRtos::delay_ms(LOOP_INTERVAL_MS);
    }
}
