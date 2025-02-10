#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

extern crate alloc;

mod aranya;
mod built;
mod hardware;
mod heap;
mod tasks;
mod tcp;

use alloc::format;
use aranya::graph_store::GraphManager;
use aranya::sink::VecSink;
use aranya_crypto::default::DefaultEngine;
use aranya_crypto::Rng;
use aranya_runtime::linear::LinearStorageProvider;
use aranya_runtime::{ClientState, GraphId, PeerCache};
use core::str::FromStr;
use embassy_executor::Spawner;
use embassy_net::tcp::TcpSocket;
use embassy_net::{IpListenEndpoint, Ipv4Address, Stack, StackResources};
use embassy_net::{Ipv4Cidr, StaticConfigV4};
use embassy_time::{Duration, Timer};
use embedded_hal_bus::spi::ExclusiveDevice;
use embedded_sdmmc::{
    Directory, RawDirectory, RawVolume, SdCard, Timestamp, VolumeIdx, VolumeManager,
};
use esp_backtrace as _;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Io, Level, Output};
use esp_hal::peripheral::Peripheral;
use esp_hal::peripherals::TIMG1;
use esp_hal::spi::master::{Config, Spi};
use esp_hal::spi::SpiMode;
use esp_hal::timer::timg::TimerX;
use esp_hal::{prelude::*, timer::timg::TimerGroup};
use esp_println::println;
use esp_wifi::wifi::{WifiApDevice, WifiDevice};
use esp_wifi::EspWifiController;
use hardware::esp32_engine::ESP32Engine;
use hardware::esp32_time::Esp32TimeSource;
use heap::init_heap;
use log::info;
use owo_colors::OwoColorize;
use tasks::router_host::{connection, net_task, run_dhcp};
use tcp::sync::TcpSyncHandler;

// ! Panics will result in lockout if early enough so try to convert to using results that don't panic

pub type Client = ClientState<
    ESP32Engine<DefaultEngine>,
    LinearStorageProvider<
        GraphManager<
            '_,
            ExclusiveDevice<Spi<'static, esp_hal::Blocking>, Output<'static>, Delay>,
            Delay,
            Esp32TimeSource<TimerX<<TIMG1 as Peripheral>::P, 1>>,
        >,
    >,
>;

// When you are okay with using a nightly compiler it's better to use https://docs.rs/static_cell/2.1.0/static_cell/macro.make_static.html
macro_rules! mk_static {
    ($t:ty,$val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write(($val));
        x
    }};
}

