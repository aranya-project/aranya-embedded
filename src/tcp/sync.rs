use alloc::{boxed::Box, vec::Vec};
use aranya_crypto::{Csprng, Rng};
use aranya_runtime::{
    CommandMeta, GraphId, PeerCache, StorageProvider, SyncError, SyncRequester, VmEffect,
};
use core::{fmt, marker::PhantomData};
use embassy_net::tcp::TcpSocket;
use embassy_time::{Duration, Timer};
use embedded_io_async::{Read, Write};
use esp_println::println;
use postcard::{from_bytes, to_slice};
use serde::{Deserialize, Serialize};

use crate::{aranya::sink::VecSink, Client};

use super::format::{read_prefix, write_prefix, Command, Subject};

#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
    SyncRequest {
        session_id: u128,
        storage_id: u64,
        max_bytes: u64,
        commands: Vec<Vec<u8>>,
    },
    SyncResponse {
        session_id: u128,
        index: u64,
        commands: Vec<CommandMeta>,
    },
    SyncEnd {
        session_id: u128,
        max_index: u64,
    },
    EndSession {
        session_id: u128,
    },
}

const MAX_MESSAGE_SIZE: usize = 1024; // Match the TCP buffer size
const PREFIX_LEN: usize = 3; // 2 bytes are used for configuring prefix

pub struct TcpSyncHandler<'a> {
    socket: TcpSocket<'a>,
    sync_requester: Option<SyncRequester<'a>>,
    buffer: [u8; MAX_MESSAGE_SIZE],
    client: Client,
    peer_cache: PeerCache,
    effect_sink: VecSink<VmEffect>,
}

impl<'a> TcpSyncHandler<'a> {
    pub fn new(
        socket: TcpSocket<'a>,
        storage_id: Option<GraphId>,
        client: Client,
        peer_cache: PeerCache,
        effect_sink: VecSink<VmEffect>,
    ) -> Self {
        Self {
            socket,
            sync_requester: storage_id.map(|storage_id| SyncRequester::new(storage_id, &mut Rng)),
            buffer: [0u8; MAX_MESSAGE_SIZE],
            client,
            peer_cache,
            effect_sink,
        }
    }

    pub async fn handle_connection(&mut self) -> Result<(), SyncError> {
        loop {
            if let Some(sync_requester) = &mut self.sync_requester {
                println!("Sync Requester Exists");
                loop {
                    // Check if requester has a message to send
                    if sync_requester.ready() {
                        // Poll the requester for a message
                        match sync_requester.poll(
                            &mut self.buffer,
                            self.client.provider(),
                            &mut self.peer_cache,
                        ) {
                            Ok((written, _)) => {
                                // Send prefix
                                let mut prefix_buf: [u8; 3] = [0; PREFIX_LEN];
                                write_prefix(
                                    &mut prefix_buf,
                                    written as u16,
                                    Command::Set(Subject::Sync),
                                );
                                self.socket
                                    .write_all(&prefix_buf)
                                    .await
                                    .map_err(|_| SyncError::NotReady)?;

                                // Send message
                                self.socket
                                    .write_all(&self.buffer[..written])
                                    .await
                                    .map_err(|_| SyncError::NotReady)?;
                            }
                            Err(e) => {
                                println!("Error polling requester: {:?}", e);
                                return Err(e);
                            }
                        }
                    }

                    // Read response
                    // First read length prefix
                    let mut len_bytes: [u8; 3] = [0u8; PREFIX_LEN];
                    // ! Potentially add a timeout which clears the buffer in case
                    match self.socket.read_exact(&mut len_bytes).await {
                        Ok(_) => {}
                        Err(_) => return Err(SyncError::NotReady),
                    }

                    // todo: Overall handle errors like this better
                    let (prefix_commands, length) =
                        read_prefix(&len_bytes).expect("Failed to unwrap message prefix");
                    let message_len = length as usize;
                    if message_len as usize > MAX_MESSAGE_SIZE {
                        return Err(SyncError::CommandOverflow);
                    }

                    // Read message
                    match self
                        .socket
                        .read_exact(&mut self.buffer[..message_len])
                        .await
                    {
                        Ok(_) => {}
                        Err(_) => return Err(SyncError::NotReady),
                    }

                    // Process received message
                    match sync_requester.receive(&self.buffer[..message_len]) {
                        Ok(Some(commands)) => {
                            println!("Received Commands: {:?}", commands);
                            if !commands.is_empty() {
                                let mut trx = self.client.transaction(GraphId::default());
                                self.client
                                    .add_commands(
                                        &mut trx,
                                        &mut self.effect_sink,
                                        &commands,
                                        &mut self.peer_cache,
                                    )
                                    .expect("Unable to add recieved commands");
                                self.client
                                    .commit(&mut trx, &mut self.effect_sink)
                                    .expect("Commit Failed");
                                println!("Committed to Graph");
                            }
                        }
                        Ok(None) => {}
                        Err(e) => {
                            println!("Error processing message: {:?}", e);
                            if matches!(e, SyncError::SessionMismatch) {
                                break;
                            }
                        }
                    }
                }
            } else {
                println!("Asking for GraphId");
                {
                    let mut prefix_buf: [u8; 3] = [0; PREFIX_LEN];
                    write_prefix(&mut prefix_buf, 0, Command::Get(Subject::GraphId));
                    self.socket
                        .write_all(&prefix_buf)
                        .await
                        .map_err(|_| SyncError::NotReady)?;

                    // Send message
                    self.socket
                        .write_all(&self.buffer[..0])
                        .await
                        .map_err(|_| SyncError::NotReady)?;
                }

                // try to read GraphId
                {
                    // Read response
                    // First read length prefix
                    let mut len_bytes: [u8; 3] = [0u8; PREFIX_LEN];
                    // ! Potentially add a timeout which clears the buffer in case
                    match self.socket.read_exact(&mut len_bytes).await {
                        Ok(_) => {}
                        Err(_) => return Err(SyncError::NotReady),
                    }

                    // todo: Overall handle errors like this better
                    let (prefix_commands, length) =
                        read_prefix(&len_bytes).expect("Failed to unwrap message prefix");
                    let message_len = length as usize;
                    if message_len > MAX_MESSAGE_SIZE {
                        return Err(SyncError::CommandOverflow);
                    }

                    // Read message
                    match self
                        .socket
                        .read_exact(&mut self.buffer[..message_len])
                        .await
                    {
                        Ok(_) => {}
                        Err(_) => return Err(SyncError::NotReady),
                    }

                    match prefix_commands {
                        // todo Shouldn't be getting at this point
                        Command::Set(Subject::GraphId) => {
                            let graph_id: GraphId =
                                from_bytes::<GraphId>(&self.buffer[..message_len])?;
                            self.sync_requester = Some(SyncRequester::new(graph_id, &mut Rng));
                        }
                        _ => {
                            Timer::after(Duration::from_millis(250)).await;
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
