pub mod irda;
pub mod wifi;

#[cfg(not(any(feature = "net-wifi", feature = "net-irda")))]
compile_error!("One of \"net-wifi\" or \"net-irda\" must be enabled");

use alloc::{string::String, vec::Vec};
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum NetworkError {
    #[error("Accept error")]
    Accept(String),
    #[error("Connect error")]
    Connect(String),
    #[error("Stream error")]
    Stream(String),
    #[error("Message too large")]
    MessageTooLarge,
}

pub(crate) trait Network {
    // The type of a peer address on this network
    type Addr: Copy;
    // A handle for a send/response transaction
    type TxId: Copy;

    /// Sends a message on the network and returns a transaction ID for the response.
    async fn send_request(&self, to: Self::Addr, req: Vec<u8>) -> Result<Self::TxId, NetworkError>;
    /// Waits until a message is received from the network with the given transaction ID.
    /// This can be called multiple times but only one call will succeed.
    async fn recv_response(&self, tx_id: Self::TxId) -> Result<Vec<u8>, NetworkError>;
    /// Wait for an incoming message. Returns a pair of ([`TxId`], `Vec<u8>`).
    async fn accept(&self) -> Result<(Self::TxId, Vec<u8>), NetworkError>;
    /// Sends a response to a message received with [`listen`].
    async fn send_response(&self, tx_id: Self::TxId, resp: Vec<u8>) -> Result<(), NetworkError>;
}
