use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};

#[derive(Debug, Default)]
pub struct NeopixelState {
    pub unseen_count: usize,
    pub mentioned: bool,
}

pub static NEOPIXEL_SIGNAL: Signal<CriticalSectionRawMutex, NeopixelState> = Signal::new();
