use anyhow::Result;
use esp_idf_svc::hal::gpio::OutputPin;
use esp_idf_svc::hal::peripheral::Peripheral;
use esp_idf_svc::hal::rmt::config::TransmitConfig;
use esp_idf_svc::hal::rmt::{RmtChannel, TxRmtDriver};
use smart_leds::{SmartLedsWrite, RGB8};
use ws2812_esp32_rmt_driver::Ws2812Esp32Rmt;

/// ON時の色。フルの(255,255,255)は消費電力・眩しさの観点で過剰なため控えめな輝度にする。
const ON_COLOR: RGB8 = RGB8::new(16, 16, 16);
const OFF_COLOR: RGB8 = RGB8::new(0, 0, 0);
/// LED用RMTのTX側メモリブロック数。このRMTチャンネルはLED専用（他チャンネル未使用）
/// のため割り当て可能な最大値(8)を使う。デフォルト(1ブロック=64 items)では
/// ATOM Matrix 25画素分(1200 items)の送信中に割り込みでのバッファ継ぎ足し(refill)が
/// 多数回必要になり、Bluetoothスタックの処理でrefillが遅延するとWS2812のリセット
/// タイミング仕様(50us以上のLow)に抵触してマーキー表示が途中から乱れる現象が
/// 実機で確認されたため、refill回数を減らして遅延の猶予を増やす。
const LED_RMT_MEM_BLOCK_NUM: u8 = 8;
/// メインループ正常時（サーボへのI2C書き込み成功）の色。
const OK_COLOR: RGB8 = RGB8::new(0, 16, 0);
/// メインループ異常時（サーボへのI2C書き込み失敗、リトライ待ち中を含む）の色。
const ERROR_COLOR: RGB8 = RGB8::new(16, 0, 0);

/// ボード搭載のアドレサブルRGB LED（ATOM Matrixは5x5のWS2812C×25画素、
/// ATOM Liteは単色1画素。どちらもGPIO27固定でRMT経由制御）をON/OFFトグルする。
pub struct Led<'a> {
    driver: Ws2812Esp32Rmt<'a>,
    pixel_count: usize,
    is_on: bool,
}

impl<'a> Led<'a> {
    /// `pixel_count` は搭載LED数（ATOM Matrix: 25, ATOM Lite: 1）。
    pub fn new(
        channel: impl Peripheral<P = impl RmtChannel> + 'a,
        pin: impl Peripheral<P = impl OutputPin> + 'a,
        pixel_count: usize,
    ) -> Result<Self> {
        let rmt_config = TransmitConfig::new()
            .clock_divider(1)
            .mem_block_num(LED_RMT_MEM_BLOCK_NUM);
        let tx = TxRmtDriver::new(channel, pin, &rmt_config)?;
        let driver = Ws2812Esp32Rmt::new_with_rmt_driver(tx)?;
        let mut led = Self {
            driver,
            pixel_count,
            is_on: false,
        };
        led.off()?;
        Ok(led)
    }

    pub fn on(&mut self) -> Result<()> {
        self.write(ON_COLOR)?;
        self.is_on = true;
        Ok(())
    }

    pub fn off(&mut self) -> Result<()> {
        self.write(OFF_COLOR)?;
        self.is_on = false;
        Ok(())
    }

    /// 現時点ではどのボタンにも割り当てていない（`main.rs`未使用）ため警告を抑止する。
    #[allow(dead_code)]
    pub fn toggle(&mut self) -> Result<()> {
        if self.is_on {
            self.off()
        } else {
            self.on()
        }
    }

    /// メインループが正常（サーボへのI2C書き込みが直近成功）であることを示す色を表示する。
    pub fn set_ok(&mut self) -> Result<()> {
        self.write(OK_COLOR)
    }

    /// メインループが異常（サーボへのI2C書き込みが失敗中、リトライ待ちを含む）であることを
    /// 示す色を表示する。
    pub fn set_error(&mut self) -> Result<()> {
        self.write(ERROR_COLOR)
    }

    /// コントローラー接続待機中のアニメーションの1フレームを表示する。
    /// `step`はフレームを追うごとに1ずつ増える通し番号。
    ///
    /// ATOM MatrixとATOM Liteとで搭載LED数が異なり単色点滅では間延びするため、
    /// Matrixは1画素ずつ点灯位置をずらす（マーキー）、Liteは単純な点滅にする。
    pub fn show_connecting_animation_frame(&mut self, step: u32) -> Result<()> {
        #[cfg(feature = "matrix")]
        let pixels = marquee_frame(step, self.pixel_count);
        #[cfg(feature = "lite")]
        let pixels = blink_frame(step);

        self.driver.write(pixels.into_iter())?;
        Ok(())
    }

    fn write(&mut self, color: RGB8) -> Result<()> {
        self.driver
            .write(std::iter::repeat(color).take(self.pixel_count))?;
        Ok(())
    }
}

/// ATOM Matrix向け: 点灯位置を1画素ずつ順送りするマーキー表示。
/// 画素の物理的な配置（配線順）は未確認のため、配線順インデックスをそのまま使う。
#[cfg(feature = "matrix")]
fn marquee_frame(step: u32, pixel_count: usize) -> Vec<RGB8> {
    let lit = step as usize % pixel_count;
    (0..pixel_count)
        .map(|i| if i == lit { ON_COLOR } else { OFF_COLOR })
        .collect()
}

/// ATOM Lite向け: 単色LED1個での点滅表示。
#[cfg(feature = "lite")]
fn blink_frame(step: u32) -> Vec<RGB8> {
    vec![if step % 2 == 0 { ON_COLOR } else { OFF_COLOR }]
}
