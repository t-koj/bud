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
* `src/gamepad/bt_hid.rs` が Bluetooth Classic HID Host (`esp_hidh`) 経由での
  PS4コントローラー探索・接続を実装しているが、`main.rs` からはまだ呼び出して
  いない（DualShock 4 の HID Input レポートのバイト単位パースを実機で確認して
  から `Gamepad` トレイトの実装として繋ぎ込む方針）。`cargo build` でのビルド
  成功は確認済み。

### 構成

* `src/motor/` — DCモーター（`dc_motor.rs`）とサーボ（`servo.rs`）の制御。
  いずれも ESP-IDF の LEDC（PWM）を esp-idf-hal 経由で利用する
* `src/gamepad/` — コントローラー入力の抽象化（`Gamepad` トレイト）と、
  Bluetooth Classic HID Host 経由の PS4 コントローラー接続（`bt_hid.rs`）
* `components/esp_hid_gap/` — ESP-IDF公式サンプル `examples/bluetooth/esp_hid_host`
  から vendor した、Bluetooth Classic HID Host の GAP/SDP 初期化・スキャン処理
  （`esp-idf-svc`/`esp-idf-hal` に相当するRust APIが無いため、`esp-idf-sys` の
  `extra_components` 機構でビルドに組み込み、bindgen でRustバインディングを生成）

### セットアップ手順

[development/setup.md](development/setup.md) を参照。
