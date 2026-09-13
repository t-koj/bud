//! Bluetooth Classic HID Host (`esp_hidh`) を使った PS4 (DualShock 4) コントローラー接続。
//!
//! esp-idf-svc/esp-idf-hal には Bluetooth Classic HID Host のRust APIが無いため、
//! ESP-IDF公式サンプル `examples/bluetooth/esp_hid_host` の `esp_hid_gap` を
//! vendor したコンポーネント(`components/esp_hid_gap`)経由でバインディングを生成し、
//! `esp_idf_svc::sys::hid_gap::*` を直接FFI呼び出しする。
//!
//! BT GAPイベント（`ESP_BT_GAP_MODE_CHG_EVT`等）の監視は、`esp_bt_gap_register_callback`
//! を直接呼ばず、`components/esp_hid_gap`に追加した汎用フック
//! `esp_hid_gap_set_event_hook`経由で行う。`esp_bt_gap_register_callback`は
//! コールバックを1つしか保持できず、`esp_hid_gap`が内部でスキャン・ペアリング処理用に
//! 既に登録済みのため、アプリが個別に登録し直すと接続確立直後のBluetoothスタック内部
//! 処理と衝突してクラッシュする（実機で確認済み）。フック方式なら登録の奪い合いが
//! 起きないため安全（[`gap_event_hook`]参照）。

use std::ffi::{c_void, CStr};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TryRecvError};
use std::sync::OnceLock;

use esp_idf_svc::sys::hid_gap::{
    esp_ble_gattc_register_callback, esp_bt_gap_cb_event_t,
    esp_bt_gap_cb_event_t_ESP_BT_GAP_MODE_CHG_EVT, esp_bt_gap_cb_param_t, esp_hid_gap_init,
    esp_hid_gap_set_event_hook, esp_hid_scan, esp_hid_scan_result_t, esp_hid_scan_results_free,
    esp_hidh_config_t, esp_hidh_dev_name_get, esp_hidh_dev_open, esp_hidh_event_data_t,
    esp_hidh_event_t_ESP_HIDH_CLOSE_EVENT as ESP_HIDH_CLOSE_EVENT,
    esp_hidh_event_t_ESP_HIDH_INPUT_EVENT as ESP_HIDH_INPUT_EVENT,
    esp_hidh_event_t_ESP_HIDH_OPEN_EVENT as ESP_HIDH_OPEN_EVENT, esp_hidh_gattc_event_handler,
    esp_hidh_init, HIDH_BTDM_MODE,
};
use esp_idf_svc::sys::{
    esp, nvs_flash_erase, nvs_flash_init, ESP_ERR_NVS_NEW_VERSION_FOUND, ESP_ERR_NVS_NO_FREE_PAGES,
};

use super::{ds4_report, Gamepad, GamepadState};

/// Bluetooth Classic HID Host から届くイベント。
#[derive(Debug)]
pub enum GamepadEvent {
    Connected { name: String },
    Disconnected,
    /// 生のHID Inputレポート。[`ds4_report::parse`] で解析する。
    RawInput { report_id: u16, data: Vec<u8> },
    /// BluedroidスタックがBT GAPイベント`ESP_BT_GAP_MODE_CHG_EVT`（接続後のリンク
    /// ポリシー・ネゴシエーション完了）を通知したことを示す。[`gap_event_hook`]参照。
    LinkReady,
}

static EVENT_TX: OnceLock<SyncSender<GamepadEvent>> = OnceLock::new();

