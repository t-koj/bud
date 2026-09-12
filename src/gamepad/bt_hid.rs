//! Bluetooth Classic HID Host (`esp_hidh`) を使った PS4 (DualShock 4) コントローラー接続。
//!
//! esp-idf-svc/esp-idf-hal には Bluetooth Classic HID Host のRust APIが無いため、
//! ESP-IDF公式サンプル `examples/bluetooth/esp_hid_host` の `esp_hid_gap` を
//! vendor したコンポーネント(`components/esp_hid_gap`)経由でバインディングを生成し、
//! `esp_idf_svc::sys::hid_gap::*` を直接FFI呼び出しする。
//!
//! DualShock4のHID Inputレポートのバイト単位パース(ボタン/スティックの構造化)は
//! 実機で値を確認してから実装する方針とし、ここでは生バイト列をそのままイベントで
//! 渡すところまでを実装する。

use std::ffi::{c_void, CStr};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::OnceLock;

use esp_idf_svc::sys::hid_gap::{
    esp_hid_gap_init, esp_hid_scan, esp_hid_scan_result_t, esp_hid_scan_results_free,
    esp_hidh_config_t, esp_hidh_dev_name_get, esp_hidh_dev_open, esp_hidh_event_data_t,
    esp_hidh_event_t_ESP_HIDH_CLOSE_EVENT as ESP_HIDH_CLOSE_EVENT,
    esp_hidh_event_t_ESP_HIDH_INPUT_EVENT as ESP_HIDH_INPUT_EVENT,
    esp_hidh_event_t_ESP_HIDH_OPEN_EVENT as ESP_HIDH_OPEN_EVENT, esp_hidh_init, HID_HOST_MODE,
};
use esp_idf_svc::sys::{
    esp, nvs_flash_erase, nvs_flash_init, ESP_ERR_NVS_NEW_VERSION_FOUND, ESP_ERR_NVS_NO_FREE_PAGES,
};

/// Bluetooth Classic HID Host から届くイベント。
#[derive(Debug)]
pub enum GamepadEvent {
    Connected { name: String },
    Disconnected,
    /// 生のHID Inputレポート。DualShock4のレポート解析は次のステップで実装する。
    RawInput { report_id: u16, data: Vec<u8> },
}

static EVENT_TX: OnceLock<SyncSender<GamepadEvent>> = OnceLock::new();

/// Bluetooth Classic HID Host を初期化し、`target_name_prefix` で始まる名前の
/// HIDデバイスを `scan_seconds` 秒間探索して見つかれば接続する。
///
/// PS4 (DualShock 4) のBluetoothデバイス名は `"Wireless Controller"`。
/// この関数はプロセス中で一度だけ呼び出せる。
pub fn init_and_connect(
    target_name_prefix: &str,
    scan_seconds: u32,
) -> anyhow::Result<Receiver<GamepadEvent>> {
    let (tx, rx) = sync_channel(16);
    EVENT_TX
        .set(tx)
        .map_err(|_| anyhow::anyhow!("bt_hid::init_and_connect is called twice"))?;

    unsafe {
        // Bluedroid はキャリブレーションデータの保存にNVSを使うため、先に初期化しておく。
        let nvs_result = nvs_flash_init();
        if nvs_result == ESP_ERR_NVS_NO_FREE_PAGES || nvs_result == ESP_ERR_NVS_NEW_VERSION_FOUND {
            esp!(nvs_flash_erase())?;
            esp!(nvs_flash_init())?;
        } else {
            esp!(nvs_result)?;
        }

        esp!(esp_hid_gap_init(HID_HOST_MODE as u8))?;

        let config = esp_hidh_config_t {
            callback: Some(hidh_event_handler),
            event_stack_size: 4096,
            callback_arg: std::ptr::null_mut(),
        };
        esp!(esp_hidh_init(&config))?;

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

    Ok(rx)
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
