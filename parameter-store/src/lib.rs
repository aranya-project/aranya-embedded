#![cfg_attr(not(feature = "std"), no_std)]

mod abstract_io;
mod parameter_store;

use aranya_runtime::GraphId;
use serde::{Serialize, Deserialize};

pub const MAX_PEERS: usize = 16;

pub use abstract_io::*;
pub use parameter_store::*;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Parameters {
    pub graph_id: Option<GraphId>,
    pub address: u16,
    pub peers: heapless::Vec<u16, MAX_PEERS>,
}
