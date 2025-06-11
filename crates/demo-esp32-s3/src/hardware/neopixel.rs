use aranya_runtime::{Sink, VmEffect};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use esp_hal::{
    gpio::{Level, Output, OutputPin},
    peripheral::Peripheral,
    peripherals::RMT,
    rmt::{Channel, PulseCode, Rmt, TxChannel, TxChannelConfig, TxChannelCreator},
    Blocking,
};
use esp_rmt_neopixel::RgbU8;

use crate::aranya::policy::LedColorChanged;

pub static NEOPIXEL_SIGNAL: Signal<CriticalSectionRawMutex, RgbU8> = Signal::new();

pub struct NeopixelSink {}

impl NeopixelSink {
    pub fn new() -> NeopixelSink {
        NeopixelSink {}
    }
}

impl Sink<VmEffect> for NeopixelSink {
    fn begin(&mut self) {}

    fn consume(&mut self, effect: VmEffect) {
        if effect.recalled || effect.name != "LedColorChanged" {
            return;
        }
        let effect: LedColorChanged = match effect.fields.try_into() {
            Ok(e) => e,
            Err(_) => return,
        };
        NEOPIXEL_SIGNAL.signal(RgbU8 {
            red: effect.r as u8,
            green: effect.g as u8,
            blue: effect.b as u8,
        });
    }

    fn rollback(&mut self) {}

    fn commit(&mut self) {}
}
