use core::str::FromStr;

use embassy_net::Stack;
use embassy_time::{Duration, Timer};
use esp_println::println;
use esp_wifi::wifi::{
    Configuration, WifiApDevice, WifiController, WifiDevice, WifiEvent, WifiState,
};

use crate::built::wifi_config::wifi_config;

/// This gives each device connecting there own Ipv4 address
#[embassy_executor::task]
pub async fn run_dhcp(
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

/// This handles setting up a ESP32 router or access point
#[embassy_executor::task]
pub async fn connection(mut controller: WifiController<'static>) {
    println!("start connection task");
    println!("Device capabilities: {:?}", controller.capabilities());
    loop {
        // If ESP32 is an access point wait until we're no longer connected and then delay 5s before continuing.
        if esp_wifi::wifi::wifi_state() == WifiState::ApStarted {
            controller.wait_for_event(WifiEvent::ApStop).await;
            Timer::after(Duration::from_millis(5000)).await
        }
        // If ESP32 controller has not started configure it as an access point with the specified configurations
        if !matches!(controller.is_started(), Ok(true)) {
            let client_config = Configuration::AccessPoint(wifi_config());
            controller.set_configuration(&client_config).unwrap();
            println!("Starting wifi");
            controller.start_async().await.unwrap();
            println!("Wifi started!");
        }
    }
}

/// This runs the entirety of the network stack
#[embassy_executor::task]
pub async fn net_task(stack: &'static Stack<WifiDevice<'static, WifiApDevice>>) {
    stack.run().await
}
