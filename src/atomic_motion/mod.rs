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
    ///
    /// 公式Arduinoライブラリ`m5stack/M5Atomic-Motion`の`setServoAngle`
    /// （`_i2c.writeByte(_addr, reg, angle)`）と同一のプロトコル。
    pub fn set_servo_angle(&mut self, channel: u8, angle_deg: f32) -> Result<()> {
        let angle = angle_deg.clamp(0.0, 180.0) as u8;
        self.write_register(servo_angle_register(channel)?, angle)
    }

    pub fn get_servo_angle(&mut self, channel: u8) -> Result<f32> {
        Ok(self.read_register(servo_angle_register(channel)?)? as f32)
    }

    /// サーボパルス幅（μs）を取得する。`channel` は 0〜3。
    pub fn get_servo_pulse(&mut self, channel: u8) -> Result<u16> {
        let register = servo_pulse_register(channel)?;
        let mut buf = [0u8; 2];
        self.i2c
            .write_read(I2C_ADDR, &[register], &mut buf, I2C_TIMEOUT_MS)?;
        Ok(((buf[0] as u16) << 8) + buf[1] as u16)
    }

    /// サーボパルス幅（μs）を設定する。`channel` は 0〜3。
    pub fn set_servo_pulse(&mut self, channel: u8, width_us: u16) -> Result<()> {
        let register = servo_pulse_register(channel)?;
        let data = [register, (width_us >> 8) as u8, (width_us & 0xFF) as u8];
        self.i2c.write(I2C_ADDR, &data, I2C_TIMEOUT_MS)?;
        Ok(())
    }

    fn write_register(&mut self, register: u8, data: u8) -> Result<()> {
        self.i2c
            .write(I2C_ADDR, &[register, data], I2C_TIMEOUT_MS)?;
        Ok(())
    }

    fn read_register(&mut self, register: u8) -> Result<u8> {
        let mut buf = [0u8; 1];
        self.i2c.write_read(I2C_ADDR, &[register], &mut buf, I2C_TIMEOUT_MS)?;
        Ok(buf[0])
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

/// サーボchannel(0〜3)をパルス幅レジスタ番号(0x10, 0x12, 0x14, 0x16)に変換する。
fn servo_pulse_register(channel: u8) -> Result<u8> {
    if channel > 3 {
        anyhow::bail!("servo channel must be 0..=3, got {channel}");
    }
    Ok(2 * channel | 0x10)
}

/// サーボパルス幅の下限・上限（μs）。SG90系の0.5〜2.5msに合わせる。
const SERVO_PULSE_MIN_US: u16 = 500;
const SERVO_PULSE_MAX_US: u16 = 2_500;

/// サーボ中央パルス幅（μs）の初期値。
pub const SERVO_CENTER_DEFAULT_US: u16 = 1_500;

/// 十字キー1回の押下で中央パルス幅を動かす量（μs）。
pub const SERVO_CENTER_STEP_US: u16 = 10;

/// スティック値(-100〜100)をサーボパルス幅(μs)に変換する。
/// 0→`center_us`、-100→下限、100→上限とし、中央の両側を別々に線形マッピングする。
/// 中央を動かしても可動範囲の端点を変えないため、片側だけ傾きが変わる。
pub fn stick_to_servo_pulse(stick: i8, center_us: u16) -> u16 {
    let stick = stick.clamp(-100, 100) as i32;
    let center = center_us.clamp(SERVO_PULSE_MIN_US, SERVO_PULSE_MAX_US) as i32;
    let span = if stick >= 0 {
        SERVO_PULSE_MAX_US as i32 - center
    } else {
        center - SERVO_PULSE_MIN_US as i32
    };
    (center + stick * span / 100) as u16
}

/// サーボ振幅(%)の初期値・調整量・範囲。負の値で稼働方向を反転する。
pub const SERVO_GAIN_DEFAULT_PERCENT: i32 = 100;
pub const SERVO_GAIN_STEP_PERCENT: i32 = 10;
const SERVO_GAIN_LIMIT_PERCENT: i32 = 100;

/// スティック値(-100〜100)に振幅`gain_percent`(-100〜100)を掛ける。
pub fn apply_servo_gain(stick: i8, gain_percent: i32) -> i8 {
    let gain = gain_percent.clamp(-SERVO_GAIN_LIMIT_PERCENT, SERVO_GAIN_LIMIT_PERCENT);
    (stick.clamp(-100, 100) as i32 * gain / 100) as i8
}

/// 振幅に`delta_percent`を加え、有効範囲に収めて返す。
pub fn adjust_servo_gain(gain_percent: i32, delta_percent: i32) -> i32 {
    (gain_percent + delta_percent).clamp(-SERVO_GAIN_LIMIT_PERCENT, SERVO_GAIN_LIMIT_PERCENT)
}

/// 中央パルス幅に`delta_us`を加え、有効範囲に収めて返す。
pub fn adjust_servo_center(center_us: u16, delta_us: i32) -> u16 {
    (center_us as i32 + delta_us).clamp(SERVO_PULSE_MIN_US as i32, SERVO_PULSE_MAX_US as i32)
        as u16
}

pub fn stick_to_servo_speed(stick: i8) -> f32 {
    (stick as f32 + 100.0) / 200.0 * 180.0
}

/// スティックの「遊び」（デッドゾーン）幅。中央からこの範囲内の入力は0として扱う。
/// 左右スティックのX/Y軸すべてで共通の値を使う。
const DEADZONE: i8 = 10;

/// スティック値(-100〜100)にデッドゾーンを適用した値へのマッピングテーブル。
/// index 0〜200 が入力値 -100〜100 に対応する。計算式ではなく事前計算した
/// テーブル参照にすることで、将来デッドゾーン形状を非線形カーブ等に変更する場合も
/// テーブル生成部のみの変更で済むようにしている。
const STICK_DEADZONE_MAP: [i8; 201] = {
    let mut map = [0i8; 201];
    let mut i = 0;
    while i < map.len() {
        let input = i as i16 - 100;
        let magnitude = input.abs();
        map[i] = if magnitude <= DEADZONE as i16 {
            0
        } else {
            let scaled = (magnitude - DEADZONE as i16) * 100 / (100 - DEADZONE as i16);
            let scaled = if scaled > 100 { 100 } else { scaled };
            (if input < 0 { -scaled } else { scaled }) as i8
        };
        i += 1;
    }
    map
};

/// スティック値(-100〜100)にデッドゾーンを適用した値を返す。
pub fn apply_stick_deadzone(stick: i8) -> i8 {
    let stick = stick.clamp(-100, 100) as i16;
    STICK_DEADZONE_MAP[(stick + 100) as usize]
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
    fn stick_to_servo_pulse_maps_stick_range_to_pulse_range() {
        assert_eq!(stick_to_servo_pulse(-100, 1_500), 500);
        assert_eq!(stick_to_servo_pulse(0, 1_500), 1_500);
        assert_eq!(stick_to_servo_pulse(100, 1_500), 2_500);
    }

    #[test]
    fn stick_to_servo_pulse_moves_neutral_and_keeps_endpoints() {
        assert_eq!(stick_to_servo_pulse(0, 1_600), 1_600);
        assert_eq!(stick_to_servo_pulse(-100, 1_600), 500);
        assert_eq!(stick_to_servo_pulse(100, 1_600), 2_500);
        assert_eq!(stick_to_servo_pulse(50, 1_500), 2_000);
    }

    #[test]
    fn adjust_servo_center_clamps_to_valid_range() {
        assert_eq!(adjust_servo_center(1_500, 10), 1_510);
        assert_eq!(adjust_servo_center(1_500, -10), 1_490);
        assert_eq!(adjust_servo_center(2_495, 10), 2_500);
        assert_eq!(adjust_servo_center(505, -10), 500);
    }

    #[test]
    fn apply_servo_gain_scales_and_reverses() {
        assert_eq!(apply_servo_gain(100, 100), 100);
        assert_eq!(apply_servo_gain(100, 50), 50);
        assert_eq!(apply_servo_gain(-100, 50), -50);
        assert_eq!(apply_servo_gain(100, -100), -100);
        assert_eq!(apply_servo_gain(40, -50), -20);
        assert_eq!(apply_servo_gain(100, 0), 0);
    }

    #[test]
    fn adjust_servo_gain_clamps_to_valid_range() {
        assert_eq!(adjust_servo_gain(100, 10), 100);
        assert_eq!(adjust_servo_gain(0, -10), -10);
        assert_eq!(adjust_servo_gain(-95, -10), -100);
    }

    #[test]
    fn apply_stick_deadzone_zeroes_values_within_deadzone() {
        assert_eq!(apply_stick_deadzone(0), 0);
        assert_eq!(apply_stick_deadzone(10), 0);
        assert_eq!(apply_stick_deadzone(-10), 0);
    }

    #[test]
    fn apply_stick_deadzone_rescales_values_outside_deadzone() {
        assert_eq!(apply_stick_deadzone(11), 1);
        assert_eq!(apply_stick_deadzone(-11), -1);
    }

    #[test]
    fn apply_stick_deadzone_preserves_extremes() {
        assert_eq!(apply_stick_deadzone(100), 100);
        assert_eq!(apply_stick_deadzone(-100), -100);
    }
}
