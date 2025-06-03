#![cfg(feature = "net-wifi")]

//mod addr;
mod tasks;

use core::sync::atomic::{AtomicU32, Ordering};

use alloc::collections::btree_map::BTreeMap;
use alloc::{format, string::String, vec::Vec};
use embassy_executor::Spawner;
use embassy_net::IpListenEndpoint;
use embassy_net::{
    tcp::{ConnectError, TcpSocket},
    IpAddress, IpEndpoint, Stack, StackResources,
};
use embedded_io_async::Write;
use esp_hal::peripheral::Peripheral;
use esp_wifi::{wifi::WifiStaDevice, EspWifiController, EspWifiRngSource, EspWifiTimerSource};
use tasks::{connection, net_task};
//use tasks::client::connection;

use super::{Network, NetworkError};

type Mutex<T> = embassy_sync::mutex::Mutex<embassy_sync::blocking_mutex::raw::NoopRawMutex, T>;

const TCP_PORT: u16 = 5080;

impl From<ConnectError> for NetworkError {
    fn from(value: ConnectError) -> Self {
        NetworkError::Connect(format!("{:?}", value))
    }
}

impl From<embassy_net::tcp::Error> for NetworkError {
    fn from(value: embassy_net::tcp::Error) -> Self {
        match value {
            embassy_net::tcp::Error::ConnectionReset => {
                NetworkError::Stream(String::from("connection reset"))
            }
        }
    }
}

impl From<embassy_net::tcp::AcceptError> for NetworkError {
    fn from(value: embassy_net::tcp::AcceptError) -> Self {
        NetworkError::Accept(format!("{value:?}"))
    }
}

pub async fn start<'a, TIM>(
    wifi: impl Peripheral<P = esp_hal::peripherals::WIFI> + 'static,
    radio_clock: impl Peripheral<P = esp_hal::peripherals::RADIO_CLK> + 'static,
    timer: impl Peripheral<P = TIM> + 'static,
    rng: impl EspWifiRngSource,
    spawner: Spawner,
) -> WifiNetwork<'a>
where
    TIM: EspWifiTimerSource,
{
    let wifi_cont = crate::mk_static!(
        EspWifiController<'static>,
        esp_wifi::init(timer, rng, radio_clock).expect("Failed to initialize wifi controller")
    );

    // Initialize wifi device with mode
    let (device, controller) =
        esp_wifi::wifi::new_with_mode(wifi_cont, wifi, WifiStaDevice).unwrap();
    let config = embassy_net::Config::dhcpv4(Default::default());

    let mut buffer = [0u8; 8]; // u64 is 8 bytes
    getrandom::getrandom(&mut buffer).expect("get random seed");
    let seed = u64::from_le_bytes(buffer);

    // Init network stack
    let (stack, runner) = embassy_net::new(
        device,
        config,
        crate::mk_static!(StackResources<3>, StackResources::<3>::new()),
        seed,
    );

    // Spawn collection of tasks that passively maintain the necessary aspects of the server which are:
    // Starting device as an access point
    spawner.spawn(connection(controller)).ok();
    spawner.spawn(net_task(runner)).ok();

    // Wait for network stack to come up
    log::info!("Waiting for network to come up...");
    stack.wait_config_up().await;
    log::info!("Network up: {:?}", stack.config_v4().unwrap());

    WifiNetwork::new(stack)
}

pub struct WifiNetwork<'a> {
    stack: Stack<'a>,
    tx_id: AtomicU32,
    pending_responses: Mutex<BTreeMap<u32, Vec<u8>>>,
}

impl<'a> WifiNetwork<'a> {
    pub fn new(stack: Stack<'a>) -> WifiNetwork<'a> {
        WifiNetwork {
            stack,
            tx_id: AtomicU32::new(0),
            pending_responses: Mutex::new(BTreeMap::new()),
        }
    }
}

impl Network for WifiNetwork<'_> {
    type Addr = IpAddress;
    type TxId = u32;

