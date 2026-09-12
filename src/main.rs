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
    let i2c = I2cDriver::new(peripherals.i2c0, i2c_sda, i2c_scl, &i2c_config)?;
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
    led.off()?;
    log::info!("PS4 controller connected");

    let mut prev_circle = false;
    let mut motor_ok = [true; 2];

    loop {
        let state = gamepad.poll();
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
