pub mod irda;
pub mod wifi;

#[cfg(not(any(feature = "net-wifi", feature = "net-irda")))]
compile_error!("One of \"net-wifi\" or \"net-irda\" must be enabled");

use alloc::{boxed::Box, string::String};
use thiserror::Error;

/// NetworkError is intentionally opaque as it may be produced by any
/// [`Network`] implementation.
#[derive(Debug, Error)]
pub(crate) enum NetworkError {
    #[error("Send error: {0}")]
    Send(String),
    #[error("Receive error: {0}")]
    Receive(String),
}

/// `Message` is a sequence of bytes with addressing information, given
/// to or produced by the a [`Network`] implementation.
#[derive(Debug)]
pub struct Message<A> {
    /// Sender address.
    pub sender: A,
    /// Recipient address.
    pub recipient: A,
    /// The payload.
    pub contents: Box<[u8]>,
}

impl<A: Default> Message<A> {
    pub fn new(sender: A, recipient: A, contents: impl Into<Box<[u8]>>) -> Message<A> {
        Message {
            sender,
            recipient,
            contents: contents.into(),
        }
    }

    pub fn new_to(recipient: A, contents: impl Into<Box<[u8]>>) -> Message<A> {
        Message {
            sender: Default::default(),
            recipient,
            contents: contents.into(),
        }
    }
}

pub(crate) trait Network {
    // The type of a peer address on this network
    type Addr: Copy;

    /// Sends a message on the network.
    async fn send_message(&self, msg: Message<Self::Addr>) -> Result<(), NetworkError>;
    /// Waits until a message is received from the network.
    async fn recv_message(&self) -> Result<Message<Self::Addr>, NetworkError>;
}