#[main]
async fn main(spawner: Spawner) {
    init_heap();

    // Initialize peripherals
    let peripherals = esp_hal::init({
        let mut config = esp_hal::Config::default();
        config.cpu_clock = CpuClock::max();
        config
    });

    esp_println::logger::init_logger_from_env();

    // Initialize embassy timer from timer 0 in timer group 1
    let timer_g1 = TimerGroup::new(peripherals.TIMG1);
    esp_hal_embassy::init(timer_g1.timer0);

    info!("Embassy initialized!");

    // Initialize wifi control timer from timer 0 in timer group 0
    let timer_g0 = TimerGroup::new(peripherals.TIMG0);
    let wifi_cont = mk_static!(
        EspWifiController<'static>,
        esp_wifi::init(
            timer_g0.timer0,
            esp_hal::rng::Rng::new(peripherals.RNG),
            peripherals.RADIO_CLK,
        )
        .expect("Failed to initialize wifi controller")
    );

    // SD Card Timer Tracking Initialization
    println!("SD Card Timer intialization");
    // ! Add live update from server for timer tracking
    let start_time = Timestamp {
        year_since_1970: 54,
        zero_indexed_month: 7,
        zero_indexed_day: 14,
        hours: 12,
        minutes: 0,
        seconds: 0,
    };
    let esp_timer_source = Esp32TimeSource::new(timer_g1.timer1, start_time);

    // SD Card SPI Interface Setting
    println!("SD Card SPI Interface intialization");
    let _io: Io = Io::new(peripherals.IO_MUX);
    let sclk = peripherals.GPIO14;
    let miso = peripherals.GPIO2;
    let mosi = peripherals.GPIO15;
    let cs: Output<'static> = Output::new(peripherals.GPIO13, Level::High) as Output<'static>;
    let spi: Spi<'static, esp_hal::Blocking> = Spi::new_with_config(
        peripherals.SPI2,
        Config {
            frequency: 100u32.kHz(),
            mode: SpiMode::Mode0,
            ..Default::default()
        },
    );
    let spi: Spi<'static, esp_hal::Blocking> = spi.with_sck(sclk).with_mosi(mosi).with_miso(miso);

    // SD Card Initialization
    println!("SD Card intialization");
    let delay = Delay::new();
    let ex_device: ExclusiveDevice<Spi<'static, esp_hal::Blocking>, Output<'static>, Delay> =
        ExclusiveDevice::new(spi, cs, delay).expect("Failed to set Exclusive SPI device");
    // ExclusiveDevice implements SpiDevice traits needed for SdCard
    let sd_card: SdCard<
        ExclusiveDevice<Spi<'static, esp_hal::Blocking>, Output<'static>, Delay>,
        Delay,
    > = SdCard::new(ex_device, delay);
    println!(
        "{}",
        format!("Card Type is {:?}", sd_card.get_card_type()).blue()
    );
    // SD Card can take some time to initialize. This can cause a permanent loop if there is an error
    while sd_card.get_card_type().is_none() {
        println!(
            "{}",
            format!("Card Type is {:?}", sd_card.get_card_type()).blue()
        );
        Timer::after(Duration::from_millis(100)).await;
    }

    let volume_manager: &mut VolumeManager<
        SdCard<ExclusiveDevice<Spi<'static, esp_hal::Blocking>, Output<'static>, Delay>, Delay>,
        Esp32TimeSource<TimerX<<TIMG1 as Peripheral>::P, 1>>,
    > = mk_static!(
        VolumeManager<
            SdCard<ExclusiveDevice<Spi<'static, esp_hal::Blocking>, Output<'static>, Delay>, Delay>,
            Esp32TimeSource<TimerX<<TIMG1 as Peripheral>::P, 1>>,
        >,
        VolumeManager::new(sd_card, esp_timer_source)
    );

    // Wifi peripheral and implementation initialization
    let wifi = peripherals.WIFI;
    let (wifi_interface, controller) =
        esp_wifi::wifi::new_with_mode(wifi_cont, wifi, WifiApDevice).unwrap();

    // Server IP set
    let gw_ip_addr_str = "192.168.2.1";
    let gw_ip_addr = Ipv4Address::from_str(gw_ip_addr_str).expect("failed to parse gateway ip");

    // Main configuration for the wifi stack in how it should set up internal and external IPs.
    let config = embassy_net::Config::ipv4_static(StaticConfigV4 {
        address: Ipv4Cidr::new(gw_ip_addr, 24), // Creates a subnet with mask /24 (255.255.255.0)
        gateway: Some(gw_ip_addr),              // Sets the gateway IP
        dns_servers: Default::default(),        // No DNS servers configured
    });

    let seed = 1234; // Used for generating TCP/IP sequence numbers. //! Bad seed but acceptable for demonstration

    // Init network stack hosted on ESP32
    let stack = &*mk_static!(
        Stack<WifiDevice<'_, WifiApDevice>>,
        Stack::new(
            wifi_interface,
            config,
            mk_static!(StackResources<16>, StackResources::<16>::new()),
            seed
        )
    );

    // Spawn collection of tasks that passively maintain the necessary aspects of the server which are:
    // Starting device as an access point
    spawner.spawn(connection(controller)).ok();
    // Running the network stack for handling communication events
    spawner.spawn(net_task(stack)).ok();
    // Running DHCP for internal IP setting
    spawner.spawn(run_dhcp(stack, gw_ip_addr_str)).ok();

    loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    while !stack.is_config_up() {
        Timer::after(Duration::from_millis(100)).await
    }
    stack
        .config_v4()
        .inspect(|c| println!("ipv4 config: {c:?}"));

    //let effect_sink = VecSink::new();

    println!("TCP server starting on port 8080");

    println!("Waiting for connection...");

    let _buf = [0u8; 1024];
    //loop {
    // TCP receive and transmit buffer sizes. 1KB for each
    let mut rx_buffer = [0; 1024];
    let mut tx_buffer = [0; 1024];
    let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);

    match socket
        .accept(IpListenEndpoint {
            addr: None,
            port: 8080,
        })
        .await
    {
        Ok(_) => {
            println!("Client connected");

            // Aranya Graph, Manager, and State Initialization
            // This default graph ID is not used for anything beyond initializing the SDIo manager. The real graph_id is set later by `new_graph` as each ID corresponds to a policy with a given action
            // Create New Graph With the Specified Effect Sink

            let mut sync_handler = TcpSyncHandler::new(socket, None, None, volume_manager);
            sync_handler
                .handle_connection()
                .await
                .expect("Failed to handle connection");
        }
        Err(e) => {
            // todo properly encase in a loop so that repeated attempts can be made to set up and use TCP sockets
            println!("Accept error: {:?}", e);
            // Close the current socket connection
            socket.abort();
            drop(socket);
            Timer::after(Duration::from_millis(100)).await;
        }
    }
    //}
}