/// Bluetooth Classic HID Host スタックを初期化する。プロセス中で一度だけ呼び出せる。
///
/// 戻り値の `Receiver` で [`GamepadEvent`] を受け取る。実際の探索・接続は
/// [`scan_and_connect`] を（必要なら繰り返し）呼び出して行う。
pub fn init() -> anyhow::Result<Receiver<GamepadEvent>> {
    let (tx, rx) = sync_channel(16);
    EVENT_TX
        .set(tx)
        .map_err(|_| anyhow::anyhow!("bt_hid::init is called twice"))?;

    unsafe {
        // Bluedroid はキャリブレーションデータの保存にNVSを使うため、先に初期化しておく。
        let nvs_result = nvs_flash_init();
        if nvs_result == ESP_ERR_NVS_NO_FREE_PAGES || nvs_result == ESP_ERR_NVS_NEW_VERSION_FOUND {
            esp!(nvs_flash_erase())?;
            esp!(nvs_flash_init())?;
        } else {
            esp!(nvs_result)?;
        }
        log::info!("bt_hid: nvs init done");

        // esp_hid_gap.h の HID_HOST_MODE は CONFIG_BT_HID_HOST_ENABLED 等の
        // sdkconfigマクロによる条件分岐で決まるが、bindgenがこれらのマクロを
        // 認識できず常に HIDH_IDLE_MODE(0) にフォールバックしてしまう
        // （実機ログで `esp_hid_gap_init` が "Invalid mode given!" で失敗して発覚）。
        // sdkconfig.defaults で BTDM(Bluetooth Classic + BLE) を有効にしているため、
        // 無条件で定義されている HIDH_BTDM_MODE を直接指定する。
        esp!(esp_hid_gap_init(HIDH_BTDM_MODE as u8))?;
        log::info!("bt_hid: gap init done");

        // esp_hid_gapが内部で使うGAPコールバックの登録を奪わずBT GAPイベントを
        // 監視するため、汎用フック経由で相乗りする（[`gap_event_hook`]参照）。
        esp_hid_gap_set_event_hook(Some(gap_event_hook));

        // esp_hidh_init() は内部でBLE HID Host(GATTC)を初期化する際、
        // esp_ble_gattc_app_register() の完了イベント(ESP_GATTC_REG_EVT)を
        // セマフォで待つ(WAIT_CB())。このイベントはGATTCコールバックとして
        // 事前登録した esp_hidh_gattc_event_handler にしか届かないため、
        // 登録を忘れるとセマフォが永久に解放されずハングする
        // （実機ログで esp_hidh_init が返らずハングして発覚。ESP-IDF公式サンプル
        // esp_hid_host_main.c の esp_hidh_init 呼び出し前の登録に倣う）。
        esp!(esp_ble_gattc_register_callback(Some(
            esp_hidh_gattc_event_handler
        )))?;

        let config = esp_hidh_config_t {
            callback: Some(hidh_event_handler),
            event_stack_size: 4096,
            callback_arg: std::ptr::null_mut(),
        };
        esp!(esp_hidh_init(&config))?;
        log::info!("bt_hid: hidh init done");
    }

    Ok(rx)
}

