# 仕様

## 概要

ESP32-Pico 搭載し、LEGO に組み込んで動くロボット。
PS4 (DualShock 4) コントローラーを Bluetooth Classic で接続し、
その入力でモーターを操作する。

## ハードウェア

* ATOM Matrix v1.1 または ATOM Lite (ESP2-Pico-D4)
* ATOMIC Motionベース v1.2
* DualShock 4 コントローラ
* 

## 機能要件

* DC モーターとサーボモーターを制御する（ATOMIC Motionベース v1.2 を I2C 経由で制御する）
* PS4 コントローラーを Bluetooth Classic 経由（HID Host）で接続し、
  スティック・ボタンの入力を取得する

* ATOMIC Motionベースを介さず、GPIO33・GPIO19からサーボ制御信号（50Hz、パルス幅
  0.5〜2.5ms）を直接出力できる（ESP32 LEDC）。現時点ではどの入力にも割り当てていない。
  詳細は[design/gpio_servo.md](design/gpio_servo.md)参照。

### 起動時

* PS4 コントローラーの接続を待機する。
  * 既に登録（ボンディング）済みのコントローラーは、PS(HOME)ボタン押下で再接続する。
  * 未登録のコントローラーは、SHARE+PSボタン長押しのペアリングモード（LED高速点滅）に
    することで検出・接続・登録する。
  * 待機中はLEDをアニメーション表示する（ATOM Matrixは点灯位置を1画素ずつ順送りする
    マーキー表示、ATOM Liteは単色LEDの点滅）。接続完了でアニメーションを止める。

### メインループ

* 左スティック上下でサーボ(S1, channel 0)、左スティック左右でサーボ(S2, channel 1)、
  右スティック上下でサーボ(S3, channel 2)、右スティック左右でサーボ(S4, channel 3)の
  角度を操作する。サーボへの書き込みに失敗した場合は20フレームごとに再試行する。
* 各スティック軸の入力には中央付近±10（入力レンジ-100〜100中）のデッドゾーンを
  適用し、中央付近のブレによる誤動作を防ぐ。デッドゾーン外の値は0〜100（または
  0〜-100）に再スケーリングする。詳細は[design/motor.md](design/motor.md)参照。
* サーボへの書き込みが（全チャンネル）成功している間はLEDを緑、いずれかのチャンネルが
  失敗している間はLEDを赤で表示する。
* コントローラー接続完了直後は、Bluetoothスタックのリンクポリシー・ネゴシエーション
  （BT GAP `MODE_CHG_EVT`）が完了するまでの間、緑/赤に優先してLEDを青で表示する
  （最大45秒でタイムアウトし通常表示にフォールバックする）。詳細は
  [design/led.md](design/led.md)参照。

## 未確定の項目

* ATOMIC Motionベースとの接続は、ATOM Matrix/Lite共通でSDA=GPIO25, SCL=GPIO21固定
  （M5Stack公式ドキュメントで確認済み。詳細は[開発環境](development.md)、
  [design/motor.md](design/motor.md)参照）。当初ATOM MatrixはGroveポート配線
  (GPIO32/26)を使うと誤って想定していたため、実機でサーボが動作しない問題が
  発生していたが修正済み。
* ATOM Matrix / ATOM Lite はオンボードLEDの画素数が異なるため、Cargo feature
  (`matrix`/`lite`)によるビルド時選択で切り替える（[開発環境](development.md) 参照）。
  デフォルトfeatureは設定しておらず、指定し忘れはビルドエラーになる。
* DS4 の Bluetooth Classic HID Input レポートは、当初想定していた Report ID `0x11`
  拡張レポートではなく、実機では Report ID `0x01` の9バイト簡易レポートが届くことを
  実機ログで確認し、パース処理を修正済み。詳細は [design/led.md](design/led.md) 参照。
  L1/R1/L2/R2/Share/Options/L3/R3等の未使用ボタンのビット位置は未検証。
* 登録済みコントローラーのPSボタン再接続と、未登録コントローラーのSHARE+PSペアリングは
  同一のスキャン処理で両対応できることを実機で確認済み（SHARE+PSペアリングモードで
  接続成功）。
* SSP(Secure Simple Pairing)が正しく有効化されないとDS4との認証に失敗するため、
  `components/esp_hid_gap/Kconfig.projbuild`で`CONFIG_EXAMPLE_SSP_ENABLED=y`を
  明示的に定義している（[design/led.md](design/led.md)、
  `components/esp_hid_gap/README.md`参照）。
* 起動〜コントローラー接続完了までの待ち時間短縮のため、`esp_hid_scan`のBLEスキャン
  フェーズ（DS4はBluetooth Classicのみで使わない）を無効化済み。詳細は
  [design/led.md](design/led.md)参照。
* ATOMIC Motionベースのサーボチャンネルは、M5Stack公式のラベル表記(S1〜S4)とコード上の
  `channel`番号(0〜3)が対応する（S1=0, S2=1, S3=2, S4=3）。DCモーター制御・LEDトグルは
  実装済みだが現時点ではどのスティック/ボタンにも割り当てていない
  （[design/motor.md](design/motor.md)、[design/led.md](design/led.md)参照）。
* ATOMIC Motionベースを接続した状態だとシリアルモニタに接続できないという実機の制約が
  あり、その状態でのログによる動作確認ができない。このためメインループの状態は
  LEDの色（緑=正常/赤=異常）で判別できるようにしている（[design/led.md](design/led.md)、
  [design/motor.md](design/motor.md)参照）。
* 180度（位置決め）サーボは、目標角度がスティック入力のノイズ等で毎フレーム
  微小に変化し続けると一定周期（体感1秒程度）でしか反応しなくなる現象が実機で
  観測された（360度連続回転サーボでは起きない）。直近に実際に送信した角度から
  一定角度（`SERVO_SEND_THRESHOLD_DEG`, 未調整）以上変化しない限り書き込みを
  行わないことで回避する。詳細は[design/motor.md](design/motor.md)参照。
* 360度連続回転サーボは、停止点（ニュートラル点）に個体差があり90度ちょうど
  とは限らない。ずれていると停止のつもりでも微回転し続けるため、チャンネルごとの
  トリム値`SERVO_NEUTRAL_TRIM_DEG`（`main.rs`）で実機校正する必要がある
  （現状は未校正でデフォルト0.0）。詳細は[design/motor.md](design/motor.md)参照。
