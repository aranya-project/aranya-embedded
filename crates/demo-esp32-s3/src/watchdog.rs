use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use embassy_time::Timer;
use esp_hal::{
    peripherals::{TIMG0, TIMG1},
    timer::timg::{TimerGroupInstance, MwdtStage, Wdt},
};
use fugit::ExtU64;
use static_cell::StaticCell;

const WATCHDOG_TIMEOUT_MS: u64 = 750;
const WATCHDOG_FEED_INTERVAL_MS: u64 = 50;

static WATCHDOG0: StaticCell<Watchdog<TIMG0>> = StaticCell::new();
static WATCHDOG1: StaticCell<Watchdog<TIMG1>> = StaticCell::new();

pub fn watchdog_init(
    wdt0: Wdt<TIMG0>,
    wdt1: Wdt<TIMG1>,
) -> (&'static Watchdog<TIMG0>, &'static Watchdog<TIMG1>) {
    (
        WATCHDOG0.init(Watchdog::new(wdt0)),
        WATCHDOG1.init(Watchdog::new(wdt1)),
    )
}

pub struct Watchdog<TIMG> {
    wdt: Mutex<CriticalSectionRawMutex, Wdt<TIMG>>,
}

impl<TIMG> Watchdog<TIMG> where TIMG: TimerGroupInstance {
    pub fn new(mut wdt: Wdt<TIMG>) -> Watchdog<TIMG> {
        wdt.set_timeout(MwdtStage::Stage0, WATCHDOG_TIMEOUT_MS.millis());
        wdt.enable();
        wdt.feed();
        Watchdog {
            wdt: Mutex::new(wdt),
        }
    }

    pub async fn feed(&self) {
        self.wdt.lock().await.feed();
    }
}

#[embassy_executor::task]
pub(crate) async fn idle_task0(watchdog: &'static Watchdog<TIMG0>) {
    loop {
        watchdog.feed().await;
        Timer::after_millis(WATCHDOG_FEED_INTERVAL_MS).await;
    }
}

#[embassy_executor::task]
pub(crate) async fn idle_task1(watchdog: &'static Watchdog<TIMG1>) {
    loop {
        watchdog.feed().await;
        Timer::after_millis(WATCHDOG_FEED_INTERVAL_MS).await;
    }
}
