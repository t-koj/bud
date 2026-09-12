//! LEGO に搭載する ESP32-Pico ベースのモーター制御アプリケーション `bud`。

mod gamepad;
mod led;
mod motor;

use anyhow::Result;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::i2c::{I2cConfig, I2cDriver};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::prelude::*;

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

/// ATOM Matrix搭載の5x5 WS2812Cマトリクスの画素数（ATOM Liteの場合は1画素）。
const LED_PIXEL_COUNT: usize = 25;

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    // DS4接続後、HID Inputレポートの受信頻度に対してBluedroidのACLキューが
    // 一時的に輻輳し、"ACL queue high watermark"警告が大量に出ることがある。
    // この警告ログ自体がUART出力（低速）でCPUを占有し、hciTタスクがログ出力に
    // 詰まって処理に戻れず、タスクウォッチドッグがリセットする実機不具合を確認した。
    // ログを出さないようにするだけで詰まりが解消するため、該当タグを黙らせる。
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

    // ATOM Matrix v1.1 のGrove(I2C)ポート固定配線。ATOM Liteの場合はSDA=gpio25/SCL=gpio21。
    let i2c_config = I2cConfig::new().baudrate(100.kHz().into());
    let i2c = I2cDriver::new(peripherals.i2c0, pins.gpio32, pins.gpio26, &i2c_config)?;
    let mut motion = AtomicMotion::new(i2c);

    // ATOM Matrix/Lite のオンボードRGB LEDはGPIO27固定配線。
    let mut led = Led::new(peripherals.rmt.channel0, pins.gpio27, LED_PIXEL_COUNT)?;

    // LED自体(RMT/WS2812駆動)が正しく動作するか、PS4接続やボタン入力と切り離して
    // 起動時に自己確認できるよう、一度ON/OFFさせる。
    log::info!("LED self-test: on");
    led.on()?;
    FreeRtos::delay_ms(500);
    log::info!("LED self-test: off");
    led.off()?;

    let rx = bt_hid::init()?;
    let mut gamepad = Ds4Gamepad::new(rx);

    log::info!("waiting for PS4 controller (SHARE+PS pairing, or PS button if already paired)...");
    while !gamepad.is_connected() {
        bt_hid::scan_and_connect(PS4_CONTROLLER_NAME_PREFIX, SCAN_SECONDS)?;
        gamepad.poll();
    }
    log::info!("PS4 controller connected");

    let mut prev_circle = false;
    let mut motor_ok = [true; 2];

    loop {
        let state = gamepad.poll();

        // ATOMIC Motionベース未接続時のI2C NACKや一時的なバス異常は回復可能なエラーとして
        // 扱い、メインループ（PS4接続・LED制御）自体は継続する
        // （docs/coding.mdのエラーハンドリング方針）。
        //
        // 未接続のI2Cバスへの書き込みはタイムアウトするまでメインループ全体をブロックする
        // ため、失敗したチャンネルは以降二度と再試行しない。ATOMIC Motionベースは起動時に
        // 配線されているかどうかで決まり、実行中に後から接続されることは無いため、初回の
        // 書き込みで確立しなければ以降も回復する見込みが無く、毎フレーム（または間引いても
        // 定期的に）再試行することはPS4コントローラーの入力ポーリング
        // （gamepad.poll()の呼び出し頻度）を無駄に落とすだけだった。
        let motor_speeds = [state.left_stick_y, state.right_stick_y];
        for (channel, &speed) in motor_speeds.iter().enumerate() {
            if !motor_ok[channel] {
                continue;
            }

            match motion.set_motor_speed(channel as u8, speed) {
                Ok(()) => {}
                Err(e) => {
                    log::warn!(
                        "set_motor_speed({channel}) failed (ATOMIC Motionベース未接続の可能性): {e}"
                    );
                    motor_ok[channel] = false;
                }
            }
        }

        if state.buttons.circle && !prev_circle {
            if let Err(e) = led.toggle() {
                log::warn!("led.toggle() failed: {e}");
            }
        }
        prev_circle = state.buttons.circle;

        FreeRtos::delay_ms(LOOP_INTERVAL_MS);
    }
}
