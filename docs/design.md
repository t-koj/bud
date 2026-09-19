# 設計

* Rust での実装を第一候補とする。
* デバイスアクセス等、Rust での実装が存在しない場合は C言語 を利用する。
* 既存のライブラリがある場合はそれを利用し、実装を最小限にする

## 機能別詳細設計

* [design/motor.md](design/motor.md) — ATOMIC Motionベース（I2C）によるモーター/サーボ制御
* [design/led.md](design/led.md) — オンボードLEDの制御とPS4コントローラーとの連携
* [design/gpio_servo.md](design/gpio_servo.md) — ESP32 GPIO(LEDC)からのサーボ制御信号の直接出力（GPIO33/GPIO19）
* [design/preferences.md](design/preferences.md) — NVSによる設定値の不揮発保存（Preferences）
