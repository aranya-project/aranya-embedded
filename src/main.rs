use std::fs::File;
use std::io::prelude::*;

use embedded_graphics::{
    mono_font::{ascii::*, MonoTextStyleBuilder},
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Baseline, Text},
};
use esp_idf_svc::fs::littlefs::Littlefs;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::gpio::PinDriver;
use esp_idf_svc::hal::i2c::{I2cConfig, I2cDriver};
use esp_idf_svc::hal::prelude::*;
use esp_idf_svc::io::vfs::MountedLittlefs;
use ssd1306::{mode::DisplayConfig, size::DisplaySize128x64};
use ssd1306::prelude::DisplayRotation;
use ssd1306::{I2CDisplayInterface, Ssd1306};

fn main() -> anyhow::Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Hello, world!");

    log::info!("Grabbing the LittleFS partition");
    let mut littlefs = unsafe { Littlefs::<()>::new_partition("storage")? };

    log::info!("Formatting LittleFS partition");
    littlefs.format()?;

    log::info!("Mounting the LittleFS partition");
    let mount = MountedLittlefs::mount(littlefs, "/littlefs")?;

    {
        let mut file = File::create("/littlefs/hello_world.txt")?;
        file.write_all(b"This is data read from a file!")?;
    }

    let data = std::fs::read("/littlefs/hello_world.txt")?;
    log::info!("{}", std::str::from_utf8(&data)?);

    let info = mount.info()?;
    log::info!(
        "Total: {} bytes, Used: {} bytes",
        info.total_bytes,
        info.used_bytes
    );

    let peripherals = Peripherals::take()?;

    let config = I2cConfig::new().baudrate(100.kHz().into());
    let i2c = I2cDriver::new(peripherals.i2c0, peripherals.pins.gpio18, peripherals.pins.gpio17, &config)?;

    let interface = I2CDisplayInterface::new(i2c);
    let mut display = Ssd1306::new(interface, DisplaySize128x64, DisplayRotation::Rotate0)
        .into_buffered_graphics_mode();
    display.init().unwrap();

    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT_9X18)
        .text_color(BinaryColor::On)
        .build();

    Text::with_baseline("Hello, world!", Point::zero(), text_style, Baseline::Top)
        .draw(&mut display)
        .unwrap();

    Text::with_baseline("Hello, Rust!", Point::new(0, 20), text_style, Baseline::Top)
        .draw(&mut display)
        .unwrap();

    display.flush().unwrap();

    let mut led = PinDriver::output(peripherals.pins.gpio37)?;

    loop {
        led.set_high()?;
        // we are sleeping here to make sure the watchdog isn't triggered
        FreeRtos::delay_ms(1000);

        led.set_low()?;
        FreeRtos::delay_ms(1000);
    }
}
