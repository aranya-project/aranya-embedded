use embassy_net::IpAddress;

use crate::net::NetworkAddr;

impl NetworkAddr for IpAddress {
    type Data = IpAddress;

    fn get(&self) -> IpAddress {
        self.clone()
    }
}
