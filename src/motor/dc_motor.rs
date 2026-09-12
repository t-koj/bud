use anyhow::Result;
use esp_idf_svc::hal::gpio::{AnyIOPin, Output, PinDriver};
use esp_idf_svc::hal::ledc::LedcDriver;

/// H ブリッジドライバ（TB6612FNG / DRV8833 等）1 チャンネル分の DC モーター制御。
///
/// `in1`/`in2` で回転方向を、`pwm` で速度を指定する構成
/// （TB6612FNG の AIN1/AIN2/PWMA に相当）を前提にしている。
pub struct DcMotor<'a> {
    pwm: LedcDriver<'a>,
    in1: PinDriver<'a, AnyIOPin, Output>,
    in2: PinDriver<'a, AnyIOPin, Output>,
}

impl<'a> DcMotor<'a> {
    pub fn new(
        pwm: LedcDriver<'a>,
        in1: PinDriver<'a, AnyIOPin, Output>,
        in2: PinDriver<'a, AnyIOPin, Output>,
    ) -> Result<Self> {
        let mut motor = Self { pwm, in1, in2 };
        motor.stop()?;
        Ok(motor)
    }

    /// 出力を設定する。`power` は -100（全力後退）〜100（全力前進）。
    pub fn set(&mut self, power: i8) -> Result<()> {
        let power = power.clamp(-100, 100);
        self.in1.set_level((power > 0).into())?;
        self.in2.set_level((power < 0).into())?;

        let max_duty = self.pwm.get_max_duty();
        let duty = max_duty * power.unsigned_abs() as u32 / 100;
        self.pwm.set_duty(duty)?;
        Ok(())
    }

    pub fn stop(&mut self) -> Result<()> {
        self.set(0)
    }
}
