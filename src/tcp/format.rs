use alloc::vec::Vec;
use aranya_runtime::GraphId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Commands {
    GetGraphID,
    SendGraphID(GraphId),
    GetSyncRequest,
    SendSyncRequest(Vec<u8>),
    DeserializeError, // Instigate rerequest
}
