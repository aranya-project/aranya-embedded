#![cfg_attr(not(feature = "std"), no_std)]

mod abstract_io;
mod parameter_store;

use core::ops::Mul;

use aranya_runtime::GraphId;
use serde::{Deserialize, Serialize};

pub const MAX_PEERS: usize = 16;

pub use abstract_io::*;
pub use parameter_store::*;

#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
pub struct RgbU8 {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Mul<f32> for RgbU8 {
    type Output = RgbU8;

    fn mul(self, rhs: f32) -> RgbU8 {
        RgbU8 {
            red: (self.red as f32 * rhs) as u8,
            green: (self.green as f32 * rhs) as u8,
            blue: (self.blue as f32 * rhs) as u8,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Parameters {
    pub graph_id: Option<GraphId>,
    pub address: u16,
    pub peers: heapless::Vec<u16, MAX_PEERS>,
    pub color: RgbU8,
}
