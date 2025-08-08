use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use num_traits::float::FloatCore;

use crate::aranya::policy;

#[derive(Debug, Default)]
pub struct MessageState {
    pub unseen_count: usize,
    pub mentioned: bool,
}

#[derive(Debug)]
pub enum NeopixelMessage {
    MessageState(MessageState),
    Rainbow,
    Ambient { color: policy::AmbientColor },
}

pub static NEOPIXEL_SIGNAL: Signal<CriticalSectionRawMutex, NeopixelMessage> = Signal::new();

const RAINBOW_V: f32 = 0.3;

/// Generate a RGB rainbow based on the given hue. This is not a very accurate conversion but
/// it'll work for our purposes.
pub fn rainbow_at(hue: u32) -> (u8, u8, u8) {
    let h = hue % 360;

    let h_rad = (h as f32) / 180.0 * 3.14159;
    let i = h_rad.trunc() as u8;
    let f = h_rad.fract();
    let q = RAINBOW_V * (1.0 - f);
    let t = RAINBOW_V * f;

    let (r, g, b) = match i {
        0 => (RAINBOW_V, t, 0.0),
        1 => (q, RAINBOW_V, 0.0),
        2 => (0.0, RAINBOW_V, t),
        3 => (0.0, q, RAINBOW_V),
        4 => (t, 0.0, RAINBOW_V),
        _ => (RAINBOW_V, 0.0, q),
    };

    ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}
