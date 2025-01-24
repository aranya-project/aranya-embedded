use alloc::format;
use aranya_runtime::StorageError;
use embedded_hal::{delay::DelayNs, spi::SpiDevice};
use embedded_sdmmc::{Mode, SdCard, TimeSource, VolumeIdx, VolumeManager};
use esp_println::println;
use owo_colors::OwoColorize;

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

    pub fn with_file<F, R>(
        &self,
        file_name: &str,
        mode: Mode,
        start_offset: u32,
        f: F,
    ) -> Result<R, StorageError>
    where
        F: Fn(
            &mut embedded_sdmmc::File<'_, SdCard<SPI, DELAY>, TS, 4, 4, 1>,
        ) -> Result<R, StorageError>,
    {
        let raw_volume = self.volume_mgr.open_raw_volume(VolumeIdx(0)).map_err(|e| {
            println!("Error Opening Raw Volume: {:?}", format!("{:?}", e).red());
            StorageError::IoError
        })?;

        let root_directory = self
            .volume_mgr
            .open_root_dir(raw_volume)
            .map(|root| root.to_directory(&self.volume_mgr))
            .map_err(|e| {
                println!("Error Opening Root Dir: {:?}", format!("{:?}", e).red());
                StorageError::IoError
            })?;

        let mut file = root_directory
            .open_file_in_dir(file_name, mode)
            .map_err(|e| {
                println!(
                    "Error Opening File {} in Root Directory: {:?}",
                    file_name,
                    format!("{:?}", e).red()
                );
                StorageError::NoSuchStorage
            })?;

        file.seek_from_start(start_offset).map_err(|e| {
            println!(
                "Failed to set cursor to offset {} bytes from the start of file: {:?}",
                start_offset,
                format!("{:?}", e).red()
            );
            StorageError::IoError
        })?;

        let result = f(&mut file);

        file.close().map_err(|e| {
            println!("Failed to Close File: {:?}", format!("{:?}", e).red());
            StorageError::IoError
        })?;
        root_directory.close().map_err(|e| {
            println!("Failed to Close Directory: {:?}", format!("{:?}", e).red());
            StorageError::IoError
        })?;
        self.volume_mgr.close_volume(raw_volume).map_err(|e| {
            println!(
                "Failed to Close Volume Manager: {:?}",
                format!("{:?}", e).red()
            );
            StorageError::IoError
        })?;

        result
    }
}
