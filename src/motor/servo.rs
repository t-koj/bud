use anyhow::Result;
use esp_idf_svc::hal::ledc::LedcDriver;

/// 標準的なホビーサーボ（PWM 50Hz、パルス幅 1.0ms〜2.0ms で 0〜180度）。
pub struct Servo<'a> {
    pwm: LedcDriver<'a>,
    min_duty: u32,
    max_duty: u32,
}

const PERIOD_MS: f32 = 1000.0 / 50.0; // LEDC タイマーは 50Hz で設定する前提
const MIN_PULSE_MS: f32 = 1.0;
const MAX_PULSE_MS: f32 = 2.0;

impl<'a> Servo<'a> {
    pub fn new(pwm: LedcDriver<'a>) -> Self {
        let full_duty = pwm.get_max_duty() as f32;
        let min_duty = (full_duty * MIN_PULSE_MS / PERIOD_MS) as u32;
        let max_duty = (full_duty * MAX_PULSE_MS / PERIOD_MS) as u32;
        Self {
            pwm,
            min_duty,
            max_duty,
        }
    }

    /// 角度を 0〜180度 で指定する。
    pub fn set_angle_deg(&mut self, angle: f32) -> Result<()> {
        let angle = angle.clamp(0.0, 180.0);
        let duty = self.min_duty
            + ((self.max_duty - self.min_duty) as f32 * angle / 180.0) as u32;
        self.pwm.set_duty(duty)?;
        Ok(())
    }
}
