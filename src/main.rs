//! LEGO に搭載する ESP32-Pico ベースのモーター制御アプリケーション `bud`。

#[cfg(not(any(feature = "matrix", feature = "lite")))]
compile_error!(
    "feature \"matrix\" か \"lite\" のどちらか一方を指定してください（例: cargo build --features matrix）"
);

#[cfg(all(feature = "matrix", feature = "lite"))]
compile_error!("feature \"matrix\" と \"lite\" は同時に指定できません");

mod connecting_animation;
mod gamepad;
mod gpio_servo;
mod led;
mod atomic_motion;
// まだどの設定値も保存していないため、利用側が実装されるまで未使用警告を抑止する
#[allow(dead_code)]
mod preferences;
use esp_idf_svc::sys::{ESP_BLE_APPEARANCE_PULSE_OXIMETER_FINGERTIP, netif_ext_callback_args_t_ipv6_addr_state_changed_s};
use gamepad::Dpad;

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
use atomic_motion::AtomicMotion;

use crate::atomic_motion::{apply_stick_deadzone, stick_to_servo_pulse};

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

    // ATOM Matrix/Lite はオンボードLEDの画素数が異なるため、ビルド時に指定したfeatureで
    // 固定する（main.rs冒頭のcompile_error!によりmatrix/liteのどちらか一方を指定する)
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

    let partition = esp_idf_svc::nvs::EspDefaultNvsPartition::take()?;
    let mut preferences = preferences::Preferences::open(partition, "bud")?;
    let mut center = preferences.get_u32("center")?.unwrap_or(1_500) as u16;

    // ATOMIC Motionベースとの接続はATOM Matrix/Lite共通でG25(SDA)/G21(SCL)
    let i2c_config = I2cConfig::new().baudrate(100.kHz().into());
    let i2c = I2cDriver::new(peripherals.i2c0, pins.gpio25, pins.gpio21, &i2c_config)?;
    
    let mut motion = AtomicMotion::new(i2c);

    // ATOM Matrix/Lite のオンボードRGB LEDはGPIO27固定配線。LEDの画素数は機種に応じて切り替える。
    let mut led = Led::new(peripherals.rmt.channel0, pins.gpio27, led_pixel_count)?;
    led.on()?;

    let rx = bt_hid::init()?;
    let mut gamepad = Ds4Gamepad::new(rx);

    log::info!("waiting for PS4 controller (SHARE+PS pairing, or PS button if already paired)...");
    let animation = ConnectingAnimation::start(led);
    while !gamepad.is_connected() {
        bt_hid::scan_and_connect(PS4_CONTROLLER_NAME_PREFIX, SCAN_SECONDS)?;
        gamepad.poll();
    }
    let mut led = animation.stop()?;
    log::info!("PS4 controller connected");

    for i in 0u8 .. 4 {
        motion.set_servo_pulse(i, center)?;
    }

    wait_gamepad_ready(&mut gamepad, led);
    
    // main loop
    loop {
        let loop_start = Instant::now();
        let state = gamepad.poll();

        let targets = [
            (0u8, state.left_stick_x),
            (1u8, state.left_stick_y),
            (2u8, state.right_stick_x),
            (3u8, state.right_stick_y),
        ];

        for (idx, stick) in targets {
            let stick = apply_stick_deadzone(stick);
            let pulse = stick_to_servo_pulse(stick);
            if let Err(e) = motion.set_servo_pulse(idx, pulse) {
                log::warn!(
                    "set_servo_pulse({idx}, width_us={pulse}) failed: {e}"
                );
            }
        }

        let speed = state.l2_analog as i16 - state.r2_analog as i16;
        if let Err(e) = motion.set_motor_speed(0, speed as i8) {
            log::warn!(
                "set_motor_speed(0, speed={speed}) failed: {e}"
            );
        }

        match state.buttons.dpad {
            Dpad::Left => {
                center = center.saturating_add(10);
                preferences.set_u32("center", center as u32)?;
            }
            Dpad::Right => {
                center = center.saturating_sub(10);
                preferences.set_u32("center", center as u32)?;
            }
            _ => {}
        }

        let elapsed_ms = loop_start.elapsed().as_millis() as u32;
        FreeRtos::delay_ms(LOOP_INTERVAL_MS.saturating_sub(elapsed_ms));
    }
}

fn wait_gamepad_ready(gamepad: &mut Ds4Gamepad, mut led: Led<'_>) {
    if let Err(e) = led.set_preparing() {
        log::warn!("failed to set LED to preparing state: {e}");
    }
    while !gamepad.is_operation_ready() {
        gamepad.poll();
        FreeRtos::delay_ms(LOOP_INTERVAL_MS);
    }
    if let Err(e) = led.set_ok() {
        log::warn!("failed to set LED to ok state: {e}");
    }
    log::info!("BT stack mode change event received; controller operation is now ready");
    }
