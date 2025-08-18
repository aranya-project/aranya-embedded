#![cfg(feature = "storage-sd")]

pub mod io_manager;

use alloc::{format, rc::Rc};

use aranya_runtime::linear::LinearStorageProvider;
use embassy_time::Duration;
use embedded_hal_bus::spi::ExclusiveDevice;
use embedded_sdmmc::{SdCard, Timestamp, VolumeManager};
use esp_hal::{
    delay::Delay,
    gpio::{InputPin, Level, Output, OutputPin},
    peripheral::Peripheral,
    spi::{self, master::Spi},
    timer::timg,
};
use fugit::RateExtU32 as _;
pub use io_manager::GraphManager;
use owo_colors::OwoColorize;

use super::StorageError;
use crate::hardware::esp32_time::Esp32TimeSource;

pub type VolumeMan = VolumeManager<
    SdCard<ExclusiveDevice<Spi<'static, esp_hal::Blocking>, Output<'static>, Delay>, Delay>,
    Esp32TimeSource<timg::Timer>,
    4,
    4,
    1,
>;

pub async fn init(
    spi: impl Peripheral<P = impl spi::master::PeripheralInstance> + 'static,
    sclk: impl Peripheral<P = impl OutputPin> + 'static,
    mosi: impl Peripheral<P = impl OutputPin> + 'static,
    miso: impl Peripheral<P = impl InputPin> + 'static,
    cs: impl Peripheral<P = impl OutputPin> + 'static,
    timer: timg::Timer,
) -> Result<LinearStorageProvider<GraphManager>, StorageError> {
    // SD Card Timer Tracking Initialization
    log::info!("SD Card Timer intialization");
    // ! Add live update from server for timer tracking
    let start_time = Timestamp {
        year_since_1970: 54,
        zero_indexed_month: 7,
        zero_indexed_day: 14,
        hours: 12,
        minutes: 0,
        seconds: 0,
    };
    let esp_timer_source = Esp32TimeSource::new(timer, start_time);

    // SD Card SPI Interface Setting
    log::info!("SD Card SPI Interface intialization");
    //let _io: Io = Io::new(peripherals.IO_MUX);
    let cs: Output<'static> = Output::new(cs, Level::High);

    let spi: Spi<'static, esp_hal::Blocking> = Spi::new(
        spi,
        esp_hal::spi::master::Config::default().with_frequency(8.MHz()),
    )
    .unwrap();
    let spi: Spi<'static, esp_hal::Blocking> = spi.with_sck(sclk).with_mosi(mosi).with_miso(miso);

    // SD Card Initialization
    log::info!("SD Card intialization");
    let delay = Delay::new();
    let ex_device: ExclusiveDevice<Spi<'static, esp_hal::Blocking>, Output<'static>, Delay> =
        ExclusiveDevice::new(spi, cs, delay).expect("Failed to set Exclusive SPI device");
    // ExclusiveDevice implements SpiDevice traits needed for SdCard
    let sd_card: SdCard<
        ExclusiveDevice<Spi<'static, esp_hal::Blocking>, Output<'static>, Delay>,
        Delay,
    > = SdCard::new(ex_device, delay);
    log::info!(
        "{}",
        format!("Card Type is {:?}", sd_card.get_card_type()).blue()
    );
    // SD Card can take some time to initialize. This can cause a permanent loop if there is an error
    while sd_card.get_card_type().is_none() {
        log::info!(
            "{}",
            format!("Card Type is {:?}", sd_card.get_card_type()).blue()
        );
        embassy_time::Timer::after(Duration::from_millis(100)).await;
    }

    let volume_manager = Rc::new(VolumeManager::new(sd_card, esp_timer_source));
    let io_manager = GraphManager::new(volume_manager)?;
    Ok(LinearStorageProvider::new(io_manager))
}
