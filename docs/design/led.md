# LED制御設計（Lチカ）

## 概要

ATOM Matrix/Lite のオンボードRGB LED（GPIO27固定配線、ATOM Matrixは5x5 WS2812Cの
25画素マトリクス、ATOM Liteは単色1画素）を制御する。現在`main.rs`で使っているのは
コントローラー接続待機中のアニメーション表示、接続完了直後のBluetoothスタック
ネゴシエーション待ち表示（`set_preparing()`）、接続完了後のメインループでの
状態表示（`set_ok()`/`set_error()`。[design/motor.md](motor.md)のサーボ書き込み
成否と連動する）。○(Circle)ボタンによるON/OFFトグル（`toggle()`）はAPIとして
実装済みだが、現時点ではどのボタンにも割り当てていない（[spec.md](../spec.md)参照）。

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
* `set_ok()` / `set_error()` — メインループの状態表示用。緑`RGB8::new(0,16,0)`/
  赤`RGB8::new(16,0,0)`を表示する。ATOMIC Motionベースを接続した状態だと
  シリアルモニタに接続できず（実機の制約）ログでの動作確認ができないため、
  サーボへのI2C書き込みの成否をLEDの色で判別できるようにしている
  （[design/motor.md](motor.md)のエラーハンドリング参照）。
* `set_preparing()` — 青`RGB8::new(0,0,16)`を表示する。コントローラー接続完了直後、
  Bluetoothスタックのリンクポリシー・ネゴシエーションが完了する（後述の
  `Ds4Gamepad::is_operation_ready()`がtrueになる）までの間に表示する
  （下記「接続完了後のBluetoothスタックネゴシエーション待ち」参照）。
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
  終了を待ち、`Led`の所有権を呼び出し側に返す。`main.rs`はここで受け取った`Led`を
  通常のメインループでのサーボ状態表示（`set_ok()`/`set_error()`）に使う。

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

同様に、左右スティックのY値が`STICK_LOG_THRESHOLD`（10）以上変化したときだけ
`stick moved: left_y=... right_y=...`をログ出力する。スティックのアナログ値は
ノイズで常に微小変動するため、ボタンのような単純な差分判定（`!=`）だと
操作していなくてもログが連続して出続けてしまう。そのため閾値判定にしている。

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

## 接続完了後のBluetoothスタックネゴシエーション待ち表示

実機で、コントローラー接続完了ログ(`"PS4 controller connected"`)から実際に
操作可能になるまで約25秒の空白があり、その間はコントローラー入力を送っても
反応しない現象が確認された。ログを調べたところ、この間にBT GAP
`ESP_BT_GAP_MODE_CHG_EVT`（`mode:2` = `ESP_BT_PM_MD_SNIFF`）が発生しており、
ESP-IDF Bluedroidスタックが接続後にリンクポリシー（sniffモード等）の
ネゴシエーションを内部タイマー（`bta_api.h`の`BTA_DM_PM_HH_OPEN_DELAY`、約30秒）
で自動的に行っていることが判明した。このタイマーやイベント自体を無効化・制御する
公式なKconfig/APIはESP-IDFに存在しないため、アプリ側はこのイベントの受信を
「実際に操作可能になった」の目安として扱う方針にした。

### 検討経緯: `esp_bt_gap_register_callback`直接呼び出しは実機でクラッシュした

`ESP_BT_GAP_MODE_CHG_EVT`等のBT GAP標準API自体はESP-IDF本体の`bt`コンポーネントが
持つAPIであり、`esp_idf_svc::sys::*`から`components/esp_hid_gap`を介さず直接使える
（vendorが必要なのは`esp_hid_scan`や`esp_hidh_init`などESP-IDF公式サンプル固有の
関数のみ）。そこで当初、`components/esp_hid_gap`を一切変更せず、Rust側
（`src/gamepad/bt_hid.rs`）からこの標準APIを直接呼び出す方式を試みた。

しかし`esp_bt_gap_register_callback`はコールバックを1つしか保持できず、
`components/esp_hid_gap`が`esp_hid_gap_init()`内でスキャン・ペアリング処理
（PIN/SSP応答、探索結果処理）用に自分のコールバックを既に登録している。
「HIDデバイス接続確立後は再スキャン・再ペアリングを行わない」設計であることを
踏まえ、接続確立後（`GamepadEvent::Connected`送出時）に限って上書き登録すれば
安全なはずと考えて実装したが、**実機検証でコントローラーが接続できなくなる
regressionが発生した**。DS4側は接続完了表示になるがATOM側はLED接続待機
アニメーションのまま進まず、接続確立の瞬間（`ESP_HIDH_OPEN_EVENT`）に上書き登録
することがBluetoothスタック内部処理と衝突してクラッシュ（ウォッチドッグリセット）
を引き起こし、再起動ループに陥っていたと考えられる（ATOMIC Motionベース接続中は
シリアルモニタに繋がらないためクラッシュのログ自体は確認できていない）。

