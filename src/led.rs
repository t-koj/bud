use anyhow::Result;
use esp_idf_svc::hal::gpio::OutputPin;
use esp_idf_svc::hal::peripheral::Peripheral;
use esp_idf_svc::hal::rmt::RmtChannel;
use smart_leds::{SmartLedsWrite, RGB8};
use ws2812_esp32_rmt_driver::Ws2812Esp32Rmt;

/// ON時の色。フルの(255,255,255)は消費電力・眩しさの観点で過剰なため控えめな輝度にする。
const ON_COLOR: RGB8 = RGB8::new(16, 16, 16);
const OFF_COLOR: RGB8 = RGB8::new(0, 0, 0);

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
        let driver = Ws2812Esp32Rmt::new(channel, pin)?;
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

    pub fn toggle(&mut self) -> Result<()> {
        if self.is_on {
            self.off()
        } else {
            self.on()
        }
    }

    fn write(&mut self, color: RGB8) -> Result<()> {
        self.driver
            .write(std::iter::repeat(color).take(self.pixel_count))?;
        Ok(())
    }
}