    async fn send_request(&self, to: IpAddress, req: Vec<u8>) -> Result<u32, NetworkError> {
        let mut rx_buffer = [0u8; 512];
        let mut tx_buffer = [0u8; 512];
        let mut socket = TcpSocket::new(self.stack, &mut rx_buffer, &mut tx_buffer);
        let endpoint = IpEndpoint::new(to, TCP_PORT);
        socket.connect(endpoint).await?;

        socket.write_all(&req).await?;
        socket.close();
        log::debug!("sent sync request to {to}");

        // get the sync response.
        let mut recv = heapless::Vec::new();
        let len = read_to_end(&mut socket, &mut recv).await?;
        log::debug!("received sync response: {len} bytes from {to}");
        // this is a janky solution but it doesn't require storing a TcpSocket in an object.
        let tx_id = self.tx_id.fetch_add(1, Ordering::Relaxed);
        self.pending_responses
            .lock()
            .await
            .insert(tx_id, recv.to_vec());

        Ok(tx_id)
    }

    async fn recv_response(&self, tx_id: u32) -> Result<Vec<u8>, NetworkError> {
        loop {
            let v = self.pending_responses.lock().await.remove(&tx_id);
            match v {
                Some(resp) => return Ok(resp),
                None => embassy_time::Timer::after_millis(100).await,
            }
        }
    }

    async fn accept(&self) -> Result<(Self::TxId, Vec<u8>), NetworkError> {
        let mut rx_buffer = [0u8; 512];
        let mut tx_buffer = [0u8; 512];
        let mut socket = TcpSocket::new(self.stack, &mut rx_buffer, &mut tx_buffer);
        let endpoint = IpListenEndpoint {
            addr: None,
            port: TCP_PORT,
        };
        socket.accept(endpoint).await?;
        let mut recv = heapless::Vec::new();
        read_to_end(&mut socket, &mut recv).await?;
        // TODO(chip): use a real tx_id
        let tx_id = 0;

        // TODO(chip): we need to respond to this socket somehow but we can't move the buffers
        Ok((tx_id, recv.to_vec()))
    }

    async fn send_response(&self, tx_id: Self::TxId, resp: Vec<u8>) -> Result<(), NetworkError> {
        todo!()
    }
}

async fn read_to_end(
    socket: &mut TcpSocket<'_>,
    out: &mut heapless::Vec<u8, 1024>,
) -> Result<usize, NetworkError> {
    let mut buf = [0u8; 64];

    loop {
        match socket.read(&mut buf).await {
            Ok(n) => out
                .extend_from_slice(&buf[..n])
                .map_err(|_| NetworkError::MessageTooLarge)?,
            Err(e) => match e {
                embassy_net::tcp::Error::ConnectionReset => break,
            },
        }
    }

    Ok(out.len())
}

/* #[embassy_executor::task]
pub async fn wifi_sync_task(stack: Stack<'static>, graph_id: GraphId, daemon: Arc<Daemon>) {
    log::info!("Waiting for connection...");

    let _buf = [0u8; 1024];
    let mut rx_buffer = [0; 1024];
    let mut tx_buffer = [0; 1024];
    loop {
        if let Some(config) = stack.config_v4() {
            log::info!("IP address: {:?}", config);
        } else {
            log::info!("No configuration detected, returning control back to scheduler to manage wifi connection");
            // Wait a bit for the networking tasks to initialize
            embassy_time::Timer::after(Duration::from_millis(1000)).await;
            continue;
        }
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
                log::info!("Client connected");

                let mut sync_handler = TcpSyncHandler::new(socket, graph_id, None);
                sync_handler
                    .handle_connection()
                    .await
                    .expect("Failed to handle connection");
            }
            Err(e) => {
                // todo properly encase in a loop so that repeated attempts can be made to set up and use TCP sockets
                log::error!("Accept error: {:?}", e);
                // Close the current socket connection
                socket.abort();
                drop(socket);
                embassy_time::Timer::after(Duration::from_millis(100)).await;
            }
        }
    }
}
 */
