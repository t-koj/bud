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

`main.rs`はコントローラーのスティック値(-100〜100, i8)をそのままサーボ角度には渡せない
ため、`stick_to_servo_angle(stick: i8) -> f32`で0.0〜180.0度に線形マッピングする
（-100→0度、0→90度（中央）、100→180度）。この変換もハードウェアに依存しない純粋関数
のため`#[cfg(test)]`でテストしている。

M5Stack公式のサーボチャンネルラベル(S1〜S4)とコード上の`channel`番号(0〜3)は
S1=0, S2=1, S3=2, S4=3で対応する。

## GPIO配線（ATOM Matrix v1.1 / ATOM Lite）

* ATOM Matrix: I2C SDA=GPIO32, SCL=GPIO26（Groveポート固定配線）
* ATOM Lite: I2C SDA=GPIO25, SCL=GPIO21
* どちらの配線を使うかはCargo feature (`matrix`/`lite`) でビルド時に選択する
  （[development.md](../development.md)参照）

## エラーハンドリング

ATOMIC Motionベース未接続時はI2C書き込みがNACKで失敗する。これは回復可能なエラーとして
扱い、`main.rs`のメインループでは`log::warn!`でログに残した上でループ自体は継続する
（PS4接続やLED制御など他の機能を止めないため。実機で確認: モーター基板未接続のまま
`?`でエラーを伝播していたところ、`Error: ESP_FAIL`でアプリ全体が終了してしまっていた）。

未接続のI2Cバスへの書き込みはタイムアウト（`I2C_TIMEOUT_MS`）するまでブロックするため、
毎フレーム試行し続けるとメインループの実効周期が`LOOP_INTERVAL_MS`から大きく後退し、
PS4コントローラーの入力ポーリング（`gamepad.poll()`の呼び出し頻度）が落ちる実機不具合を
確認した。そのため`main.rs`では失敗したチャンネルを`SERVO_RETRY_INTERVAL_FRAMES`
（20フレーム）待ってから再試行する（サーボ2ch分を`servo_retry_countdown`/
`servo_error`配列で管理）。当初は初回失敗で以降二度と再試行しない実装だったが、
ATOMIC Motionベースを実際に接続した状態でも起動直後の一時的な応答遅延等で
初回書き込みだけ失敗するケースを再試行できず、サーボが恒久的に無反応になる不具合が
あったため、間引いたリトライに変更した。

## 現在の入力割り当て

`main.rs`のメインループは、左スティック上下をサーボS1(channel 0)、右スティック上下を
サーボS3(channel 2)に割り当てている。`set_motor_speed`（DCモーター制御）はAPIとして
実装済みだが、現時点ではどのスティック/ボタンにも割り当てていない
（`#[allow(dead_code)]`で警告を抑止）。

## 未検証事項

* ATOMIC MotionベースへのI2Cレジスタ書き込みで実際にモーター/サーボが動作するかは
  実機での確認が必要（基板自体は未接続の状態でPS4接続機能のみ検証済み）。
* 実機（基板接続済み）での動作確認で、`set_servo_angle`が`ESP_FAIL`で失敗し続ける
  現象を確認した。約16秒間・スティック操作ありで観測しても一度も成功
  （`recovered`ログ）が記録されず、原因未特定（配線/アドレス不一致、I2Cバスの
  タイミング、ATOMIC Motionベース側の応答性等の可能性がある）。切り分けのため、
  失敗ログに角度の値を追加し、失敗から成功に復帰した際にも
  `set_servo_angle(...) recovered`をログ出力するようにした（成功が続いている間は
  ログを出さない。詳細は[led.md](led.md)参照）。
* さらなる切り分けのため、`main.rs`の起動時（`AtomicMotion`生成前）にI2Cバス上の
  全アドレス(1〜127)へゼロバイト書き込みを試み、応答したアドレスを`I2C scan:
  device(s) found at ...`としてログ出力する一時的な診断コードを追加した。
  応答が無ければ`I2C scan: no device responded`と出力され、配線・電源・プルアップ
  等バス自体の問題である可能性が高いと判断できる。原因判明後は削除する想定。
