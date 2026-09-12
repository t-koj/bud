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

### 起動時

* PS4 コントローラーの接続を待機する。
  * 既に登録（ボンディング）済みのコントローラーは、PS(HOME)ボタン押下で再接続する。
  * 未登録のコントローラーは、SHARE+PSボタン長押しのペアリングモード（LED高速点滅）に
    することで検出・接続・登録する。

### メインループ

* ○(Circle)ボタン押下でLEDをON/OFFトグルする（押しっぱなしで連続トグルしない）。

## 未確定の項目

* モーター/サーボの実際の配線（GPIO 番号）。[開発環境](development.md) の
  ピン配置は仮のもので、実機の配線に合わせて変更する
* 対象ボードは ATOM Matrix v1.1 に決定（[開発環境](development.md) 参照）。
  ATOM Lite を使う場合はI2CのGPIO番号が異なる点に注意する。
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
