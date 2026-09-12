//! コントローラー接続待機中、LEDアニメーションを別スレッドで表示し続ける。
//!
//! `bt_hid::scan_and_connect`は1回あたり数秒間ブロックするため、メインスレッドの
//! 待機ループ内でLEDを更新してもアニメーションにならない。専用スレッドで
//! `Led`の所有権を持ち、独立した周期でフレームを更新する。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use anyhow::Result;
use esp_idf_svc::hal::delay::FreeRtos;

use crate::led::Led;

/// アニメーションのフレーム間隔。
const FRAME_INTERVAL_MS: u32 = 150;

/// 実行中のアニメーションスレッドのハンドル。
pub struct ConnectingAnimation {
    stop: Arc<AtomicBool>,
    handle: thread::JoinHandle<Led<'static>>,
}

impl ConnectingAnimation {
    /// `led`の所有権を受け取り、停止されるまでアニメーションを表示し続ける
    /// スレッドを開始する。
    pub fn start(mut led: Led<'static>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_for_thread = Arc::clone(&stop);

        let handle = thread::spawn(move || {
            let mut step: u32 = 0;
            while !stop_for_thread.load(Ordering::Relaxed) {
                if let Err(e) = led.show_connecting_animation_frame(step) {
                    log::warn!("connecting animation frame failed: {e}");
                }
                step = step.wrapping_add(1);
                FreeRtos::delay_ms(FRAME_INTERVAL_MS);
            }
            led
        });

        Self { stop, handle }
    }

    /// アニメーションスレッドを停止し、`Led`の所有権を呼び出し側に返す。
    pub fn stop(self) -> Result<Led<'static>> {
        self.stop.store(true, Ordering::Relaxed);
        self.handle
            .join()
            .map_err(|_| anyhow::anyhow!("connecting animation thread panicked"))
    }
}
