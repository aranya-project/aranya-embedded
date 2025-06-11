use embedded_sdmmc::{TimeSource, Timestamp};

/// A dummy timer that doesn't actually keep track of time. Just needed to run the SD card access.
pub struct DummyTimeSource {}

impl DummyTimeSource
{
    pub fn new() -> Self {
        DummyTimeSource {}
    }
}

impl TimeSource for DummyTimeSource
{
    fn get_timestamp(&self) -> Timestamp {
        Timestamp::from_fat(0, 0)
    }
}
