use anyhow::Result;
use esp_idf_svc::hal::i2c::I2cDriver;

/// ATOMIC Motionベース v1.2 のI2Cアドレス（固定）。
const I2C_ADDR: u8 = 0x38;

/// I2C書き込みのタイムアウト。
const I2C_TIMEOUT_MS: u32 = 100;

/// ATOMIC Motionベース v1.2（DCモーター2ch・サーボ4ch、I2C制御）へのアクセス。
///
/// TB6612FNG等の直結ドライバICと異なり、モーター/サーボはESP32のGPIOに直結せず、
/// ATOMIC Motionベース上のSTM32がI2Cレジスタ書き込みでPWM/方向を生成する
/// （公式Arduinoライブラリ `m5stack/M5Atomic-Motion` の `I2C_Class::writeByte` と
/// 同一プロトコル: `[レジスタ番号, データ]` の2バイトを1トランザクションで送る）。
pub struct AtomicMotion<'a> {
    i2c: I2cDriver<'a>,
}

impl<'a> AtomicMotion<'a> {
    pub fn new(i2c: I2cDriver<'a>) -> Self {
        Self { i2c }
    }

    /// DCモーター速度を設定する。`channel` は 0 か 1。
    /// `speed` は -100（全力後退）〜100（全力前進）
    /// （デバイス上限は-127〜127だが、本プロジェクトのスティック入力規約に合わせる）。
    ///
    /// 現時点ではどのスティック/ボタンにも割り当てていない（`main.rs`未使用）ため、
    /// DCモーター制御自体の実装([spec.md](../../docs/spec.md)の機能要件)を残しつつ警告を抑止する。
    #[allow(dead_code)]
    pub fn set_motor_speed(&mut self, channel: u8, speed: i8) -> Result<()> {
        let speed = speed.clamp(-100, 100);
        self.write_register(motor_speed_register(channel)?, speed as u8)
    }

    /// サーボ角度を設定する。`channel` は 0〜3。`angle_deg` は 0.0〜180.0度。
    pub fn set_servo_angle(&mut self, channel: u8, angle_deg: f32) -> Result<()> {
        let angle = angle_deg.clamp(0.0, 180.0) as u8;
        self.write_register(servo_angle_register(channel)?, angle)
    }

    fn write_register(&mut self, register: u8, data: u8) -> Result<()> {
        self.i2c
            .write(I2C_ADDR, &[register, data], I2C_TIMEOUT_MS)?;
        Ok(())
    }
}

/// DCモーターchannel(0/1)を速度レジスタ番号(0x20/0x21)に変換する。
fn motor_speed_register(channel: u8) -> Result<u8> {
    if channel > 1 {
        anyhow::bail!("motor channel must be 0 or 1, got {channel}");
    }
    Ok(0x20 + channel)
}

/// サーボchannel(0〜3)を角度レジスタ番号(0x00〜0x03)に変換する。
fn servo_angle_register(channel: u8) -> Result<u8> {
    if channel > 3 {
        anyhow::bail!("servo channel must be 0..=3, got {channel}");
    }
    Ok(channel)
}

/// スティック値(-100〜100)をサーボ角度(0.0〜180.0度)に変換する。
/// -100→0度、0→90度（中央）、100→180度に線形マッピングする。
pub fn stick_to_servo_angle(stick: i8) -> f32 {
    (stick as f32 + 100.0) / 200.0 * 180.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motor_speed_register_maps_channel_to_register() {
        assert_eq!(motor_speed_register(0).unwrap(), 0x20);
        assert_eq!(motor_speed_register(1).unwrap(), 0x21);
        assert!(motor_speed_register(2).is_err());
    }

    #[test]
    fn servo_angle_register_maps_channel_to_register() {
        assert_eq!(servo_angle_register(0).unwrap(), 0x00);
        assert_eq!(servo_angle_register(3).unwrap(), 0x03);
        assert!(servo_angle_register(4).is_err());
    }

    #[test]
    fn stick_to_servo_angle_maps_stick_range_to_angle_range() {
        assert_eq!(stick_to_servo_angle(-100), 0.0);
        assert_eq!(stick_to_servo_angle(0), 90.0);
        assert_eq!(stick_to_servo_angle(100), 180.0);
    }
}
