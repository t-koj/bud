/// PS4 コントローラーから受け取る入力の状態。
///
/// スティックは -100（左/下いっぱい）〜100（右/上いっぱい）で正規化する。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct GamepadState {
    pub left_stick_x: i8,
    pub left_stick_y: i8,
    pub right_stick_x: i8,
    pub right_stick_y: i8,
    pub buttons: Buttons,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Buttons {
    pub cross: bool,
    pub circle: bool,
    pub square: bool,
    pub triangle: bool,
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
