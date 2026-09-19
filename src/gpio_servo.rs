use anyhow::Result;
use esp_idf_svc::hal::gpio::OutputPin;
use esp_idf_svc::hal::ledc::config::{Resolution, TimerConfig};
use esp_idf_svc::hal::ledc::{LedcChannel, LedcDriver, LedcTimer, LedcTimerDriver};
use esp_idf_svc::hal::peripheral::Peripheral;
use esp_idf_svc::hal::prelude::*;

/// サーボ制御信号の周期(50Hz)。
const SERVO_PERIOD_US: u32 = 20_000;
const SERVO_FREQUENCY_HZ: u32 = 50;

/// 0度・180度に対応するパルス幅。M5Stack SG90系サーボの仕様(0.5〜2.5ms)に合わせる。
const MIN_PULSE_US: u32 = 500;
const MAX_PULSE_US: u32 = 2_500;

/// 20ms周期に対しパルス幅を約1.2μs刻みで指定できる分解能。
const SERVO_RESOLUTION: Resolution = Resolution::Bits14;

/// 全 `GpioServo` チャンネルが共有する50HzのLEDCタイマーを生成する。
/// LEDCは周波数をタイマー単位でしか設定できないため、チャンネルとは別に生成して借用で渡す。
pub fn new_servo_timer<'a, T: LedcTimer + 'a>(
    timer: impl Peripheral<P = T> + 'a,
) -> Result<LedcTimerDriver<'a, T>> {
    let config = TimerConfig::new()
        .frequency(SERVO_FREQUENCY_HZ.Hz())
        .resolution(SERVO_RESOLUTION);
    Ok(LedcTimerDriver::new(timer, &config)?)
}

/// ESP32のGPIOから直接サーボ制御信号を出力する（ATOMIC Motionベース経由ではない）。
///
/// 生成直後はデューティ比0（パルス無し）で、`set_angle`/`set_pulse_us`を呼ぶまで
/// サーボへ制御信号を出さない。
pub struct GpioServo<'a> {
    driver: LedcDriver<'a>,
}

impl<'a> GpioServo<'a> {
    pub fn new<C, T>(
        channel: impl Peripheral<P = C> + 'a,
        timer: &LedcTimerDriver<'a, T>,
        pin: impl Peripheral<P = impl OutputPin> + 'a,
    ) -> Result<Self>
    where
        C: LedcChannel<SpeedMode = <T as LedcTimer>::SpeedMode>,
        T: LedcTimer + 'a,
    {
        Ok(Self {
            driver: LedcDriver::new(channel, timer, pin)?,
        })
    }

    /// サーボ角度を設定する。`angle_deg` は 0.0〜180.0度（範囲外は端に丸める）。
    #[allow(dead_code)]
    pub fn set_angle(&mut self, angle_deg: f32) -> Result<()> {
        self.set_pulse_us(angle_to_pulse_us(angle_deg))
    }

    /// パルス幅（μs）を直接設定する。範囲外は500〜2500μsに丸める。
    #[allow(dead_code)]
    pub fn set_pulse_us(&mut self, pulse_us: u32) -> Result<()> {
        let pulse_us = pulse_us.clamp(MIN_PULSE_US, MAX_PULSE_US);
        let duty = pulse_us_to_duty(pulse_us, self.driver.get_max_duty());
        self.driver.set_duty(duty)?;
        Ok(())
    }

    /// 信号出力を止める（パルス無し）。サーボは保持トルクを失う。
    #[allow(dead_code)]
    pub fn stop(&mut self) -> Result<()> {
        self.driver.set_duty(0)?;
        Ok(())
    }
}

/// 角度(0.0〜180.0度)をパルス幅(μs)に線形変換する。
fn angle_to_pulse_us(angle_deg: f32) -> u32 {
    let angle = angle_deg.clamp(0.0, 180.0);
    MIN_PULSE_US + (angle / 180.0 * (MAX_PULSE_US - MIN_PULSE_US) as f32).round() as u32
}

/// パルス幅(μs)をタイマー分解能のデューティ値に変換する。
fn pulse_us_to_duty(pulse_us: u32, max_duty: u32) -> u32 {
    (pulse_us as u64 * max_duty as u64 / SERVO_PERIOD_US as u64) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angle_to_pulse_us_maps_angle_range_to_pulse_range() {
        assert_eq!(angle_to_pulse_us(0.0), 500);
        assert_eq!(angle_to_pulse_us(90.0), 1_500);
        assert_eq!(angle_to_pulse_us(180.0), 2_500);
    }

    #[test]
    fn angle_to_pulse_us_clamps_out_of_range() {
        assert_eq!(angle_to_pulse_us(-10.0), 500);
        assert_eq!(angle_to_pulse_us(200.0), 2_500);
    }

    #[test]
    fn pulse_us_to_duty_scales_by_period() {
        assert_eq!(pulse_us_to_duty(0, 16_383), 0);
        assert_eq!(pulse_us_to_duty(10_000, 16_383), 8_191);
        assert_eq!(pulse_us_to_duty(20_000, 16_383), 16_383);
    }
}
