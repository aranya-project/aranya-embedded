use alloc::{boxed::Box, format, vec::Vec};
use aranya_crypto::{default::DefaultEngine, Csprng, Rng};
use aranya_policy_vm::Value;
use aranya_runtime::{
    linear::LinearStorageProvider, ClientState, CommandMeta, GraphId, PeerCache, StorageProvider,
    SyncError, SyncRequester, VmEffect,
};
use core::{cell::UnsafeCell, fmt, marker::PhantomData};
use embassy_net::tcp::TcpSocket;
use embassy_time::{Duration, Timer};
use embedded_hal::{delay::DelayNs, spi::SpiDevice};
use embedded_hal_bus::spi::ExclusiveDevice;
use embedded_io::{ReadReady, WriteReady};
use embedded_io_async::{Read, Write};
use embedded_sdmmc::{
    Directory, RawDirectory, RawVolume, SdCard, TimeSource, VolumeIdx, VolumeManager,
};
use esp_hal::{
    delay::Delay, gpio::Output, peripheral::Peripheral, peripherals::TIMG1, spi::master::Spi,
    timer::timg::TimerX,
};
use esp_println::println;
use owo_colors::OwoColorize;
use postcard::{from_bytes, to_slice};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{
    aranya::{graph_store::GraphManager, sink::VecSink},
    hardware::{esp32_engine::ESP32Engine, esp32_time::Esp32TimeSource},
};

use super::format::Commands;

//static mut LED: UnsafeCell<Option<Output<'static>>> = UnsafeCell::new(None);

const MAX_MESSAGE_SIZE: usize = 1024; // Match the TCP buffer size
const MAX_RETRY_TIME_MS: u64 = 1000; // Maximum retry time of 1 second
const RETRY_DELAY_MS: u64 = 100; // Delay between retries (0.1s)

#[derive(Deserialize, Serialize, Clone)]
pub struct ServerStub;

pub struct TcpSyncHandler<'a> {
    socket: TcpSocket<'a>,
    sync_requester: Option<SyncRequester<'a, ServerStub>>,
    graph_id: Option<GraphId>,
    buffer: [u8; MAX_MESSAGE_SIZE],
    client: Option<
        ClientState<
            ESP32Engine<DefaultEngine>,
            LinearStorageProvider<
                GraphManager<
                    'a,
                    ExclusiveDevice<Spi<'static, esp_hal::Blocking>, Output<'static>, Delay>,
                    Delay,
                    Esp32TimeSource<TimerX<<TIMG1 as Peripheral>::P, 1>>,
                >,
            >,
        >,
    >,
    peer_cache: PeerCache,
    effect_sink: VecSink<VmEffect>,
    directory: Directory<
        'a,
        SdCard<ExclusiveDevice<Spi<'static, esp_hal::Blocking>, Output<'static>, Delay>, Delay>,
        Esp32TimeSource<TimerX<<TIMG1 as Peripheral>::P, 1>>,
        4,
        4,
        1,
    >,
}

impl<'a> TcpSyncHandler<'a> {
    pub fn new(
        socket: TcpSocket<'a>,
        storage_id: Option<GraphId>,
        client: Option<
            ClientState<
                ESP32Engine<DefaultEngine>,
                LinearStorageProvider<
                    GraphManager<
                        'a,
                        ExclusiveDevice<Spi<'static, esp_hal::Blocking>, Output<'static>, Delay>,
                        Delay,
                        Esp32TimeSource<TimerX<<TIMG1 as Peripheral>::P, 1>>,
                    >,
                >,
            >,
        >,
        volume_manager: &'a mut VolumeManager<
            SdCard<ExclusiveDevice<Spi<'static, esp_hal::Blocking>, Output<'static>, Delay>, Delay>,
            Esp32TimeSource<TimerX<<TIMG1 as Peripheral>::P, 1>>,
        >,
    ) -> Self {
        let raw_volume: RawVolume = volume_manager
            .open_raw_volume(VolumeIdx(0))
            .expect("Failed to get volume");

        let raw_root_directory: RawDirectory = volume_manager
            .open_root_dir(raw_volume)
            .expect("Failed to open root directory");

        let directory: Directory<
            'a,
            SdCard<ExclusiveDevice<Spi<'static, esp_hal::Blocking>, Output<'static>, Delay>, Delay>,
            Esp32TimeSource<TimerX<<TIMG1 as Peripheral>::P, 1>>,
            4,
            4,
            1,
        > = raw_root_directory.to_directory(volume_manager);
        Self {
            socket,
            graph_id: storage_id,
            sync_requester: storage_id
                .map(|storage_id| SyncRequester::new(storage_id, &mut Rng, ServerStub)),
            buffer: [0u8; MAX_MESSAGE_SIZE],
            client,
            peer_cache: PeerCache::new(),
            effect_sink: VecSink::new(),
            directory,
        }
    }