/// `target_name_prefix` で始まる名前のHIDデバイスを `scan_seconds` 秒間探索し、
/// 見つかれば接続を試みる。
///
/// PS4 (DualShock 4) のBluetoothデバイス名は `"Wireless Controller"`。
/// 接続の成否はこの関数の戻り値ではなく [`GamepadEvent::Connected`] で判定する
/// （`esp_hidh_dev_open` は接続確立ではなく試行の開始のみを表すため）。
///
/// 繰り返し呼び出すことができるため、呼び出し側は「コントローラーの接続を待機する」
/// （ボンディング済みコントローラーのPSボタン再接続、および未ペアリングコントローラーの
/// SHARE+PSペアリングモードのどちらも、このスキャンで検出できる想定。実機で要検証）
/// をこの関数のリトライループとして実装できる。
pub fn scan_and_connect(target_name_prefix: &str, scan_seconds: u32) -> anyhow::Result<()> {
    log::info!("bt_hid: scanning for {scan_seconds}s...");
    unsafe {
        let mut num_results: usize = 0;
        let mut results: *mut esp_hid_scan_result_t = std::ptr::null_mut();
        esp!(esp_hid_scan(scan_seconds, &mut num_results, &mut results))?;
        log::info!("HID scan finished: {num_results} device(s) found");

        let mut target: *mut esp_hid_scan_result_t = std::ptr::null_mut();
        let mut cursor = results;
        while !cursor.is_null() {
            let r = &*cursor;
            let name = if r.name.is_null() {
                "(no name)".to_string()
            } else {
                CStr::from_ptr(r.name).to_string_lossy().into_owned()
            };
            log::info!("  found HID device: {name}");
            if name.starts_with(target_name_prefix) {
                target = cursor;
            }
            cursor = r.next;
        }

        if let Some(target) = target.as_ref() {
            log::info!("connecting to matching HID device...");
            // esp_hidh_dev_open は esp_err_t ではなく esp_hidh_dev_t* を返す。
            // 接続の成否は esp_hidh_dev_open 自体ではなく、この後の
            // ESP_HIDH_OPEN_EVENT コールバックで判定する。
            // NOTE: `ble.addr_type` は Classic BT デバイスに対しては無視される
            // (esp_hidh_dev_open の第3引数は BLE 接続時のみ使用)。
            let dev = esp_hidh_dev_open(
                target.bda.as_ptr() as *mut _,
                target.transport,
                target.__bindgen_anon_1.ble.addr_type as u8,
            );
            if dev.is_null() {
                log::error!("esp_hidh_dev_open failed");
            }
        } else {
            log::warn!("no HID device matching prefix '{target_name_prefix}' found");
        }

        if !results.is_null() {
            esp_hid_scan_results_free(results);
        }
    }

    Ok(())
}

/// `components/esp_hid_gap`の内部GAPコールバックから呼ばれるフック。
/// `ESP_BT_GAP_MODE_CHG_EVT`（接続後のリンクポリシー・ネゴシエーション完了）を
/// [`GamepadEvent::LinkReady`]としてチャネルに流す。
///
/// `esp_bt_gap_register_callback`で登録し直す方式は、`esp_hid_gap`が内部で
/// スキャン・ペアリング処理用に既にコールバックを登録済みであるため、
/// 上書き登録すると接続確立直後のBluetoothスタック内部処理と衝突しクラッシュする
/// （実機で確認済み）。この関数は登録の奪い合いが起きないフック機構
/// （`esp_hid_gap_set_event_hook`）経由で呼ばれるため安全。
unsafe extern "C" fn gap_event_hook(event: esp_bt_gap_cb_event_t, _param: *mut esp_bt_gap_cb_param_t) {
    if event == esp_bt_gap_cb_event_t_ESP_BT_GAP_MODE_CHG_EVT {
        if let Some(tx) = EVENT_TX.get() {
            let _ = tx.try_send(GamepadEvent::LinkReady);
        }
    }
}

unsafe extern "C" fn hidh_event_handler(
    _handler_args: *mut c_void,
    _base: esp_idf_svc::sys::hid_gap::esp_event_base_t,
    id: i32,
    event_data: *mut c_void,
) {
    let Some(tx) = EVENT_TX.get() else {
        return;
    };
    let param = &*(event_data as *mut esp_hidh_event_data_t);

    if id == ESP_HIDH_OPEN_EVENT {
        let open = param.open;
        let name = if open.status == 0 {
            let raw = esp_hidh_dev_name_get(open.dev);
            if raw.is_null() {
                "(unknown)".to_string()
            } else {
                CStr::from_ptr(raw).to_string_lossy().into_owned()
            }
        } else {
            log::error!("HID open failed: status={}", open.status);
            return;
        };
        let _ = tx.try_send(GamepadEvent::Connected { name });
    } else if id == ESP_HIDH_INPUT_EVENT {
        let input = param.input;
        let data = std::slice::from_raw_parts(input.data, input.length as usize).to_vec();
        let _ = tx.try_send(GamepadEvent::RawInput {
            report_id: input.report_id,
            data,
        });
    } else if id == ESP_HIDH_CLOSE_EVENT {
        let _ = tx.try_send(GamepadEvent::Disconnected);
    }
}

