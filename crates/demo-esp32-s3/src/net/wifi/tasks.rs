use core::str::FromStr;

use embassy_net::Runner;
use esp_println::println;
use esp_wifi::wifi::{
    ClientConfiguration, Configuration, WifiController, WifiDevice, WifiEvent, WifiStaDevice,
};

use crate::built::wifi_config::{WIFI_PASSWORD, WIFI_SSID};
/// This handles setting up a ESP32 as a client
#[embassy_executor::task]
pub async fn connection(mut controller: WifiController<'static>) {
    println!("start connection task");
    println!("Device capabilities: {:?}", controller.capabilities());

    // If ESP32 controller has not started configure it as an access point with the specified configurations
    if !matches!(controller.is_started(), Ok(true)) {
        let client_config = Configuration::Client(ClientConfiguration::default());
        controller.set_configuration(&client_config).unwrap();
        println!("Starting wifi");
        controller.start_async().await.unwrap();
        println!("Wifi started!");

        let mut connection_successful = false;
        let max_retries = 5;
        let mut retry_count = 0;

        while !connection_successful && retry_count < max_retries {
            println!(
                "Scanning for networks, attempt {}/{}",
                retry_count + 1,
                max_retries
            );

            match controller.scan_n::<25>() {
                Ok(access_points) => {
                    let wifi_ssid: heapless::String<32> =
                        heapless::String::from_str(WIFI_SSID).unwrap();
                    let wifi_password: heapless::String<64> =
                        heapless::String::from_str(WIFI_PASSWORD).unwrap();

                    if let Some(router) = access_points.0.iter().find(|n| n.ssid == wifi_ssid) {
                        println!("Found router: {}", wifi_ssid);
                        let client_config = ClientConfiguration {
                            ssid: wifi_ssid,
                            bssid: Some(router.bssid),
                            auth_method: router.auth_method.unwrap(),
                            password: wifi_password,
                            channel: Some(router.channel),
                        };

                        let client_config = Configuration::Client(client_config);

                        if let Err(e) = controller.set_configuration(&client_config) {
                            println!("Failed to set client configuration: {:?}", e);
                            retry_count += 1;
                            embassy_time::Timer::after_secs(5).await; // Wait before retrying
                            continue;
                        }

                        if let Err(e) = controller.connect() {
                            println!("Failed to connect to network: {:?}", e);
                            retry_count += 1;
                            embassy_time::Timer::after_secs(5).await; // Wait before retrying
                            continue;
                        }

                        println!("Connecting to access point...");

                        // Wait for connection with timeout
                        let connection_result = embassy_time::with_timeout(
                            embassy_time::Duration::from_secs(15),
                            controller.wait_for_event(WifiEvent::StaConnected),
                        )
                        .await;

                        if connection_result.is_err() {
                            println!("Connection timeout");
                            retry_count += 1;
                            embassy_time::Timer::after_secs(5).await; // Wait before retrying
                            continue;
                        }

                        println!("Connected to access point");
                        connection_successful = true;
                    } else {
                        println!("Failed to find router: {}. Retrying...", wifi_ssid);
                        retry_count += 1;
                        embassy_time::Timer::after_secs(5).await; // Wait before retrying
                    }
                }
                Err(e) => {
                    println!("Failed to scan for networks: {:?}", e);
                    retry_count += 1;
                    embassy_time::Timer::after_secs(5).await; // Wait before retrying
                }
            }
        }

        if !connection_successful {
            println!("Failed to connect after {} attempts", max_retries);
            // Handle persistent failure case - you can customize this based on your needs
            // Instead of panic, you might want to return an error or restart the device
            return;
        }

        println!("Capabilities: {:?}", controller.capabilities());
        println!("Configuration: {:?}", controller.configuration());
        // Now print the IP address
    }
}

#[embassy_executor::task]
pub async fn net_task(mut runner: Runner<'static, WifiDevice<'static, WifiStaDevice>>) {
    runner.run().await
}