    async fn send_message(&mut self, command: Commands) -> Result<(), SyncError> {
        // Check if socket is ready to write
        if !self.socket.write_ready().map_err(|_| SyncError::NotReady)? {
            return Err(SyncError::NotReady);
        }

        // Serialize the command using postcard
        let serialized =
            to_slice(&command, &mut self.buffer).map_err(|e| SyncError::Serialize(e))?;

        // Send the serialized data
        self.socket
            .write_all(serialized)
            .await
            .map_err(|_| SyncError::NotReady)
    }

    // todo: Make a recoverable error
    async fn receive_message(&mut self) -> Result<Commands, SyncError> {
        // Check if socket is ready to read
        if !self.socket.read_ready().map_err(|_| SyncError::NotReady)? {
            return Err(SyncError::NotReady);
        }

        let start_time = embassy_time::Instant::now();
        let timeout = Duration::from_millis(MAX_RETRY_TIME_MS);

        let mut temp_buffer = [0u8; MAX_MESSAGE_SIZE];
        let mut read_position = 0usize;

        loop {
            // Check timeout
            if start_time.elapsed() > timeout {
                self.send_message(Commands::DeserializeError).await?;
                return Err(SyncError::Serialize(
                    postcard::Error::DeserializeUnexpectedEnd,
                ));
            }

            // Try to read data into our temporary buffer
            match self.socket.read(&mut temp_buffer[read_position..]).await {
                Ok(n) => {
                    read_position += n;
                    if read_position >= MAX_MESSAGE_SIZE {
                        self.send_message(Commands::DeserializeError).await?;
                        return Err(SyncError::Serialize(
                            postcard::Error::DeserializeUnexpectedEnd,
                        ));
                    }
                    // Try to deserialize all that has been written to the temporary buffer
                    match from_bytes::<Commands>(&temp_buffer[..read_position]) {
                        Ok(command) => return Ok(command),
                        Err(_) => {
                            Timer::after(Duration::from_millis(RETRY_DELAY_MS)).await;
                        }
                    }
                }
                Err(_) => return Err(SyncError::NotReady),
            }
        }
    }

