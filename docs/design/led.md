# LED制御設計（Lチカ）

## 概要

ATOM Matrix/Lite のオンボードRGB LED（GPIO27固定配線、ATOM Matrixは5x5 WS2812Cの
25画素マトリクス、ATOM Liteは単色1画素）を制御する。現在`main.rs`で使っているのは
コントローラー接続待機中のアニメーション表示と、接続完了時の消灯のみ（`on()`/`off()`）。
○(Circle)ボタンによるON/OFFトグル（`toggle()`）はAPIとして実装済みだが、現時点では
どのボタンにも割り当てていない（[spec.md](../spec.md)参照）。

## ライブラリ選定

[design.md](../design.md) の方針「既存のライブラリがあれば利用し、実装を最小限にする」
に従い、WS2812/SK6812をRMT経由で駆動する `ws2812-esp32-rmt-driver` crate
（`smart-leds-trait` feature、`smart-leds`のRGB8型を使用）を採用した。
RMT自体は`esp-idf-hal`（`esp_idf_svc::hal::rmt`）にAPIがあるが、タイミング生成を
手書きするより既存ライブラリを使う方がバグが少なく実装量も少ない。

バージョンは `=0.13.1` に固定している。0.14系以降は `esp-idf-hal ^0.46` を要求し、
本プロジェクトが固定している `esp-idf-hal 0.45.2`（`esp-idf-svc 0.51`経由）と
バージョン不一致になるため。

## `Led` 型 (`src/led.rs`)

* `Led::new(channel, pin, pixel_count)` — RMTチャンネルとGPIOピン、画素数を受け取る。
  画素数は呼び出し側（`main.rs`）でATOM Matrix=25、ATOM Lite=1のように指定する。
* `on()` / `off()` / `toggle()` — 全画素を同じ色に設定する。ONは控えめな白
  `RGB8::new(16,16,16)`（フル255は消費電力・眩しさの観点で過剰なため）。
* `show_connecting_animation_frame(step)` — コントローラー接続待機中のアニメーション
  の1フレームを表示する。`matrix`/`lite`のCargo featureで挙動を分岐する。
  * `matrix`（25画素）: `step % 25`番目の画素だけを点灯するマーキー表示。画素の物理的な
    配置（配線順）は未確認のため、配線順インデックスをそのまま使っている
    （[spec.md](../spec.md)の未確定の項目参照）。
  * `lite`（1画素）: `step`の偶奇でON/OFFする単純な点滅。

## `ConnectingAnimation` 型 (`src/connecting_animation.rs`)

`bt_hid::scan_and_connect`（`esp_hid_scan`のFFI呼び出し）は1回あたり`SCAN_SECONDS`秒
ブロックするため、`main.rs`の接続待機ループ内で毎フレーム`Led`を更新してもアニメーション
にならない。そこで`Led`の所有権を専用スレッドに渡し、`FRAME_INTERVAL_MS`(150ms)周期で
`show_connecting_animation_frame`を呼び続けることでアニメーションを実現している。

* `ConnectingAnimation::start(led)` — `Led`の所有権を受け取り、`AtomicBool`の停止フラグを
  共有しつつ`std::thread::spawn`でアニメーションスレッドを開始する。
* `ConnectingAnimation::stop(self)` — 停止フラグを立てて`JoinHandle::join()`でスレッドの
  終了を待ち、`Led`の所有権を呼び出し側に返す。`main.rs`はここで受け取った`Led`で
  改めて`off()`し、通常のメインループに入る。

`Led<'a>`は`peripherals.rmt.channel0`/`pins.gpio27`という所有値から構築されるため
`'a = 'static`に推論され、`thread::spawn`（`'static`境界）の制約を満たす。

## ○ボタンによるLEDトグルの割り当てについて

以前は`main.rs`のメインループで、コントローラーのボタンが「押されている間true」の状態
のみ届く性質を踏まえ、前回フレームの状態(`prev_circle`)を保持して`circle &&
!prev_circle`（立ち上がりエッジ）でのみ`toggle()`を呼ぶ実装だった（押しっぱなしで
連続トグルしないため）。現在は左右スティックをサーボ制御（[design/motor.md]
(motor.md)参照）に割り当てたため、この○ボタン割り当ては削除し、`toggle()`は
現時点でどのボタンにも割り当てていないAPIとして残している。

## DS4 Bluetooth Classic HID Input レポートの解析 (`src/gamepad/ds4_report.rs`)

