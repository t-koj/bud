use super::{Buttons, GamepadState};

/// DualShock4 が Bluetooth Classic 接続時に送る HID Input レポート（Report ID `0x01`、
/// 9バイトの簡易フォーマット）を [`GamepadState`] に変換する。
///
/// 実機での接続確認により判明: 当初BT拡張レポート(Report ID `0x11`, 78バイト)を
/// 想定していたが、実際には `esp_hidh` 経由で届くのは Report ID `0x01` の9バイト
/// レポートだった（DS4をBT拡張モードへ切り替えるための追加のfeature report送信を
/// 行っていないため、簡易フォーマットのまま送られてくると見られる）。
///
/// `data` は `esp_hidh` から届く生バイト列（Report IDを含まない、
/// [`crate::gamepad::bt_hid::GamepadEvent::RawInput`] の `data` フィールド）を想定する。
/// レイアウト（実機キャプチャで確認済み）:
///
/// | offset | 内容 |
/// | --- | --- |
/// | 0 | 左スティックX |
/// | 1 | 左スティックY |
/// | 2 | 右スティックX |
/// | 3 | 右スティックY |
/// | 4 | bit0-3: D-pad方向(8=中央), bit4-7: Square/Cross/Circle/Triangle |
/// | 5 | L1/R1/L2/R2/Share/Options/L3/R3（本プロジェクトでは未使用） |
/// | 6 | bit0: PS(HOME), bit1: Touchpad, bit2-7: カウンタ（本プロジェクトでは未使用） |
/// | 7 | L2アナログ値（本プロジェクトでは未使用） |
/// | 8 | R2アナログ値（本プロジェクトでは未使用） |
pub fn parse(report_id: u16, data: &[u8]) -> Option<GamepadState> {
    const BT_INPUT_REPORT_ID: u16 = 0x01;
    const MIN_LEN: usize = 5;

    if report_id != BT_INPUT_REPORT_ID || data.len() < MIN_LEN {
        return None;
    }

    let buttons_byte = data[4];
    Some(GamepadState {
        left_stick_x: axis_from_raw(data[0], false),
        left_stick_y: axis_from_raw(data[1], true),
        right_stick_x: axis_from_raw(data[2], false),
        right_stick_y: axis_from_raw(data[3], true),
        buttons: Buttons {
            square: buttons_byte & 0x10 != 0,
            cross: buttons_byte & 0x20 != 0,
            circle: buttons_byte & 0x40 != 0,
            triangle: buttons_byte & 0x80 != 0,
        },
    })
}

/// 生値(0〜255, 中央128)を -100〜100 に変換する。
/// `invert` は DS4 の Y軸（下方向が値の増加方向）を、本プロジェクトの規約
/// （[`GamepadState`] の doc comment: 上/右が+100）に合わせて反転させるために使う。
fn axis_from_raw(raw: u8, invert: bool) -> i8 {
    let centered = raw as i16 - 128;
    let scaled = (centered * 100 / 127).clamp(-100, 100);
    let scaled = if invert { -scaled } else { scaled };
    scaled as i8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn neutral_report() -> Vec<u8> {
        vec![128, 128, 128, 128, 0x08, 0, 0, 0, 0]
    }

    #[test]
    fn wrong_report_id_returns_none() {
        assert_eq!(parse(0x11, &neutral_report()), None);
    }

    #[test]
    fn too_short_returns_none() {
        let short = vec![128, 128, 128, 128];
        assert_eq!(parse(0x01, &short), None);
    }

    #[test]
    fn centered_sticks_are_neutral() {
        let state = parse(0x01, &neutral_report()).unwrap();
        assert_eq!(state.left_stick_x, 0);
        assert_eq!(state.left_stick_y, 0);
        assert_eq!(state.right_stick_x, 0);
        assert_eq!(state.right_stick_y, 0);
        assert_eq!(state.buttons, Buttons::default());
    }

    #[test]
    fn circle_bit_sets_circle_button_only() {
        let mut data = neutral_report();
        data[4] = 0x08 | 0x40;
        let state = parse(0x01, &data).unwrap();
        assert_eq!(
            state.buttons,
            Buttons {
                circle: true,
                ..Buttons::default()
            }
        );
    }

    #[test]
    fn all_shape_buttons_combination() {
        let mut data = neutral_report();
        data[4] = 0x08 | 0xF0;
        let state = parse(0x01, &data).unwrap();
        assert_eq!(
            state.buttons,
            Buttons {
                square: true,
                cross: true,
                circle: true,
                triangle: true,
            }
        );
    }

    #[test]
    fn stick_extremes_clamp_to_100() {
        let mut data = neutral_report();
        data[0] = 255; // left_stick_x -> +100 (右)
        data[1] = 0; // left_stick_y raw最小(上いっぱい) -> 反転して+100
        data[2] = 0; // right_stick_x raw最小(左いっぱい) -> -100
        data[3] = 255; // right_stick_y raw最大(下いっぱい) -> 反転して-100
        let state = parse(0x01, &data).unwrap();
        assert_eq!(state.left_stick_x, 100);
        assert_eq!(state.left_stick_y, 100);
        assert_eq!(state.right_stick_x, -100);
        assert_eq!(state.right_stick_y, -100);
    }
}
