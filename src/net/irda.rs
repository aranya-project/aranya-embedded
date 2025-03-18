#![cfg(feature = "net-irda")]

use super::Network;

pub struct IrdaNetwork {}

impl Network for IrdaNetwork {
    type Addr = u16;
    type TxId = u16;

    async fn send_request(
        &self,
        to: u16,
        req: alloc::vec::Vec<u8>,
    ) -> Result<u16, super::NetworkError> {
        todo!()
    }

    async fn recv_response(
        &self,
        tx_id: u16,
    ) -> Result<alloc::vec::Vec<u8>, super::NetworkError> {
        todo!()
    }
    
    async fn accept(&self) -> Result<(Self::TxId, alloc::vec::Vec<u8>), super::NetworkError> {
        todo!()
    }
    
    async fn send_response(&self, tx_id: Self::TxId, resp: alloc::vec::Vec<u8>) -> Result<(), super::NetworkError> {
        todo!()
    }
}

pub async fn start() -> IrdaNetwork {
    // TODO(chip): the whole thing
    IrdaNetwork {}
}