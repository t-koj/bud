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
use esp_idf_svc::sys::netif_ext_callback_args_t_ipv6_addr_state_changed_s;
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
use gpio_servo::GpioServo;
use led::Led;
use atomic_motion::AtomicMotion;

/// メインループの周期。
const LOOP_INTERVAL_MS: u32 = 50;

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

    // GPIO33/GPIO19から直接サーボ制御信号を出す。2チャンネルで50HzのLEDCタイマーを共有する。
    // 生成直後は信号を出さず、スティックへの割り当ては未実施。
    let servo_timer = gpio_servo::new_servo_timer(peripherals.ledc.timer0)?;
    let _gpio_servo_g33 = GpioServo::new(peripherals.ledc.channel0, &servo_timer, pins.gpio33)?;
    let _gpio_servo_g19 = GpioServo::new(peripherals.ledc.channel1, &servo_timer, pins.gpio19)?;

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

    for i in 0u8 .. 4 {
        motion.set_servo_pulse(i, 1500)?;
    }

    // S1(channel 0)は左スティック上下、S3(channel 2)は右スティック上下に割り当てる。
    // 接続直後はBluetoothスタックのリンクポリシー・ネゴシエーションが未完了で、
    // 実際の操作が安定しない期間があるため、それを示す専用のLED表示を挟む。
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
    
    let max_movement_per_frame   = 100;

    // main loop
    loop {
        let loop_start = Instant::now();
        let state = gamepad.poll();
        let servo_targets = [
            (0u8, state.left_stick_x),
        ];

        for (idx, &(channel, stick)) in servo_targets.iter().enumerate() {
            let current = motion.get_servo_pulse(idx as u8)?;
            print!("servo channel {} pulse width: {}\r", idx, current);
            let target = atomic_motion::stick_to_servo_pulse(stick);

            // easing: limit the change in servo pulse width per frame to avoid abrupt movements
            let next = target.clamp(current.saturating_sub(max_movement_per_frame),
                current.saturating_add(max_movement_per_frame));

            if let Err(e) =  motion.set_servo_pulse(channel, next) {
                log::warn!(
                    "set_servo_pulse({channel}, width_us={next}) failed: {e}"
                );
            }
        }
        let elapsed_ms = loop_start.elapsed().as_millis() as u32;
        FreeRtos::delay_ms(LOOP_INTERVAL_MS.saturating_sub(elapsed_ms));
    }
}
