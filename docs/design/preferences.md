# 設定値の不揮発保存（Preferences）

## 方針

* 新規crateは追加せず、導入済みの `esp-idf-svc` の `nvs` モジュール（`EspNvs`）を利用する。
* `src/preferences.rs` の `Preferences` は、名前空間単位のキー・バリューストアとして
  `EspNvs` を薄くラップする（Arduinoの `Preferences` 相当）。

## API

* `Preferences::open(partition, namespace)` — 名前空間を読み書き可能で開く（無ければ作成）。
* `get_*` は未保存なら `Ok(None)` を返す。呼び出し側でデフォルト値にフォールバックする。
* 型: `u32` / `i32` / `f32`（ビット列を `u32` で保存）/ 文字列（127バイトまで）。
* `remove(key)` でキーを削除する。
* NVSの制約により、名前空間名・キー名は15文字以内。

## 利用手順

1. `main` で `EspDefaultNvsPartition::take()` を一度だけ呼ぶ（空きページ無し・
   バージョン不一致の場合は自動で消去して再初期化する）。
2. `partition.clone()` を `Preferences::open` に渡す。
3. `bt_hid::init` 内の `nvs_flash_init` はBluedroidのために別途呼ばれるが、
   初期化済みの場合は成功を返すため衝突しない。

## 現状

* サーボ中央パルス幅（キー`center`, u32）と振幅（キー`gain`, i32）を保存している（[motor.md](motor.md)参照）。