    pub async fn handle_connection(&mut self) -> Result<(), SyncError> {
        loop {
            if self.sync_requester.is_some() {
                println!("Sync Requester Exists");
                loop {
                    // Check if requester has a message to send
                    if self.sync_requester.as_ref().unwrap().ready() {
                        // Poll the requester for a message
                        match self.sync_requester.as_mut().unwrap().poll(
                            &mut self.buffer,
                            self.client.as_mut().unwrap().provider(),
                            &mut self.peer_cache,
                        ) {
                            Ok((written, _)) => {
                                let sync_data = Vec::from(&self.buffer[..written]);
                                let command = Commands::SendSyncRequest(sync_data);
                                self.send_message(command).await?;
                            }
                            Err(e) => {
                                println!("Error polling requester: {:?}", e);
                                return Err(e);
                            }
                        }
                    }

                    // Read message
                    match self.receive_message().await {
                        Ok(command) => match command {
                            // todo get rif of unwraps
                            Commands::GetGraphID => {
                                self.send_message(Commands::SendGraphID(self.graph_id.unwrap()))
                                    .await?
                            }
                            Commands::SendGraphID(graph_id) => {}
                            Commands::GetSyncRequest => {
                                if self.sync_requester.as_ref().unwrap().ready() {
                                    // Poll the requester for a message
                                    match self.sync_requester.as_mut().unwrap().poll(
                                        &mut self.buffer,
                                        self.client.as_mut().unwrap().provider(),
                                        &mut self.peer_cache,
                                    ) {
                                        Ok((written, _)) => {
                                            let sync_data = Vec::from(&self.buffer[..written]);
                                            let command = Commands::SendSyncRequest(sync_data);
                                            self.send_message(command).await?;
                                        }
                                        Err(e) => {
                                            println!("Error polling requester: {:?}", e);
                                            return Err(e);
                                        }
                                    }
                                }
                            }
                            Commands::SendSyncRequest(sync_data) => {
                                match self.sync_requester.as_mut().unwrap().receive(&sync_data)? {
                                    Some(sync_commands) => {
                                        println!("Recieved Commands: {:?}", sync_commands);
                                        if !sync_commands.is_empty() {
                                            let mut trx = self
                                                .client
                                                .unwrap()
                                                .transaction(self.graph_id.unwrap());
                                            self.client
                                                .unwrap()
                                                .add_commands(
                                                    &mut trx,
                                                    &mut self.effect_sink,
                                                    &sync_commands,
                                                    &mut self.peer_cache,
                                                )
                                                .expect("Unable to add recieved commands");
                                            self.client
                                                .unwrap()
                                                .commit(&mut trx, &mut self.effect_sink)
                                                .expect("commit failed");
                                            println!("committed");
                                            println!("{:?}", self.effect_sink)
                                        }
                                        if let Some(last_effect) =
                                            remove_first(&mut self.effect_sink.effects)
                                        {
                                            if last_effect.name == "LEDBool" {
                                                if let Some(kv_pair) = last_effect
                                                    .fields
                                                    .iter()
                                                    .find(|kv| kv.key() == "on")
                                                {
                                                    println!(
                                                        "{}",
                                                        "Action Call based on bool".green()
                                                    );
                                                    match kv_pair.value() {
                                                        Value::Bool(state) => {
                                                            /*match unsafe { &mut *LED.get() } {
                                                                Some(led) => led_control(*state, led),
                                                                None => println!("LED peripheral not initialized"),
                                                            };*/
                                                        }
                                                        _ => {
                                                            println!(
                                                                "{}",
                                                                format!(
                                                                    "Unexpected Value type for LED State: {:?}",
                                                                    kv_pair.value()
                                                                )
                                                                .yellow()
                                                            );
                                                        }
                                                    }
                                                } else {
                                                    println!(
                                                        "{}",
                                                        "'on' Field not Found in LEDBool Effect"
                                                            .yellow()
                                                    );
                                                }
                                            } else {
                                                println!(
                                                    "{}",
                                                    format!(
                                                        "Unexpected Effect: {:?}",
                                                        last_effect.name
                                                    )
                                                    .yellow()
                                                );
                                            }
                                        } else {
                                            println!("{}", "No Effect Present".yellow());
                                            Timer::after_secs(1).await;
                                        }
                                    }
                                    None => {
                                        println!("No Data")
                                    }
                                }
                            }
                            Commands::DeserializeError => todo!(),
                        },
                        Err(_) => todo!(),
                    }
                }
            } else {
                println!("Asking for GraphId");
                {
                    let command = Commands::GetGraphID;
                    self.send_message(command).await?;
                }

                // try to read GraphId
                {
                    if let Ok(command) = self.receive_message().await {
                        match command {
                            // todo get rif of unwraps
                            Commands::GetGraphID => {}
                            Commands::SendGraphID(graph_id) => {
                                self.graph_id = Some(graph_id);
                                self.sync_requester =
                                    Some(SyncRequester::new(graph_id, &mut Rng, ServerStub));
                                // todo collect correct graphID

                                let policy = ESP32Engine::<DefaultEngine>::new();
                                self.client = Some(ClientState::new(
                                    policy,
                                    LinearStorageProvider::new(GraphManager::new(
                                        &self.directory,
                                        graph_id,
                                    )),
                                ));
                            }
                            Commands::GetSyncRequest => {}
                            Commands::SendSyncRequest(sync_data) => {}
                            Commands::DeserializeError => todo!(),
                        }
                    }
                }
            }
            // todo check if timer is necessary at all
            Timer::after(Duration::from_millis(250)).await;
        }
        Ok(())
    }
}

fn remove_first<T>(vec: &mut Vec<T>) -> Option<T> {
    if vec.is_empty() {
        return None;
    }
    Some(vec.remove(0))
}

/*fn led_control(led_bool: bool, led: &mut Output) {
    if led_bool {
        led.set_high();
    } else {
        led.set_low();
    }
}*/
