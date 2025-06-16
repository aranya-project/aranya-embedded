#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]
#![feature(core_io_borrowed_buf)]
#![feature(new_zeroed_alloc)]

extern crate alloc;

mod boards;
mod hardware;

use adafruit_seesaw::devices::{NeoKey1x4, SeesawDevice, SeesawDeviceInit};
use adafruit_seesaw::prelude::NeopixelModule;
use adafruit_seesaw::rgb::Rgb;
use adafruit_seesaw::SeesawRefCell;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer, WithTimeout};
use embedded_hal_bus::spi::ExclusiveDevice;
use embedded_sdmmc::{SdCard, VolumeIdx, VolumeManager};
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Input, Level, Output, Pull};
use esp_hal::i2c::master::I2c;
use esp_hal::spi::master::Spi;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::{i2c, Blocking};
use esp_hal_embassy::main;
use esp_println::{print, println};
use fugit::RateExtU32 as _;

use esp_rmt_neopixel::Neopixel;
use esp_irda_transceiver::IrdaTransceiver;
use log::info;

macro_rules! menu {
    ($title:expr, $($key:literal: $item:literal => $code:block),*) => {
        {
            println!("{}", $title);
            $(
                println!("{}) {}", $key, $item);
            )*
            print!("? ");
            let b = read_con_byte().await as char;
            println!("");
            match b {
                $($key => $code)*
                _ => ()
            }
        }
    };
}

struct SdParts<'a> {
    spi: Spi<'a, Blocking>,
    cs: Output<'a>,
    cd: Input<'a>,
}

async fn read_con_byte() -> u8 {
    let usb_jtag = unsafe { esp_hal::peripherals::USB_DEVICE::steal() };
    let mut usb_serial = esp_hal::usb_serial_jtag::UsbSerialJtag::new(usb_jtag);
    loop {
        match usb_serial.read_byte() {
            Ok(b) => return b,
            Err(nb::Error::WouldBlock) => Timer::after_millis(1).await,
        }
    }
}

#[main]
async fn main(_spawner: Spawner) {
    // Initialize peripherals
    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    let board_def = boards::board_def(peripherals);

    let timer_group1 = TimerGroup::new(board_def.peripherals.timg1);
    esp_hal_embassy::init(timer_group1.timer0);

    // Initialize heaps
    esp_alloc::heap_allocator!(96 * 1024);
    if let Some(psram) = board_def.peripherals.psram {
        esp_alloc::psram_allocator!(psram, esp_hal::psram);
    }

    esp_println::logger::init_logger_from_env();
    info!("Embassy initialized!");

    let mut acc_driver = board_def
        .accessory_power
        .map(|pin| Output::new(pin, Level::Low));
    if acc_driver.is_some() {
        info!("Board has accessory power control");
    }

    let mut neopixel = Neopixel::new(
        board_def.peripherals.rmt,
        board_def.neopixel.data,
        board_def.neopixel.power,
        board_def.neopixel.power_inverted,
    )
    .expect("could not initialize neopixel");

    let mut main_button = Input::new(board_def.button, Pull::Up);

    let mut i2c = I2c::new(board_def.peripherals.i2c0, i2c::master::Config::default())
        .expect("could not create i2c")
        .with_sda(board_def.i2c.sda)
        .with_scl(board_def.i2c.scl);

    let mut sd = board_def.sd.map(|sd| {
        let miso = Input::new(sd.miso, Pull::Up);
        SdParts {
            spi: Spi::new(
                board_def.peripherals.spi2,
                esp_hal::spi::master::Config::default().with_frequency(1.MHz()),
            )
            .unwrap()
            .with_sck(sd.sck)
            .with_mosi(sd.mosi)
            .with_miso(miso),
            cs: Output::new(sd.cs, Level::High),
            cd: Input::new(sd.cd, Pull::Up),
        }
    });

    let mut irts = board_def
        .ir
        .map(|ir| IrdaTransceiver::new(board_def.peripherals.uart1, ir.tx, ir.rx, ir.en));

    loop {
        menu!("Select test",
            'b': "Button" => { button_test(&mut main_button).await },
            'n': "Neopixel" => { led_test(&mut neopixel).await },
            'a': "Accessory power" => { accessory_power_test(&mut acc_driver).await },
            'q': "I2C/Qwiic" => { i2c_test(&mut acc_driver, &mut i2c).await },
            's': "SD/SPI" => { sd_test(&mut acc_driver, &mut sd).await },
            'i': "IR" => { ir_test(&mut acc_driver, &mut irts).await }
        );
    }
}

async fn button_test(button: &mut Input<'_>) {
    log::info!("testing button for ten seconds");
    async {
        loop {
            println!(
                "Button is {}",
                match button.level() {
                    Level::Low => "pressed",
                    Level::High => "not pressed",
                }
            );
            button.wait_for_any_edge().await;
        }
    }
    .with_timeout(Duration::from_secs(10))
    .await
    .ok();
    log::info!("done");
}

async fn led_test(neopixel: &mut Neopixel<'_>) {
    log::info!("neopixel test started");
    neopixel.set_power(true);
    neopixel.set_color(100, 100, 0).ok();
    Timer::after_millis(1500).await;
    neopixel.set_color(0, 100, 100).ok();
    Timer::after_millis(1500).await;
    neopixel.set_color(100, 0, 100).ok();
    Timer::after_millis(1500).await;
    neopixel.set_power(false);
    //neopixel.set_color(0, 0, 0);
    log::info!("neopixel test finished")
}