当初、DS4はBT接続時にReport ID `0x11`の拡張レポート(78バイト、ジャイロ/タッチパッド等を
含む)を送るものと想定していたが、実機接続で `bt_hid::Ds4Gamepad` に生バイト列をログ出力
して確認したところ、実際に届いていたのは **Report ID `0x01`、9バイトの簡易レポート**
だった（DS4をBT拡張モードへ切り替えるための追加のfeature report送信を行っていないため、
DS4はUSB接続時と同様の簡易フォーマットのまま送ってくると見られる）。この実機ログを元に
パース処理を全面的に修正した。

| オフセット | 内容 |
| --- | --- |
| `data[0]` | 左スティックX（0〜255, 128中央） |
| `data[1]` | 左スティックY |
| `data[2]` | 右スティックX |
| `data[3]` | 右スティックY |
| `data[4]` bit0-3 | D-pad方向（8=中央） |
| `data[4]` bit4-7 | Square/Cross/Circle(`0x40`)/Triangle |
| `data[5]` | L1/R1/L2/R2/Share/Options/L3/R3（本プロジェクトでは未使用） |
| `data[6]` | PS/Touchpad/カウンタ（本プロジェクトでは未使用） |
| `data[7]`,`data[8]` | L2/R2アナログ値（本プロジェクトでは未使用） |

スティックのY軸はDS4の生値は下方向が増加するため、本プロジェクトの規約
（[`GamepadState`](../../src/gamepad/mod.rs)のdoc comment: 上/右が+100）に合わせて
符号反転している。

パース関数はハードウェア型に依存しない純粋関数のため`#[cfg(test)]`でテストしている
（実行可否は[testing.md](../testing.md)、[development.md](../development.md)参照）。

### 実機診断の仕組み (`Ds4Gamepad`)

`ds4_report`の想定が実機と食い違っていた経緯を踏まえ、`Ds4Gamepad::poll()`には
接続直後の数件（`raw_input_log_budget`）だけHID Inputレポートの生バイト列を
`report_id`と併せてログ出力する仕組みを残している。また、パース後のボタン状態が
変化したときだけ`buttons changed: ...`をログ出力する（DS4は高頻度で連射してくるため、
毎フレームログすると`docs/development.md`に記録した通りログ出力自体がCPUを占有して
ウォッチドッグリセットを招く恐れがあるため、変化時のみに限定している）。

## `bt_hid.rs` の接続待機ループ

`init()`（NVS/GAP/HIDH初期化、一度だけ呼べる）と `scan_and_connect()`
（スキャン&接続試行、繰り返し呼べる）に分割し、`main.rs`から

```rust
while !gamepad.is_connected() {
    bt_hid::scan_and_connect(PS4_CONTROLLER_NAME_PREFIX, SCAN_SECONDS)?;
    gamepad.poll();
}
```

のようにリトライループとして呼び出すことで「コントローラーの接続を待機する」を実現する。
登録済みコントローラーのPSボタン再接続と、未登録コントローラーのSHARE+PSペアリングの
どちらも`esp_hid_scan`で検出できることは実機で確認済み。

### 接続待機の高速化（BLEスキャンフェーズの無効化）

`esp_hid_scan`（`components/esp_hid_gap/esp_hid_gap.c`）は本来、BLE HIDデバイス
探索用のBLEスキャンを`seconds`秒、続けてClassic BTデバイス探索用のスキャンを
`seconds`秒実行する（呼び出し1回あたり合計約2倍の待ち時間）。DS4はBluetooth Classic
のみでBLE広告を行わないため、BLEスキャンは実機ログ上も毎回対象0件で終わっており、
起動〜コントローラー接続完了までの時間を不必要に伸ばしていた。この待ち時間短縮のため、
`esp_hid_scan`のBLEスキャンフェーズを無効化し、Classic BTスキャンのみを実行するように
`components/esp_hid_gap/esp_hid_gap.c`を変更した。これにより`scan_and_connect`
1回あたりの待ち時間がほぼ半分になる。

## 未検証事項

* WS2812C 5x5マトリクスがRMT ch0/GPIO27でちらつき無く光るか
  （公式ドキュメントにはWi-Fi/Bluetooth使用時にちらつく既知の問題が記載されており、
  必要ならRMTのmem_block_numを増やす対応を検討する）
* DS4 Report ID `0x01`のbyte[5]〜byte[8]（L1/R1/L2/R2/Share/Options/L3/R3、PS/Touchpad、
  L2/R2アナログ値）は実機ログで存在は確認したがビット位置までは未検証
  （本プロジェクトでは現状Square/Cross/Circle/Triangleとスティックのみ使用）。
