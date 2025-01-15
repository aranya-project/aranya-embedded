#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

extern crate alloc;

mod built;
mod heap;
mod tasks;

use alloc::format;
use core::str::FromStr;
use embassy_executor::Spawner;
use embassy_net::tcp::TcpSocket;
use embassy_net::{IpListenEndpoint, Ipv4Address, Stack, StackResources};
use embassy_net::{Ipv4Cidr, StaticConfigV4};
use embassy_time::{Duration, Timer};
use embedded_io_async::Write;
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
use tasks::router_host::{connection, net_task, run_dhcp};

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

    // TCP receive and transmit buffer sizes. 1KB for each
    let mut rx_buffer = [0; 1024];
    let mut tx_buffer = [0; 1024];

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

    let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);

    println!("TCP server starting on port 8080");

    loop {
        println!("Waiting for connection...");

        match socket
            .accept(IpListenEndpoint {
                addr: None,
                port: 8080,
            })
            .await
        {
            Ok(_) => {
                println!("Client connected");

                // Create a buffer for reading
                let mut buf = [0u8; 1024];

                // Simple read loop
                loop {
                    match socket.read(&mut buf).await {
                        Ok(0) => {
                            println!("Connection closed");
                            break;
                        }
                        Ok(n) => {
                            println!("Received: {:?}", &buf[..n]);
                        }
                        Err(e) => {
                            println!("Read error: {:?}", e);
                            // Close the current socket connection
                            socket.close();
                            // Reset the socket by creating a new one
                            drop(socket);
                            socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                println!("Accept error: {:?}", e);
                // Close the current socket connection
                socket.close();
                // Reset the socket by creating a new one
                drop(socket);
                socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
                // Add a small delay before retrying
                Timer::after(Duration::from_millis(100)).await;
            }
        }
    }
}
