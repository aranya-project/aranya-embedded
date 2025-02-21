use embassy_time::{Duration, Timer};
use esp_println::println;
use esp_wifi::wifi::{Configuration, WifiController, WifiEvent, WifiState};

use crate::built::wifi_config::wifi_config;

/// This handles setting up a ESP32 as an accessor
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
            let client_config = Configuration::Client(wifi_config());
            controller.set_configuration(&client_config).unwrap();
            println!("Starting wifi");
            controller.start_async().await.unwrap();
            println!("Wifi started!");

            // Wait for connection to be established
            controller.wait_for_event(WifiEvent::StaConnected).await;
            println!("Connected to access point");

            // Now print the IP address
        }
    }
}
