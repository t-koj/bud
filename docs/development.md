# 開発環境

## ESP-IDF

* バージョン: release/v5.3
* 対象チップ: esp32

## Rust プロジェクト（本リポジトリの実装）

* [design](./design.md) の方針に従い、`esp-rs/esp-idf-template` 相当の構成
  （ターゲット/ビルド設定は `esp-rs/esp-idf-template` を `cargo generate` で
  展開した場合と同等）を採用する。
* 対象チップ: esp32（ESP32-Pico-D4。`.cargo/config.toml` の `MCU` / `target` で
  `xtensa-esp32-espidf` を指定）
* PS4 (DualShock 4) コントローラーは Bluetooth Classic の HID Host で接続する。
  ESP32-Pico（オリジナルESP32）は Bluetooth Classic + BLE のデュアルモードに対応するが、
  ESP32-S3 等の後継チップは BLE のみで Classic 非対応な点に注意（機種選定時の制約）。
* ESP-IDF は embuild が `ESP_IDF_TOOLS_INSTALL_DIR=workspace`（`.cargo/config.toml`
  で設定）によりプロジェクト配下へ自動取得・管理する。
  [development/setup.md](development/setup.md) の C言語プロジェクト向け手順で
  `~/esp/esp-idf` に取得する ESP-IDF（C言語実装用）とは別管理であり、
  バージョンも `ESP_IDF_VERSION`（`.cargo/config.toml`）で個別に指定する。
* `src/gamepad/bt_hid.rs` は Bluetooth Classic HID Host (`esp_hidh`) 経由での
  PS4コントローラー探索・接続（`init`/`scan_and_connect`）と、DS4のHID Inputレポート
  を解析して`Gamepad`トレイトを実装する `Ds4Gamepad` を提供し、`main.rs` から
  呼び出している。`cargo build` でのビルド成功は確認済み（`cargo test`は
  ハードウェア接続が必要なため未検証。[testing.md](testing.md)参照）。

### GPIO割り当て（ATOM Matrix v1.1、暫定）

対象ボードは ATOM Matrix v1.1 に決定。ATOM Matrix/ATOM Lite ともESP32-Pico-D4を
使用しており、内蔵フラッシュ用にGPIO6〜11を使用しているため、それ以外の用途には
使わない。

| 用途 | GPIO | 備考 |
| --- | --- | --- |
| I2C SDA (ATOMIC Motionベースへ) | GPIO32 | ATOM MatrixのGroveポート固定配線 |
| I2C SCL (ATOMIC Motionベースへ) | GPIO26 | 同上 |
| オンボードRGB LED (WS2812C, RMT) | GPIO27 | ATOM Matrix/Lite共通の固定配線 |

ATOM Lite を使う場合はI2CがSDA=GPIO25, SCL=GPIO21に変わる点に注意（LEDは同じGPIO27）。
以前のTB6612FNG等のGPIO直結ドライバIC想定の配線（GPIO25/33/14/13/4等）は、実機の
ATOMIC MotionベースがI2C制御のボードだったため廃止した
（詳細: [design/motor.md](design/motor.md)）。

### 構成

* `src/motor/` — ATOMIC Motionベース v1.2 (I2C, アドレス`0x38`) 経由のDCモーター・
  サーボ制御（`AtomicMotion`型）。詳細は [design/motor.md](design/motor.md)
* `src/led.rs` — オンボードRGB LED (WS2812C/SK6812, RMT経由, `ws2812-esp32-rmt-driver`
  crateを使用) のON/OFFトグル制御。詳細は [design/led.md](design/led.md)
* `src/gamepad/` — コントローラー入力の抽象化（`Gamepad` トレイト）、
  Bluetooth Classic HID Host 経由の PS4 コントローラー接続（`bt_hid.rs`）、
  DS4 HID Inputレポートのパース（`ds4_report.rs`）
* `components/esp_hid_gap/` — ESP-IDF公式サンプル `examples/bluetooth/esp_hid_host`
  から vendor した、Bluetooth Classic HID Host の GAP/SDP 初期化・スキャン処理
  （`esp-idf-svc`/`esp-idf-hal` に相当するRust APIが無いため、`esp-idf-sys` の
  `extra_components` 機構でビルドに組み込み、bindgen でRustバインディングを生成）

### 依存クレート（LED制御）

* `ws2812-esp32-rmt-driver = "=0.13.1"`（`smart-leds-trait` feature） /
  `smart-leds = "0.4"` — WS2812/SK6812をRMT経由で駆動する（[design/led.md](design/led.md)参照）

### セットアップ手順

[development/setup.md](development/setup.md) を参照。