async fn accessory_power_test(acc_power: &mut Option<Output<'_>>) {
    let Some(acc_power) = acc_power else {
        log::error!("No accessory power control on this board");
        return;
    };
    acc_power.set_high();
    println!("Accessory power enabled. Press any key to disable and return.");
    read_con_byte().await;
    acc_power.set_low();
    println!("Accessory power disabled.");
}

async fn i2c_test(acc_power: &mut Option<Output<'_>>, i2c: &mut I2c<'_, Blocking>) {
    if let Some(acc_power) = acc_power {
        acc_power.set_high();
        // The Seesaw apparently needs like 50ms to boot
        Timer::after_millis(50).await;
    }
    let seesaw = SeesawRefCell::new(Delay::new(), i2c);
    let mut neokey = NeoKey1x4::new_with_default_addr(seesaw.acquire_driver())
        .init()
        .expect("Failed to start NeoKey1x4");
    neokey
        .set_neopixel_colors(&[
            Rgb::new(100, 0, 0).into(),
            Rgb::new(0, 100, 0).into(),
            Rgb::new(0, 0, 100).into(),
            Rgb::new(100, 0, 100).into(),
        ])
        .expect("could not set neopixel colors");
    neokey.sync_neopixel().expect("could not sync neopixel");
    println!("NeoKey1x4 LEDs lit. You should see red, green, blue, and magenta. Press any key to return.");
    println!("");
    read_con_byte().await;
    if let Some(acc_power) = acc_power {
        acc_power.set_low();
    }
}

async fn sd_test(acc_power: &mut Option<Output<'_>>, sd: &mut Option<SdParts<'_>>) {
    let Some(SdParts { spi, cs, cd }) = sd else {
        log::error!("No SD on this board");
        return;
    };
    if let Some(acc_power) = acc_power {
        log::info!("Enabling accessory power");
        acc_power.set_high();
    }
    let mut speed = 1.MHz();
    loop {
        spi.apply_config(&esp_hal::spi::master::Config::default().with_frequency(speed))
            .expect("could not set SPI speed");
        println!("SPI speed is {}", speed);
        menu!("SD/SPI",
            't': "Read SD card" => {
                read_sd(spi, cs).await;
            },
            'i': "Increase SPI speed" => {
                speed = speed * 2;
            },
            'd': "Decrease SPI speed" => {
                speed = speed / 2;
            },
            'c': "Card Detect" => {
                println!("Card Detect: {:?} ({})", cd.level(), match cd.level() {
                    Level::High => "card not present",
                    Level::Low => "card present",
                });
            },
            'x': "Exit" => { break; }
        );
    }

    if let Some(acc_power) = acc_power {
        log::info!("Disabling accessory power");
        acc_power.set_low();
    }
}

async fn read_sd(spi: &mut Spi<'_, Blocking>, cs: &mut Output<'_>) {
    let delay = Delay::new();
    let ex_device =
        ExclusiveDevice::new(spi, cs, delay).expect("Failed to set Exclusive SPI device");
    // ExclusiveDevice implements SpiDevice traits needed for SdCard
    let sd_card = SdCard::new(ex_device, delay);
    if sd_card.get_card_type().is_none() {
        log::error!("no card found");
        return;
    }
    println!("Card Type is {:?}", sd_card.get_card_type());

    let time_source = hardware::esp32_time::DummyTimeSource::new();
    let volume_manager = VolumeManager::new(sd_card, time_source);
    let volume0 = volume_manager
        .open_volume(VolumeIdx(0))
        .expect("could not open volume");
    log::info!("volume 0: {:?}", volume0);
    let root_dir = volume0.open_root_dir().expect("could not open root dir");
    root_dir
        .iterate_dir(|d| {
            println!("{:13} {:10} bytes", d.name, d.size);
        })
        .expect("could not iterate over root dir (filesystem corrupt?)");
}

async fn ir_test(acc_power: &mut Option<Output<'_>>, irts: &mut Option<IrdaTransceiver<'_>>) {
    let Some(irts) = irts else {
        log::error!("No IR on this board");
        return;
    };

    if let Some(acc_power) = acc_power {
        log::info!("Enabling accessory power");
        acc_power.set_high();
    }

    irts.enable(true);

    let ir_task = async {
        let mut buf = [0u8; 16];
        loop {
            irts.send(b"TEST\n").await.expect("could not send IR");
            match irts.read_nb(&mut buf) {
                Ok(n) => {
                    for b in &buf[..n] {
                        print!("{}", *b as char)
                    }
                }
                Err(e) => log::error!("{e}"),
            }
            Timer::after_secs(1).await;
        }
    };

    println!("Sending/receiving. Press any key to return.");

    embassy_futures::select::select(ir_task, read_con_byte()).await;

    irts.enable(false);

    if let Some(acc_power) = acc_power {
        log::info!("Disabling accessory power");
        acc_power.set_low();
    }
}

#[embassy_executor::task]
async fn heap_report() {
    loop {
        let stats = esp_alloc::HEAP.stats();
        log::info!("{}", stats);
        Timer::after_secs(10).await;
    }
}
