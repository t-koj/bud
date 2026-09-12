# esp_hid_gap (vendored)

ESP-IDF公式サンプル [`examples/bluetooth/esp_hid_host`](https://github.com/espressif/esp-idf/tree/v5.3.5/examples/bluetooth/esp_hid_host)
の `esp_hid_gap.c` / `esp_hid_gap.h` をそのまま取り込んだコンポーネント。

Bluetooth Classic HID Host (`esp_hidh`) を使ってPS4コントローラー等のHIDデバイスを
探索・接続するためのGAP初期化/スキャン処理を提供する。`esp-idf-svc`/`esp-idf-hal`には
相当するRust APIが存在しないため、`esp-idf-sys`の`extra_components`機構でこの
コンポーネントをビルドに組み込み、`bindings.h`経由でRustバインディングを生成して
`src/gamepad/bt_hid.rs`から直接呼び出す(design.mdの「既存のライブラリを活用し、
実装を最小限にする」方針に基づく)。

ライセンス: 元ファイルのSPDXヘッダの通り `Unlicense OR CC0-1.0` (Espressif Systems)。
