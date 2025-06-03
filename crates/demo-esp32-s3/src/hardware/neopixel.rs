use aranya_runtime::{Sink, VmEffect};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use esp_hal::{
    gpio::{Level, Output, OutputPin},
    peripheral::Peripheral,
    peripherals::RMT,
    rmt::{Channel, PulseCode, Rmt, TxChannel, TxChannelConfig, TxChannelCreator},
    Blocking,
};
use fugit::RateExtU32 as _;
use parameter_store::RgbU8;

use crate::aranya::policy::LedColorChanged;

const RMT_CLOCK_MHZ: u32 = 80;
const RMT_CLOCK_DIVIDER: u8 = 1;
const T0H: u16 = (((RMT_CLOCK_MHZ / RMT_CLOCK_DIVIDER as u32) * 400) / 1000) as u16;
const T0L: u16 = (((RMT_CLOCK_MHZ / RMT_CLOCK_DIVIDER as u32) * 850) / 1000) as u16;
const T1H: u16 = (((RMT_CLOCK_MHZ / RMT_CLOCK_DIVIDER as u32) * 800) / 1000) as u16;
const T1L: u16 = (((RMT_CLOCK_MHZ / RMT_CLOCK_DIVIDER as u32) * 450) / 1000) as u16;

pub static NEOPIXEL_SIGNAL: Signal<CriticalSectionRawMutex, RgbU8> = Signal::new();

#[derive(Debug, thiserror::Error)]
pub enum NeopixelError {
    #[error("Rmt error")]
    Rmt(esp_hal::rmt::Error),
    #[error("Neopixel in use")]
    InUse,
}

impl From<esp_hal::rmt::Error> for NeopixelError {
    fn from(value: esp_hal::rmt::Error) -> Self {
        NeopixelError::Rmt(value)
    }
}

pub struct Neopixel<'a> {
    power: Output<'a>,
    channel: Option<Channel<Blocking, 0>>,
    pulses: (u32, u32),
}

impl<'a> Neopixel<'a> {
    pub fn new(
        rmt: impl Peripheral<P = RMT> + 'a,
        data: impl Peripheral<P = impl OutputPin> + 'a,
        power: impl Peripheral<P = impl OutputPin> + 'a,
    ) -> Result<Self, NeopixelError> {
        let rmt = Rmt::new(rmt, 80.MHz())?;
        // Initialize neopixel power
        let power = Output::new(power, Level::High);

        // Initialize neopixel data channel
        let tx_config = TxChannelConfig {
            clk_divider: RMT_CLOCK_DIVIDER,
            ..TxChannelConfig::default()
        };
        let channel = rmt.channel0.configure(data, tx_config)?;

        log::info!("t0h: {T0H}, t0l: {T0L}, t1h: {T1H}, t1l: {T1L}");
        let pulses = (
            PulseCode::new(true, T0H, false, T0L),
            PulseCode::new(true, T1H, false, T1L),
        );

        Ok(Self {
            power,
            channel: Some(channel),
            pulses,
        })
    }

    pub fn set_color(&mut self, r: u8, g: u8, b: u8) -> Result<(), NeopixelError> {
        // Create the signal
        let mut signal: heapless::Vec<u32, 25> = heapless::Vec::new();
        for c in [g, r, b] {
            for i in (0..8).rev() {
                let bit: bool = (1 << i) & c != 0;
                // SAFETY: We push exactly as many items as we've allocated
                signal
                    .push(if bit { self.pulses.1 } else { self.pulses.0 })
                    .ok();
            }
        }
        signal.push(0).ok(); // empty pulse; end of pulse train

        // Actually send it (blocks all other operations)
        let channel = self.channel.take().ok_or(NeopixelError::InUse)?;
        // This transactional ownership scheme makes usage very awkward
        let tx = channel.transmit(&signal)?;
        let channel = tx.wait().expect("neopixel broken");
        self.channel = Some(channel);
        Ok(())
    }

    pub fn set_power(&mut self, on: bool) {
        self.power.set_level(on.into());
    }
}

impl Drop for Neopixel<'_> {
    fn drop(&mut self) {
        // Turn off the neopixel
        self.power.set_low();
    }
}

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
