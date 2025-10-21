use aranya_crypto::CmdId;
use aranya_runtime::{Address, Keys, Location, PolicyId, Prior, Priority};
use serde::{Deserialize, Serialize};

use crate::plathacks::Usize32;

type Bytes = Box<[u8]>;
type Update = (String, Keys, Option<Bytes>);

#[derive(Debug, Serialize, Deserialize)]
pub struct SegmentRepr {
    /// Self offset in file.
    pub offset: Usize32,
    pub prior: Prior<Location>,
    pub parents: Prior<Address>,
    pub policy: PolicyId,
    /// Offset in file to associated fact index.
    pub facts: Usize32,
    pub commands: Vec<CommandData>, // A Vec1 and a Vec have the same serialized representation
    pub max_cut: Usize32,
    pub skip_list: Vec<(Location, Usize32)>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CommandData {
    pub id: CmdId,
    pub priority: Priority,
    pub policy: Option<Bytes>,
    pub data: Bytes,
    pub updates: Vec<Update>,
}
