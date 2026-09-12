#pragma once

// esp_hidh.h (esp_hid コンポーネント) は esp-idf-sys の標準 bindings.h には
// 含まれていない(標準では bluedroid 内蔵の古い esp_hidh_api.h のみが対象)ため、
// esp_hid_gap.h と合わせてここでバインディングを生成する。
#include "esp_hid_gap.h"
#include "esp_hidh.h"
