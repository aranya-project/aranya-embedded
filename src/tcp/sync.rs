use alloc::{format, rc::Rc, vec::Vec};
use aranya_crypto::{default::DefaultEngine, Rng};
use aranya_policy_vm::Value;
use aranya_runtime::{
    linear::LinearStorageProvider, ClientState, GraphId, PeerCache, SyncError, SyncRequester,
    VmEffect,
};
use embassy_net::tcp::TcpSocket;
use embassy_time::Duration;
use embedded_io::{ReadReady, WriteReady};
use embedded_io_async::Write;
use esp_println::println;
use owo_colors::OwoColorize;
use postcard::{from_bytes, to_slice};
use serde::{Deserialize, Serialize};

use crate::{
    aranya::{graph_store::GraphManager, sink::VecSink},
    hardware::esp32_engine::ESP32Engine,
    VolumeMan,
};

use super::format::Commands;

const MAX_MESSAGE_SIZE: usize = 2048; // Match the TCP buffer size
const MAX_RETRY_TIME_MS: u64 = 1000; // Maximum retry time of 1 second
const RETRY_DELAY_MS: u64 = 100; // Delay between retries (0.1s)

#[derive(Deserialize, Serialize, Clone)]
pub struct ServerStub;

pub struct TcpSyncHandler<'a> {
    socket: TcpSocket<'a>,
    sync_requester: Option<SyncRequester<'a, ServerStub>>,
    graph_id: Option<GraphId>,
    buffer: [u8; MAX_MESSAGE_SIZE],
    client: Option<ClientState<ESP32Engine<DefaultEngine>, LinearStorageProvider<GraphManager>>>,
    peer_cache: PeerCache,
    effect_sink: VecSink<VmEffect>,
    volume_manager: Rc<VolumeMan>,
}

impl<'a> TcpSyncHandler<'a> {
    pub fn new(
        socket: TcpSocket<'a>,
        storage_id: Option<GraphId>,
        client: Option<
            ClientState<ESP32Engine<DefaultEngine>, LinearStorageProvider<GraphManager>>,
        >,
        volume_manager: Rc<VolumeMan>,
    ) -> Self {
        Self {
            socket,
            graph_id: storage_id,
            sync_requester: storage_id
                .map(|storage_id| SyncRequester::new(storage_id, &mut Rng, ServerStub)),
            buffer: [0u8; MAX_MESSAGE_SIZE],
            client,
            peer_cache: PeerCache::new(),
            effect_sink: VecSink::new(),
            volume_manager,
        }
    }

