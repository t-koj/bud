# セットアップ

## Rust プロジェクト（本リポジトリの実装）

### セットアップ手順（macOS）

1. [espup](https://github.com/esp-rs/espup) で Rust の esp32 (Xtensa) 向けツールチェーンを
   インストールする

   ```sh
   cargo install espup
   espup install
   . $HOME/export-esp.sh
   ```

2. `ldproxy` と `espflash`（書き込み・モニタ用）をインストールする

   ```sh
   cargo install ldproxy espflash
   ```

3. ビルドする（初回は ESP-IDF がワークスペース配下に自動取得される）。
   ATOM Matrix/Lite でGPIO配線が異なるため、`--features matrix` または
   `--features lite` を必ず指定する（指定しないとコンパイルエラーになる。
   詳細: [development.md](../development.md)）。

   ```sh
   cargo build --features matrix   # ATOM Matrixの場合
   cargo build --features lite     # ATOM Liteの場合
   ```

4. 書き込み・モニタする

   ```sh
   cargo run --features matrix
   ```

   ESP32-Pico ボードを USB(シリアル変換経由)で接続すると `/dev/cu.usbserial-*`
   等のデバイスが現れる。ポートを明示する場合は次のようにする。

   ```sh
   espflash flash --port /dev/cu.usbserial-0001 --monitor target/xtensa-esp32-espidf/debug/bud
   ```

## C言語プロジェクト（Rust 実装が存在しない場合のフォールバック）

### セットアップ手順（macOS）

1. 前提ツールをインストールする

   ```sh
   brew install cmake ninja dfu-util
   ```

2. ESP-IDF を `~/esp/esp-idf` に取得する

   ```sh
   mkdir -p ~/esp
   git clone -b release/v5.3 --recursive https://github.com/espressif/esp-idf.git ~/esp/esp-idf
   ```

3. ツールチェーンをインストールする

   ```sh
   cd ~/esp/esp-idf
   ./install.sh esp32,esp32s3
   ```

   Python の SSL 証明書検証エラー（`CERTIFICATE_VERIFY_FAILED`）が出る場合は、
   `python.org` 版 Python の証明書がシステムに未反映であることが原因。
   `/Applications/Python <version>/Install Certificates.command` を実行してから
   再度 `install.sh` を実行する。

4. シェルごとに環境変数を読み込む

   ```sh
   . ~/esp/esp-idf/export.sh
   ```

### 動作確認

```sh
idf.py --version
```
