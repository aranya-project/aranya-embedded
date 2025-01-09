#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

use core::str::FromStr;

use embassy_executor::Spawner;
use embassy_net::tcp::TcpSocket;
use embassy_net::{IpListenEndpoint, Ipv4Address, Stack, StackResources};
use embassy_net::{Ipv4Cidr, StaticConfigV4};
use embassy_time::{Duration, Timer};
use esp_alloc::heap_allocator;
use esp_backtrace as _;
use esp_hal::{prelude::*, timer::timg::TimerGroup};
use esp_println::println;
use esp_wifi::wifi::{
    AccessPointConfiguration, Configuration, WifiApDevice, WifiController, WifiDevice, WifiEvent,
    WifiState,
};
use esp_wifi::EspWifiController;
use heap::init_heap;
use log::info;

extern crate alloc;

mod built;
mod heap;
use built::wifi_config::wifi_config;

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

    let wifi = peripherals.WIFI;
    let (wifi_interface, mut controller) =
        esp_wifi::wifi::new_with_mode(wifi_cont, wifi, WifiApDevice).unwrap();

    // Configuration as an access point meaning that other devices access it as a router
    controller
        .set_configuration(&Configuration::AccessPoint(wifi_config()))
        .unwrap();

    let gw_ip_addr_str = "192.168.2.1";
    let gw_ip_addr = Ipv4Address::from_str(gw_ip_addr_str).expect("failed to parse gateway ip");

    let config = embassy_net::Config::ipv4_static(StaticConfigV4 {
        address: Ipv4Cidr::new(gw_ip_addr, 24), // Creates a subnet with mask /24 (255.255.255.0)
        gateway: Some(gw_ip_addr),              // Sets the gateway IP
        dns_servers: Default::default(),        // No DNS servers configured
    });

    let seed = 1234; // Used for generating TCP/IP sequence numbers

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

    spawner.spawn(connection(controller)).ok();
    spawner.spawn(net_task(&stack)).ok();
    spawner.spawn(run_dhcp(&stack, gw_ip_addr_str)).ok();

    let mut rx_buffer = [0; 128];
    let mut tx_buffer = [0; 128];

    loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }
    println!(
        "Connect to the AP `esp-wifi` and point your browser to http://{gw_ip_addr_str}:8080/"
    );
    println!("DHCP is enabled so there's no need to configure a static IP, just in case:");
    while !stack.is_config_up() {
        Timer::after(Duration::from_millis(100)).await
    }
    stack
        .config_v4()
        .inspect(|c| println!("ipv4 config: {c:?}"));

    let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
    socket.set_timeout(Some(embassy_time::Duration::from_secs(10)));
    loop {
        println!("Wait for connection...");
        let r = socket
            .accept(IpListenEndpoint {
                addr: None,
                port: 8080,
            })
            .await;
        println!("Connected...");

        if let Err(e) = r {
            println!("connect error: {:?}", e);
            continue;
        }

        use embedded_io_async::Write;

        let mut buffer = [0u8; 1024];
        let mut pos = 0;
        loop {
            match socket.read(&mut buffer).await {
                Ok(0) => {
                    println!("read EOF");
                    break;
                }
                Ok(len) => {
                    let to_print =
                        unsafe { core::str::from_utf8_unchecked(&buffer[..(pos + len)]) };

                    if to_print.contains("\r\n\r\n") {
                        println!("{}", to_print);
                        break;
                    }

                    pos += len;
                }
                Err(e) => {
                    println!("read error: {:?}", e);
                    break;
                }
            };
        }

        let r = socket
            .write_all(
                b"HTTP/1.0 200 OK\r\n\r\n\
            <html>\
                <body>\
                    <h1>Hello Rust! Hello esp-wifi!</h1>\
                </body>\
            </html>\r\n\
            ",
            )
            .await;
        if let Err(e) = r {
            println!("write error: {:?}", e);
        }

        let r = socket.flush().await;
        if let Err(e) = r {
            println!("flush error: {:?}", e);
        }
        Timer::after(Duration::from_millis(1000)).await;

        socket.close();
        Timer::after(Duration::from_millis(1000)).await;

        socket.abort();
    }
}
#[embassy_executor::task]
async fn run_dhcp(
    stack: &'static Stack<WifiDevice<'static, WifiApDevice>>,
    gw_ip_addr: &'static str,
) {
    use core::net::{Ipv4Addr, SocketAddrV4};

    use edge_dhcp::{
        io::{self, DEFAULT_SERVER_PORT},
        server::{Server, ServerOptions},
    };
    use edge_nal::UdpBind;
    use edge_nal_embassy::{Udp, UdpBuffers};

    let ip = Ipv4Addr::from_str(gw_ip_addr).expect("dhcp task failed to parse gw ip");

    let mut buf = [0u8; 1500];

    let mut gw_buf = [Ipv4Addr::UNSPECIFIED];

    let buffers = UdpBuffers::<3, 1024, 1024, 10>::new();
    let unbound_socket = Udp::new(stack, &buffers);
    let mut bound_socket = unbound_socket
        .bind(core::net::SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::UNSPECIFIED,
            DEFAULT_SERVER_PORT,
        )))
        .await
        .unwrap();

    loop {
        _ = io::server::run(
            &mut Server::<64>::new(ip),
            &ServerOptions::new(ip, Some(&mut gw_buf)),
            &mut bound_socket,
            &mut buf,
        )
        .await
        .inspect_err(|e| log::warn!("DHCP server error: {e:?}"));
        Timer::after(Duration::from_millis(500)).await;
    }
}

#[embassy_executor::task]
async fn connection(mut controller: WifiController<'static>) {
    println!("start connection task");
    println!("Device capabilities: {:?}", controller.capabilities());
    loop {
        match esp_wifi::wifi::wifi_state() {
            WifiState::ApStarted => {
                // wait until we're no longer connected
                controller.wait_for_event(WifiEvent::ApStop).await;
                Timer::after(Duration::from_millis(5000)).await
            }
            _ => {}
        }
        if !matches!(controller.is_started(), Ok(true)) {
            let client_config = Configuration::AccessPoint(AccessPointConfiguration {
                ssid: "esp-wifi".try_into().unwrap(),
                ..Default::default()
            });
            controller.set_configuration(&client_config).unwrap();
            println!("Starting wifi");
            controller.start_async().await.unwrap();
            println!("Wifi started!");
        }
    }
}

#[embassy_executor::task]
async fn net_task(stack: &'static Stack<WifiDevice<'static, WifiApDevice>>) {
    stack.run().await
}