    async fn send_message(&mut self, command: Commands) -> Result<(), SyncError> {
        println!("Send Message over TCP");
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
                            embassy_time::Timer::after(Duration::from_millis(RETRY_DELAY_MS)).await;
                        }
                    }
                }
                Err(_) => return Err(SyncError::NotReady),
            }
        }
    }

    pub async fn handle_connection(&mut self) -> Result<(), SyncError> {
        // Define a constant for timeout duration
        const COMMAND_TIMEOUT: Duration = Duration::from_millis(1000);

        // Track if we're waiting for a response
        let mut waiting_for_response = false;
        let mut last_command_sent = None;

        loop {
            if self.sync_requester.is_none() {
                if !waiting_for_response {
                    println!("Asking for GraphId");
                    let command = Commands::GetGraphID;
                    self.send_message(command).await?;
                    waiting_for_response = true;
                    last_command_sent = Some("GetGraphID");
                }

                // Wait for response or timeout
                match embassy_time::with_timeout(COMMAND_TIMEOUT, self.receive_message()).await {
                    Ok(Ok(command)) => {
                        match command {
                            Commands::SendGraphID(graph_id) => {
                                println!("Received SendGraphID");
                                self.graph_id = Some(graph_id);
                                self.sync_requester =
                                    Some(SyncRequester::new(graph_id, &mut Rng, ServerStub));

                                let policy = ESP32Engine::<DefaultEngine>::new();
                                self.client = Some(ClientState::new(
                                    policy,
                                    LinearStorageProvider::new(
                                        GraphManager::new(self.volume_manager.clone()).unwrap(),
                                    ),
                                ));
                                println!("{}", "Client State has been created".green());
                                waiting_for_response = false;
                            }
                            _ => {
                                println!("Received unexpected command while waiting for GraphID");
                                // Reset waiting state and retry after delay
                                waiting_for_response = false;
                                embassy_time::Timer::after_millis(500).await;
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        println!("Error receiving message: {:?}", e);
                        waiting_for_response = false;
                        embassy_time::Timer::after_millis(500).await;
                    }
                    Err(_) => {
                        // Timeout occurred
                        println!("Timeout waiting for GraphID response");
                        waiting_for_response = false;
                        embassy_time::Timer::after_millis(500).await;
                    }
                }
                continue;
            }

            // At this point we have a sync_requester
            if !waiting_for_response {
                // Only send a new request if we're not waiting for a response
                if let (Some(requester), Some(client)) =
                    (self.sync_requester.as_mut(), self.client.as_mut())
                {
                    if requester.ready() {
                        match requester.poll(
                            &mut self.buffer,
                            client.provider(),
                            &mut self.peer_cache,
                        ) {
                            Ok((written, _)) => {
                                if written > 0 {
                                    // Only send if there's actual data
                                    let sync_data = Vec::from(&self.buffer[..written]);
                                    let command = Commands::SendSyncRequest(sync_data);
                                    self.send_message(command).await?;
                                    waiting_for_response = true;
                                    last_command_sent = Some("SendSyncRequest");
                                } else {
                                    // No data to send, wait a bit before polling again
                                    embassy_time::Timer::after_millis(250).await;
                                }
                            }
                            Err(e) => {
                                println!("Error polling requester: {:?}", e);
                                return Err(e);
                            }
                        }
                    } else {
                        // If not ready to poll, send a GetSyncRequest
                        let command = Commands::GetSyncRequest;
                        self.send_message(command).await?;
                        waiting_for_response = true;
                        last_command_sent = Some("GetSyncRequest");
                    }
                }
            } else {
                // We're waiting for a response, let's wait with a timeout
                match embassy_time::with_timeout(COMMAND_TIMEOUT, self.receive_message()).await {
                    Ok(Ok(command)) => {
                        println!("Received Message from TCP");
                        match command {
                            Commands::GetGraphID => {
                                println!("Received GetGraphID");
                                self.send_message(Commands::SendGraphID(self.graph_id.unwrap()))
                                    .await?;
                                // We've responded, so we're not waiting anymore
                                waiting_for_response = false;
                            }
                            Commands::SendGraphID(graph_id) => {
                                println!("Received SendGraphID");
                                // Just acknowledge this, we already have a graph ID
                                waiting_for_response = false;
                            }
                            Commands::GetSyncRequest => {
                                println!("Received GetSyncRequest");
                                if let Some(requester) = self.sync_requester.as_mut() {
                                    if requester.ready() {
                                        match requester.poll(
                                            &mut self.buffer,
                                            self.client.as_mut().unwrap().provider(),
                                            &mut self.peer_cache,
                                        ) {
                                            Ok((written, _)) => {
                                                let sync_data = Vec::from(&self.buffer[..written]);
                                                let command = Commands::SendSyncRequest(sync_data);
                                                self.send_message(command).await?;
                                                // Still waiting for a response to this new command
                                                last_command_sent = Some("SendSyncRequest");
                                            }
                                            Err(e) => {
                                                println!("Error polling requester: {:?}", e);
                                                waiting_for_response = false;
                                                return Err(e);
                                            }
                                        }
                                    } else {
                                        // Not ready, just acknowledge
                                        waiting_for_response = false;
                                    }
                                }
                            }
                            Commands::SendSyncRequest(sync_data) => {
                                println!("Received SendSyncRequest");
                                if let Some(requester) = self.sync_requester.as_mut() {
                                    match requester.receive(&sync_data)? {
                                        Some(sync_commands) => {
                                            println!("Received Commands: {:?}", sync_commands);
                                            if !sync_commands.is_empty() {
                                                let client = self.client.as_mut().unwrap();
                                                let mut trx =
                                                    client.transaction(self.graph_id.unwrap());
                                                client
                                                    .add_commands(
                                                        &mut trx,
                                                        &mut self.effect_sink,
                                                        &sync_commands,
                                                        &mut self.peer_cache,
                                                    )
                                                    .expect("Unable to add received commands");

                                                client
                                                    .commit(&mut trx, &mut self.effect_sink)
                                                    .expect("commit failed");
                                                println!("committed");
                                                println!("{:?}", self.effect_sink);

                                                // Process any effects
                                                if let Some(last_effect) =
                                                    remove_first(&mut self.effect_sink.effects)
                                                {
                                                    Self::process_effect(last_effect);
                                                } else {
                                                    println!("{}", "No Effect Present".yellow());
                                                }
                                            }
                                        }
                                        None => {
                                            println!("No Data");
                                        }
                                    }
                                }
                                waiting_for_response = false;
                            }
                            Commands::DeserializeError => {
                                println!("Received DeserializeError");
                                waiting_for_response = false;
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        println!("Error receiving message: {:?}", e);
                        waiting_for_response = false;
                        // Brief pause before retrying
                        embassy_time::Timer::after_millis(500).await;
                    }
                    Err(_) => {
                        // Timeout occurred
                        println!("Timeout waiting for response to {:?}", last_command_sent);
                        waiting_for_response = false;
                        // Brief pause before retrying
                        embassy_time::Timer::after_millis(500).await;
                    }
                }
            }

            // Small delay between loop iterations to prevent CPU hogging
            embassy_time::Timer::after_millis(50).await;
        }
    }

    // Helper function to process effects
    fn process_effect(effect: VmEffect) {
        if effect.name == "LEDBool" {
            if let Some(kv_pair) = effect.fields.iter().find(|kv| kv.key() == "on") {
                println!("{}", "Action Call based on bool".green());
                match kv_pair.value() {
                    Value::Bool(state) => {
                        // Commented out the LED control code as in original
                        /*match unsafe { &mut *LED.get() } {
                            Some(led) => led_control(*state, led),
                            None => println!("LED peripheral not initialized"),
                        };*/
                    }
                    _ => {
                        println!(
                            "{}",
                            format!("Unexpected Value type for LED State: {:?}", kv_pair.value())
                                .yellow()
                        );
                    }
                }
            } else {
                println!("{}", "'on' Field not Found in LEDBool Effect".yellow());
            }
        } else {
            println!(
                "{}",
                format!("Unexpected Effect: {:?}", effect.name).yellow()
            );
        }
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
