use embedded_hal::{delay::DelayNs, spi::SpiDevice};
use embedded_sdmmc::{SdCard, TimeSource, VolumeManager};

pub struct SdCardManager<SPI, DELAY, TS>
where
    SPI: SpiDevice,
    DELAY: DelayNs,
    TS: TimeSource,
{
    volume_mgr: VolumeManager<SdCard<SPI, DELAY>, TS>,
}

impl<SPI, DELAY, TS> SdCardManager<SPI, DELAY, TS>
where
    SPI: SpiDevice,
    DELAY: DelayNs,
    TS: TimeSource,
{
    pub fn new(sdcard: SdCard<SPI, DELAY>, time_source: TS) -> Self {
        let volume_mgr = VolumeManager::new(sdcard, time_source);
        SdCardManager { volume_mgr }
    }
}