### 採用した方式: `components/esp_hid_gap`への汎用フック追加

上記の反省を踏まえ、GAPコールバックの登録を奪い合わない方式に変更した。
`components/esp_hid_gap/esp_hid_gap.h`/`.c`に汎用フック
`esp_hid_gap_set_event_hook(void (*hook)(esp_bt_gap_cb_event_t, esp_bt_gap_cb_param_t*))`
を追加し、既存の内部コールバック`bt_gap_event_handler`の先頭で
（登録されていれば）このフックを呼ぶだけにした。既存の登録（PIN/SSP応答・
探索結果処理）は一切変更せず「相乗り」する形のため、登録の奪い合いによる
クラッシュは起きない。フック自体は特定の意味を持たない汎用の通知機構に留め、
「どのイベントを見て何をするか」の判断はすべてRust側（`bt_hid.rs`の
`gap_event_hook`）に置くことで、vendorしたコードへの意味的な侵食も避けている。

`bt_hid::init()`内で`esp_hid_gap_init()`直後に一度だけ`esp_hid_gap_set_event_hook`
を呼んで登録し（接続前なので衝突の懸念がない）、`gap_event_hook`が
`ESP_BT_GAP_MODE_CHG_EVT`受信時に`GamepadEvent::LinkReady`を既存のイベント
チャネル（`Connected`/`Disconnected`/`RawInput`と同じ`mpsc`チャネル）に送出する。
`Ds4Gamepad::poll()`がこれを受けて`operation_ready`内部状態を立て、
`Ds4Gamepad::is_operation_ready()`で参照できるようにしている。

`main.rs`のメインループでは、`gamepad.is_operation_ready()`がtrueになるまで
`Led::set_preparing()`（青）を表示し、サーボの正常/異常表示（`set_ok()`/`set_error()`）
より優先する。イベントが何らかの理由で届かない場合に備え、接続後
`OPERATION_READY_TIMEOUT_MS`(45秒)でタイムアウトし、警告ログを出した上で
通常表示に切り替えるフォールバックを設けている。

なお、メインループ自体（サーボへのスティック入力反映）はこのフラグの状態に関わらず
常時動作させている。本対応はLED表示（ユーザーへのフィードバック）のみを対象とし、
制御ロジックの開始タイミングを遅延させるものではない。

## RMTバッファ不足によるマーキー表示の乱れ対策

実機で、接続待機中のマーキー表示が先頭の画素は正常だが途中からランダムに
複数画素が点灯して流れるように乱れ、継続するとさらに乱れが増す症状が発生した。

`ws2812-esp32-rmt-driver`（`Ws2812Esp32Rmt::new`）はデフォルトでRMT送信バッファを
1ブロック（64 items）しか確保しない。ATOM Matrixの25画素は1200 items必要なため、
送信中に割り込みで約19回バッファを継ぎ足す(refill)必要がある。本プロジェクトは
Bluetooth Classic HID (esp_hidh) を常時動かしており、その処理でrefill割り込みの
サービスが遅延すると、WS2812のリセット仕様（約50us以上のLowで再ラッチ）に
抵触して途中から画素データがずれる。これが症状と一致すると判断した。

対策として、`Led::new`（`src/led.rs`）で`TxRmtDriver`を明示的に構築し、
`TransmitConfig::mem_block_num`を最大値の8に設定している。このRMTチャンネルは
LED専用（他チャンネル未使用）のため、全ブロックを割り当てても問題ない。
refillの頻度が減ることで遅延に対する猶予が増える。

シリアルモニタが使えない実機制約（[spec.md](../spec.md)未確定の項目参照）により
本対応の効果は未検証。改善しない場合は`ConnectingAnimation`スレッド
（`src/connecting_animation.rs`）の優先度引き上げ・CPUコア固定を次の対応候補とする。

## 未検証事項

* 上記のRMTバッファ拡張策で実機のマーキー表示の乱れが解消するか。
* DS4 Report ID `0x01`のbyte[5]〜byte[8]（L1/R1/L2/R2/Share/Options/L3/R3、PS/Touchpad、
  L2/R2アナログ値）は実機ログで存在は確認したがビット位置までは未検証
  （本プロジェクトでは現状Square/Cross/Circle/Triangleとスティックのみ使用）。
* `ESP_BT_GAP_MODE_CHG_EVT`の受信タイミングが実際に「コントローラー操作が有効になる
  タイミング」と厳密に一致するかは未検証（実機ログ上の相関から採用した目安であり、
  因果関係を確認したものではない）。
