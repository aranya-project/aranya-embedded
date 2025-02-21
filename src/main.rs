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
use alloc::rc::Rc;
use embassy_executor::Spawner;
use embassy_net::tcp::TcpSocket;
use embassy_net::{IpListenEndpoint, StackResources};
use embassy_time::Duration;
use hardware::esp32_rng::esp32_getrandom;

use embedded_hal_bus::spi::ExclusiveDevice;
use embedded_sdmmc::{SdCard, Timestamp, VolumeManager};
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Io, Level, Output};
use esp_hal::spi::master::{Config, Spi};
use esp_hal::timer::timg::{Timer, TimerGroup};
use esp_hal_embassy::main;
use esp_println::println;
use esp_wifi::wifi::WifiStaDevice;
use esp_wifi::EspWifiController;
use hardware::esp32_time::Esp32TimeSource;
use heap::init_heap;
use log::info;
use owo_colors::OwoColorize;
use tasks::client::connection;
use tcp::sync::TcpSyncHandler;

// ! Panics will result in lockout if early enough so try to convert to using results that don't panic

// When you are okay with using a nightly compiler it's better to use https://docs.rs/static_cell/2.1.0/static_cell/macro.make_static.html
macro_rules! mk_static {
    ($t:ty,$val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write(($val));
        x
    }};
}

pub type VolumeMan = VolumeManager<
    SdCard<ExclusiveDevice<Spi<'static, esp_hal::Blocking>, Output<'static>, Delay>, Delay>,
    Esp32TimeSource<Timer>,
    4,
    4,
    1,
>;

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
    let spi: Spi<'static, esp_hal::Blocking> =
        Spi::new(peripherals.SPI2, Config::default()).unwrap();
    let spi: Spi<'static, esp_hal::Blocking> = spi.with_sck(sclk).with_mosi(mosi).with_miso(miso);

    // Initialize wifi device with mode
    let (mut device, controller) =
        esp_wifi::wifi::new_with_mode(wifi_cont, peripherals.WIFI, WifiStaDevice).unwrap();
    let config = embassy_net::Config::dhcpv4(Default::default());

    let mut buffer = [0u8; 8]; // u64 is 8 bytes
    esp32_getrandom(&mut buffer);
    let seed = u64::from_le_bytes(buffer);

    // Init network stack
    let (stack, runner) = embassy_net::new(
        device,
        config,
        mk_static!(StackResources<3>, StackResources::<3>::new()),
        seed,
    );

    /*
        // network interface (iface) instance
        let iface = smoltcp::iface::Interface::new(
            // I have no idea why using ethernet works here
            smoltcp::iface::Config::new(smoltcp::wire::HardwareAddress::Ethernet(
                smoltcp::wire::EthernetAddress::from_bytes(&device.mac_address()),
            )),
            &mut device,
            smoltcp::time::Instant::from_micros(now().duration_since_epoch().to_micros() as i64),
        );

        let mut socket_set_entries: [SocketStorage; 3] = Default::default();
        let mut socket_set = SocketSet::new(&mut socket_set_entries[..]);
        let mut dhcp_socket = smoltcp::socket::dhcpv4::Socket::new();
        // we can set a hostname here (or add other DHCP options)
        dhcp_socket.set_outgoing_options(&[DhcpOption {
            kind: 12,
            data: b"LighthouseWifi",
        }]);
        socket_set.add(dhcp_socket);

        let stack = Stack::new(iface, socket_set, rng.random());
    */
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
        embassy_time::Timer::after(Duration::from_millis(100)).await;
    }

    let volume_manager = Rc::new(VolumeManager::new(sd_card, esp_timer_source));

    // Wait a bit for net_task to initialize
    embassy_time::Timer::after(Duration::from_millis(100)).await;

    // Spawn collection of tasks that passively maintain the necessary aspects of the server which are:
    // Starting device as an access point
    spawner.spawn(connection(controller)).ok();

    println!("Waiting for connection...");

    let _buf = [0u8; 1024];
    let mut rx_buffer = [0; 1024];
    let mut tx_buffer = [0; 1024];
    loop {
        // TCP receive and transmit buffer sizes. 1KB for each
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

                let mut sync_handler =
                    TcpSyncHandler::new(socket, None, None, volume_manager.clone());
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
                embassy_time::Timer::after(Duration::from_millis(100)).await;
            }
        }
    }
}
