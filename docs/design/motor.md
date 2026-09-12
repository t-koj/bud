# モーター制御設計

## 背景

当初 `src/motor/` は TB6612FNG 等のGPIO直結モータードライバIC（PWM+方向ピン+STBYピン）を
前提に実装されていたが、実機の仕様（[spec.md](../spec.md)）が定める「ATOMIC Motionベース
v1.2」は **I2C制御**のボード（STM32搭載、I2Cアドレス`0x38`）であり、GPIOに直結する
インターフェースを持たない。この不一致が判明したため、I2Cレジスタ制御方式に書き換えた。

## I2Cプロトコル

M5Stack公式Arduinoライブラリ [`m5stack/M5Atomic-Motion`](https://github.com/m5stack/M5Atomic-Motion)
の実装（`I2C_Class::writeByte`）を参考にした。`[レジスタ番号, データ]` の2バイトを
1トランザクションで書き込む。

| 対象 | レジスタ | 値 |
| --- | --- | --- |
| サーボ角度 (ch 0〜3) | `0x00 + ch` | 0〜180 (u8, 度) |
| DCモーター速度 (ch 0〜1) | `0x20 + ch` | -127〜127 (i8) |

本プロジェクトでは `AtomicMotion::set_motor_speed` の速度引数を、他コード（スティック値）
との規約に合わせて -100〜100 に制限している（デバイス上限-127〜127は使い切らない）。

## `AtomicMotion` 型 (`src/motor/mod.rs`)

DCモーター・サーボは同一のI2Cバス上の1デバイスであるため、TB6612FNG時代のような
`DcMotor`/`Servo`という別々の型ではなく、`AtomicMotion`という単一の型が
チャンネル番号を引数に取る形にまとめた。

* `set_motor_speed(channel: u8, speed: i8)`
* `set_servo_angle(channel: u8, angle_deg: f32)`

レジスタ番号の計算とチャンネル範囲チェックは純粋関数（`motor_speed_register`,
`servo_angle_register`）に切り出し、`#[cfg(test)]`でテストしている
（I2Cハードウェア自体の動作は実機なしでは検証できない。[testing.md](../testing.md)参照）。

## GPIO配線（ATOM Matrix v1.1）

* I2C: SDA=GPIO32, SCL=GPIO26（Groveポート固定配線）
* ATOM Lite の場合はSDA=GPIO25, SCL=GPIO21となり異なる（[development.md](../development.md)参照）

## エラーハンドリング

ATOMIC Motionベース未接続時はI2C書き込みがNACKで失敗する。これは回復可能なエラーとして
扱い、`main.rs`のメインループでは`log::warn!`でログに残した上でループ自体は継続する
（PS4接続やLED制御など他の機能を止めないため。実機で確認: モーター基板未接続のまま
`?`でエラーを伝播していたところ、`Error: ESP_FAIL`でアプリ全体が終了してしまっていた）。

未接続のI2Cバスへの書き込みはタイムアウト（`I2C_TIMEOUT_MS`）するまでブロックするため、
`main.rs`では失敗したチャンネルを以降二度と再試行しない。ATOMIC Motionベースは起動時の
配線の有無で決まり実行中に後から繋がることは無いため、初回の書き込みで確立しなければ
以降も回復する見込みが無く、毎フレーム（間引いた定期的な再試行でも同様）試行し続けると
メインループの実効周期が`LOOP_INTERVAL_MS`から大きく後退し、PS4コントローラーの
入力ポーリング（`gamepad.poll()`の呼び出し頻度）が落ちて○ボタンの短い押下エッジを
取りこぼす実機不具合を確認した。

## 未検証事項

* ATOMIC MotionベースへのI2Cレジスタ書き込みで実際にモーター/サーボが動作するかは
  実機での確認が必要（基板自体は未接続の状態でPS4接続機能のみ検証済み）。
