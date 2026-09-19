/// PS4 コントローラーから受け取る入力の状態。
///
/// スティックは -100（左/下いっぱい）〜100（右/上いっぱい）で正規化する。
/// L2/R2 のアナログ値は 0（離している）〜100（全押し）で正規化する。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct GamepadState {
    pub left_stick_x: i8,
    pub left_stick_y: i8,
    pub right_stick_x: i8,
    pub right_stick_y: i8,
    pub l2_analog: u8,
    pub r2_analog: u8,
    pub buttons: Buttons,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Buttons {
    pub cross: bool,
    pub circle: bool,
    pub square: bool,
    pub triangle: bool,
    pub dpad: Dpad,
    pub l1: bool,
    pub r1: bool,
    pub l2: bool,
    pub r2: bool,
    pub l3: bool,
    pub r3: bool,
    pub share: bool,
    pub options: bool,
}

/// 十字キーの方向。DS4のHIDレポートではhat switch形式（0〜7を時計回り、8で中央）
/// で送られてくる。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Dpad {
    #[default]
    Neutral,
    Up,
    UpRight,
    Right,
    DownRight,
    Down,
    DownLeft,
    Left,
    UpLeft,
}

pub mod bt_hid;
mod ds4_report;

/// コントローラー入力ソースの抽象化。
///
/// 実装は [`bt_hid::Ds4Gamepad`]（Bluetooth Classic HID Host 経由で PS4 コントローラーと
/// 通信するバックエンド）。
pub trait Gamepad {
    fn poll(&mut self) -> GamepadState;
}