/// スティックの値がこの幅以上変化したときだけログ出力する（ノイズによる微小変化での
/// ログ連発を避けるため。詳細は[design/led.md](../../docs/design/led.md)参照）。
const STICK_LOG_THRESHOLD: i8 = 10;

/// [`GamepadEvent`] を受信して [`GamepadState`] を更新する、DS4用の [`Gamepad`] 実装。
pub struct Ds4Gamepad {
    rx: Receiver<GamepadEvent>,
    last_state: GamepadState,
    connected: bool,
    /// [`GamepadEvent::LinkReady`]を受信済みかどうか（[`is_operation_ready`]参照）。
    operation_ready: bool,
    /// DS4のBT Inputレポートのバイトオフセットが未検証([`ds4_report`]参照)なため、
    /// 最初の数件は生バイト列をログに出して実機での解析確認に使う。
    raw_input_log_budget: u8,
    /// 直近ログ出力した左右スティックのY値（`STICK_LOG_THRESHOLD`判定用）。
    last_logged_stick_y: (i8, i8),
}

impl Ds4Gamepad {
    pub fn new(rx: Receiver<GamepadEvent>) -> Self {
        Self {
            rx,
            last_state: GamepadState::default(),
            connected: false,
            operation_ready: false,
            raw_input_log_budget: 5,
            last_logged_stick_y: (0, 0),
        }
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// BluedroidスタックのSniffモード遷移等のリンクポリシー・ネゴシエーションが
    /// 完了し、実際の操作が安定して届くようになった目安を返す
    /// （[docs/design/led.md](../../docs/design/led.md)参照）。
    pub fn is_operation_ready(&self) -> bool {
        self.operation_ready
    }
}

impl Gamepad for Ds4Gamepad {
    fn poll(&mut self) -> GamepadState {
        loop {
            match self.rx.try_recv() {
                Ok(GamepadEvent::Connected { name }) => {
                    log::info!("gamepad connected: {name}");
                    self.connected = true;
                }
                Ok(GamepadEvent::Disconnected) => {
                    log::info!("gamepad disconnected");
                    self.connected = false;
                    self.operation_ready = false;
                    self.last_state = GamepadState::default();
                }
                Ok(GamepadEvent::LinkReady) => {
                    log::info!("BT link ready (mode change event received)");
                    self.operation_ready = true;
                }
                Ok(GamepadEvent::RawInput { report_id, data }) => {
                    let should_log = self.raw_input_log_budget > 0;
                    if should_log {
                        self.raw_input_log_budget -= 1;
                        log::info!("raw input report_id={report_id:#x} data={data:02x?}");
                    }
                    match ds4_report::parse(report_id, &data) {
                        Some(state) => {
                            if state.buttons != self.last_state.buttons {
                                log::info!("buttons changed: {:?}", state.buttons);
                            }
                            let (last_left_y, last_right_y) = self.last_logged_stick_y;
                            let left_diff = state.left_stick_y as i16 - last_left_y as i16;
                            let right_diff = state.right_stick_y as i16 - last_right_y as i16;
                            if left_diff.abs() >= STICK_LOG_THRESHOLD as i16
                                || right_diff.abs() >= STICK_LOG_THRESHOLD as i16
                            {
                                log::info!(
                                    "stick moved: left_y={} right_y={}",
                                    state.left_stick_y,
                                    state.right_stick_y
                                );
                                self.last_logged_stick_y = (state.left_stick_y, state.right_stick_y);
                            }
                            self.last_state = state;
                        }
                        None if should_log => {
                            log::warn!("unrecognized report_id={report_id:#x} len={}", data.len());
                        }
                        None => {}
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
        self.last_state
    }
}
