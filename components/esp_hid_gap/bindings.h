#pragma once

// esp_hidh.h (esp_hid コンポーネント) は esp-idf-sys の標準 bindings.h には
// 含まれていない(標準では bluedroid 内蔵の古い esp_hidh_api.h のみが対象)ため、
// esp_hid_gap.h と合わせてここでバインディングを生成する。
#include "esp_hid_gap.h"
#include "esp_hidh.h"
// esp_hidh_init() は内部で BLE HID Host (GATTC) を初期化するが、GATTC の
// コールバックディスパッチには esp_ble_gattc_register_callback() での事前登録が要る
// (ESP-IDF公式サンプル esp_hid_host_main.c を参照)。関数宣言は esp_hidh.h に
// 含まれないため個別にincludeする。
#include "esp_hidh_gattc.h"
