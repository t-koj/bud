use anyhow::Result;
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};

/// 文字列を読み出すときの最大バイト数（終端NULを含む）。
const STR_BUF_LEN: usize = 128;

/// NVS(不揮発ストレージ)に設定値を保存する、名前空間単位のキー・バリューストア。
///
/// 電源を切っても値が残る。キー名は15文字以内、名前空間名も15文字以内（NVSの制約）。
/// 未保存のキーの取得は `Ok(None)` を返し、呼び出し側でデフォルト値にフォールバックさせる。
pub struct Preferences {
    nvs: EspNvs<NvsDefault>,
}

impl Preferences {
    /// `namespace` を読み書き可能で開く（無ければ作成する）。
    /// NVSパーティションは `EspDefaultNvsPartition::take` で一度だけ取得して複製して渡す。
    pub fn open(partition: EspDefaultNvsPartition, namespace: &str) -> Result<Self> {
        Ok(Self {
            nvs: EspNvs::new(partition, namespace, true)?,
        })
    }

    pub fn get_u32(&self, key: &str) -> Result<Option<u32>> {
        Ok(self.nvs.get_u32(key)?)
    }

    pub fn set_u32(&mut self, key: &str, value: u32) -> Result<()> {
        Ok(self.nvs.set_u32(key, value)?)
    }

    pub fn get_i32(&self, key: &str) -> Result<Option<i32>> {
        Ok(self.nvs.get_i32(key)?)
    }

    pub fn set_i32(&mut self, key: &str, value: i32) -> Result<()> {
        Ok(self.nvs.set_i32(key, value)?)
    }

    /// NVSにf32型は無いため、ビット列をu32として保存する。
    pub fn get_f32(&self, key: &str) -> Result<Option<f32>> {
        Ok(self.get_u32(key)?.map(f32::from_bits))
    }

    pub fn set_f32(&mut self, key: &str, value: f32) -> Result<()> {
        self.set_u32(key, value.to_bits())
    }

    /// `STR_BUF_LEN - 1` バイトを超える文字列は読み出せない（エラーになる）。
    pub fn get_string(&self, key: &str) -> Result<Option<String>> {
        let mut buf = [0u8; STR_BUF_LEN];
        Ok(self.nvs.get_str(key, &mut buf)?.map(str::to_owned))
    }

    pub fn set_string(&mut self, key: &str, value: &str) -> Result<()> {
        Ok(self.nvs.set_str(key, value)?)
    }

    /// キーを削除する。存在していれば `true`。
    pub fn remove(&mut self, key: &str) -> Result<bool> {
        Ok(self.nvs.remove(key)?)
    }
}
