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

/// [`stick_to_servo_angle`]に、チャンネルごとのニュートラル点トリム`trim_deg`を加えて
/// 0.0〜180.0度にクランプする。
///
/// 180度（位置決め）サーボは90度がどの位置か個体差があっても実害がないため
/// トリム不要（`trim_deg = 0.0`でよい）。一方、360度連続回転サーボは
/// 「90度=停止」という前提で動作するが、実際の停止点（ニュートラル点）には
/// 個体差があり90度ちょうどとは限らない（[Servo Kit 360°](https://docs.m5stack.com/en/accessory/servo_kit_360)
/// 公式ドキュメントも個体ごとの実験確認を求めている）。停止点がずれていると
/// スティック中央（停止のつもり）でもサーボが微回転し続け、電流を消費し続ける。
/// `trim_deg`は実機でスティック中央時に回転が止まる値を探して設定する。
pub fn stick_to_servo_angle_with_trim(stick: i8, trim_deg: f32) -> f32 {
    (stick_to_servo_angle(stick) + trim_deg).clamp(0.0, 180.0)
}

/// `current`から`target`へ向けて、1フレームあたり最大`max_step_deg`度だけ変化させた
/// 値を返す（差が`max_step_deg`以下ならそのまま`target`を返す）。
///
/// 実機で、360度連続回転サーボ(S2/S4)へスティックの大きな傾き（中央から端まで一気に
/// 変化する値）をそのまま送ると、サーボが数百ms〜1秒程度の周期でしか反応せず、
/// ぎくしゃくと回転する現象が観測された。180度サーボ(S1/S3)と360度サーボは同じ
/// SG90系のPWM仕様（50Hz/0.5〜2.5ms）を持つ同種の回路と考えられ、レジスタや
/// PWM値域を変えても症状が変わらなかったことから、サーボ側のアナログ制御回路が
/// 急激なPWM変化に追従できず不安定になっている可能性が高いと判断した。目標値へ
/// 一気に飛ばさずなめらかに追従させることで、この不安定さを回避する狙い。
pub fn ramp_toward(current: f32, target: f32, max_step_deg: f32) -> f32 {
    let diff = target - current;
    if diff.abs() <= max_step_deg {
        target
    } else if diff > 0.0 {
        current + max_step_deg
    } else {
        current - max_step_deg
    }
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
    STICK_DEADZONE_MAP[(stick as i16 + 100) as usize]
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

    #[test]
    fn stick_to_servo_angle_with_trim_shifts_neutral_point() {
        assert_eq!(stick_to_servo_angle_with_trim(0, 5.0), 95.0);
        assert_eq!(stick_to_servo_angle_with_trim(0, -5.0), 85.0);
    }

    #[test]
    fn stick_to_servo_angle_with_trim_clamps_to_valid_range() {
        assert_eq!(stick_to_servo_angle_with_trim(100, 10.0), 180.0);
        assert_eq!(stick_to_servo_angle_with_trim(-100, -10.0), 0.0);
    }

    #[test]
    fn ramp_toward_reaches_target_when_within_max_step() {
        assert_eq!(ramp_toward(90.0, 95.0, 10.0), 95.0);
        assert_eq!(ramp_toward(90.0, 90.0, 10.0), 90.0);
    }

    #[test]
    fn ramp_toward_clamps_to_max_step_when_target_far() {
        assert_eq!(ramp_toward(90.0, 180.0, 10.0), 100.0);
        assert_eq!(ramp_toward(90.0, 0.0, 10.0), 80.0);
    }
}
