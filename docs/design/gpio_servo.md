# GPIO直結サーボ制御設計

## 目的

ATOMIC Motionベース（I2C、[motor.md](motor.md)）を経由せず、ESP32のGPIOから
サーボ制御信号（50Hz・パルス幅0.5〜2.5ms）を直接出力する。`src/gpio_servo.rs` の
`GpioServo` 型で扱う。

## 実装方針

* ESP32のLEDCペリフェラルを既存依存 `esp-idf-svc::hal::ledc` 経由で使う
  （新規ライブラリなし）。
* LEDCは周波数をタイマー単位でしか設定できない。全チャンネルが同じ50Hzなので
  `new_servo_timer` で1つ生成し、各 `GpioServo::new` に借用で渡して共有する。
* 分解能は14bit。20ms周期で約1.2μs刻みとなり、パルス幅を細かく指定できる。
* パルス幅は500μs(0度)〜2500μs(180度)。`ATOMIC Motion` 側のサーボ仕様
  （M5Stack SG90系）と同じ範囲。
* 生成直後はデューティ比0（パルス無し）。`set_angle` / `set_pulse_us` を呼ぶまで
  サーボへ信号を出さない。`stop()` でも同じ状態に戻る。
* 角度→パルス幅、パルス幅→デューティ値は純粋関数（`angle_to_pulse_us`,
  `pulse_us_to_duty`）に切り出して `#[cfg(test)]` でテストする
  （[testing.md](../testing.md)参照）。

## 割り当て

| 出力 | GPIO | LEDCチャンネル | タイマー |
| --- | --- | --- | --- |
| サーボ信号 1 | GPIO33 | channel0 | timer0 (50Hz) |
| サーボ信号 2 | GPIO19 | channel1 | timer0 (50Hz) |

現時点では初期化のみで、スティック入力等には未割り当て。

## API

* `new_servo_timer(timer)`: 50Hzのタイマーを生成する
* `GpioServo::new(channel, &timer, pin)`
* `set_angle(angle_deg: f32)`: 0.0〜180.0度、範囲外は丸める
* `set_pulse_us(pulse_us: u32)`: 500〜2500μs、範囲外は丸める
* `stop()`
