//! LEGO に搭載する ESP32-Pico ベースのモーター制御アプリケーション `bud`。

mod gamepad;
mod motor;

use anyhow::Result;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::gpio::{IOPin, PinDriver};
use esp_idf_svc::hal::ledc::{config::TimerConfig, LedcDriver, LedcTimerDriver};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::prelude::*;

use gamepad::{Gamepad, NullGamepad};
use motor::{DcMotor, Servo};

/// メインループの周期。
const LOOP_INTERVAL_MS: u32 = 33;

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let pins = peripherals.pins;

    // LEDC タイマーは 3 チャンネル（左右モーター + サーボ）で共有する。
    let timer_config = TimerConfig::new().frequency(50.Hz());
    let timer = LedcTimerDriver::new(peripherals.ledc.timer0, &timer_config)?;

    // 配線に合わせて GPIO 番号を変更すること。ESP32-PICO-D4 は内蔵フラッシュ用に
    // GPIO6〜11 を使用しているため、モーター/サーボの配線には使わない。
    let mut motor_left = DcMotor::new(
        LedcDriver::new(peripherals.ledc.channel0, &timer, pins.gpio25)?,
        PinDriver::output(pins.gpio26.downgrade())?,
        PinDriver::output(pins.gpio27.downgrade())?,
    )?;
    let mut motor_right = DcMotor::new(
        LedcDriver::new(peripherals.ledc.channel1, &timer, pins.gpio32)?,
        PinDriver::output(pins.gpio33.downgrade())?,
        PinDriver::output(pins.gpio14.downgrade())?,
    )?;
    let mut arm_servo = Servo::new(LedcDriver::new(peripherals.ledc.channel2, &timer, pins.gpio13)?);

    // TB6612FNG 等の STBY ピンを常時有効化する。
    let mut standby = PinDriver::output(pins.gpio4.downgrade())?;
    standby.set_high()?;

    // gamepad::bt_hid が PS4 コントローラー接続を実装済みだが、HID Input レポートの
    // 解析(Gamepad実装への橋渡し)は未実装のため、当面はダミー入力を使う。
    let mut gamepad = NullGamepad;

    log::info!("started");

    loop {
        let state = gamepad.poll();

        motor_left.set(state.left_stick_y)?;
        motor_right.set(state.right_stick_y)?;
        if state.buttons.cross {
            arm_servo.set_angle_deg(0.0)?;
        } else if state.buttons.circle {
            arm_servo.set_angle_deg(180.0)?;
        }

        FreeRtos::delay_ms(LOOP_INTERVAL_MS);
    }
}
