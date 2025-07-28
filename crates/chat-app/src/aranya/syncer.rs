use core::task::Poll;

use alloc::collections::btree_map;
use alloc::string::String;
use alloc::vec;
use alloc::{boxed::Box, collections::btree_map::BTreeMap};
use aranya_crypto::Rng;
use aranya_runtime::{
    linear::LinearSegment, Address, GraphId, Location, PeerCache, Segment, Storage,
    StorageProvider, SyncError, SyncRequestMessage, SyncRequester, SyncResponder, SyncType,
    Transaction, MAX_SYNC_MESSAGE_SIZE,
};
use aranya_runtime::{ClientError, Command};
use embassy_futures::poll_once;
use embassy_futures::select::{select, Either};
use embassy_time::{with_timeout, Duration, Instant, Timer};
use parameter_store::MAX_PEERS;

use crate::aranya::daemon::Client;
use crate::aranya::sink::{DebugSink, PubSubSink};
use crate::{
    aranya::daemon::{PE, SP},
    aranya::error::Result,
    net::{Message, NetworkInterface},
};

const SYNC_STALL_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum SyncMessageType {
    Request,
    Response,
    Hello,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct HelloMessage<N>
where
    N: NetworkInterface,
{
    address: N::Addr,
    head: Address,
    peer_count: u16,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct SyncMessage {
    t: SyncMessageType,
    bytes: Box<[u8]>,
}

impl SyncMessage {
    pub fn new(t: SyncMessageType, bytes: Box<[u8]>) -> SyncMessage {
        SyncMessage { t, bytes }
    }

    pub fn into_message<A>(self, from: A, to: A) -> Result<Message<A>>
    where
        A: Default + Ord,
    {
        let ib = postcard::to_allocvec(&self)?;
        Ok(Message::new(from, to, ib.into_boxed_slice()))
    }

    pub fn from_message<A>(m: Message<A>) -> Result<(A, SyncMessage)>
    where
        A: Default + Ord,
    {
        let sm = postcard::from_bytes(&m.contents)?;
        Ok((m.sender, sm))
    }
}

/// A response to a sync request.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum SyncResponse {
    /// Success.
    Ok(Box<[u8]>),
    /// Failure.
    Err(String),
}

/// Container for a SyncRequester and its starting timestamp
struct SyncSession<'a, A> {
    requester: SyncRequester<'a, A>,
    trx: Option<Transaction<SP, PE>>,
    last_seen: Instant,
}

/// Aranya client.
pub struct SyncEngine<'a, N>
where
    N: NetworkInterface,
{
    graph_id: GraphId,
    network: N,
    syncable_peers: heapless::FnvIndexSet<N::Addr, MAX_PEERS>,
    sessions: BTreeMap<N::Addr, SyncSession<'a, N::Addr>>,
    peer_caches: BTreeMap<N::Addr, PeerCache>,
    sink: PubSubSink<'a>,
    hello_boost: u8,
    last_hello: Instant,
}

impl<N> SyncEngine<'_, N>
where
    N: NetworkInterface,
{
    /// Creates a new [`Client`].
    pub fn new(graph_id: GraphId, network: N) -> Self {
        SyncEngine {
            graph_id,
            network,
            syncable_peers: heapless::FnvIndexSet::new(),
            sessions: BTreeMap::new(),
            peer_caches: BTreeMap::new(),
            sink: PubSubSink::new(),
            hello_boost: 0,
            last_hello: Instant::from_ticks(0),
        }
    }
}

impl<N> SyncEngine<'_, N>
where
    N: NetworkInterface,
    N::Addr: Default + Ord + serde::Serialize + for<'b> serde::Deserialize<'b>,
{
    /// Syncs with the peer.
    /// Aranya client sends a `SyncRequest` to peer. The `SyncResponse` is handled below in
    /// [`handle_message()`](Self::handle_message).
    async fn sync_peer(&mut self, peer_addr: N::Addr, client: &mut Client) -> Result<()> {
        let server_addr = self.network.my_address();
        let mut send_buf = vec![0u8; MAX_SYNC_MESSAGE_SIZE];

        let (len, _) = {
            let requester = match self.sessions.entry(peer_addr) {
                btree_map::Entry::Vacant(entry) => {
                    &mut entry
                        .insert(SyncSession {
                            requester: SyncRequester::new(self.graph_id, &mut Rng, server_addr),
                            trx: None,
                            last_seen: Instant::now(),
                        })
                        .requester
                }
                btree_map::Entry::Occupied(entry) => {
                    let last_seen = entry.get().last_seen;
                    if Instant::now() - last_seen > SYNC_STALL_TIMEOUT {
                        log::info!("sync_peer: sync stalled for {peer_addr}");
                        // sync is stalled. Commit any progress so far and remove this entry
                        let mut ses = entry.remove();
                        if let Some(trx) = &mut ses.trx {
                            client.commit(trx, &mut self.sink)?;
                        }
                        self.syncable_peers.remove(&peer_addr);
                    }
                    // Otherwise, we wait for this sync to proceed
                    return Ok(());
                }
            };
            let peer_cache = self.peer_caches.entry(peer_addr).or_default();
            log::info!("peer_cache for {peer_addr}: {peer_cache:?}");
            requester.poll(&mut send_buf, client.provider(), peer_cache)?
        };
        log::info!("sync_peer: sending Request len {len} to {peer_addr}");
        send_buf.truncate(len);
        let sm = SyncMessage::new(SyncMessageType::Request, send_buf.into());
        let m = sm.into_message(self.network.my_address(), peer_addr)?;
        self.network.send_message(m).await?;
        Ok(())
    }

    /// Execute one iteration of syncer logic, handling incoming messages and sending responses
    pub async fn process(&mut self, client: &mut Client) {
        if Instant::now() - self.last_hello > self.hello_timeout() {
            if let Err(err) = self.send_hello(client).await {
                log::error!("initiate: could not send hello {err}");
            }
        }
        if let Err(err) = self.handle_messages(client).await {
            log::error!("sync handle_message: {err}");
        }
        // we have to make a copy of this list otherwise we're borrowing
        // &self inside the loop where we need to do self.sync_peer()
        let peers: heapless::Vec<N::Addr, MAX_PEERS> =
            self.syncable_peers.iter().cloned().collect();
        for peer in peers {
            if let Err(err) = self.sync_peer(peer, client).await {
                log::error!("Could not initiate sync with {peer}: {err}");
            }
        }
    }

    fn hello_timeout(&mut self) -> Duration {
        Duration::from_millis(1000 >> self.hello_boost)
    }

    pub fn boost_hello(&mut self) {
        self.hello_boost = 3;
        self.last_hello = Instant::from_ticks(0);
    }

    async fn send_hello(&mut self, client: &mut Client) -> Result<()> {
        log::info!("send_hello");
        // BUG: check if it the same as our head before accessing storage.

        let provider = client.provider();
        let storage = provider.get_storage(self.graph_id)?;
        let head = storage.get_head()?;

        let segment = storage.get_segment(head)?;
        let command = segment.get_command(head).expect("location must exist");

        let address = Address {
            id: command.id(),
            //BUG: can this really not fail?
            max_cut: command.max_cut().expect("BUG: Why can it fail?"),
        };

        let hello: HelloMessage<N> = HelloMessage {
            address: self.network.my_address(),
            peer_count: 0,
            head: address,
        };

        let hello_bytes = postcard::to_allocvec(&hello)?;
        let sm = SyncMessage::new(SyncMessageType::Hello, hello_bytes.into());
        let m = sm.into_message(self.network.my_address(), N::BROADCAST)?;
        self.network.send_message(m).await?;

        if self.hello_boost > 0 {
            self.hello_boost -= 1;
        }
        self.last_hello = Instant::now();

        Ok(())
    }

    async fn sync_respond(
        &mut self,
        from: N::Addr,
        request: SyncRequestMessage,
        client: &mut Client,
    ) -> Result<()> {
        let mut responder = SyncResponder::new(from);
        responder.receive(request)?;
        let mut c = 0;
        while responder.ready() {
            let mut msg_buf = vec![0u8; MAX_SYNC_MESSAGE_SIZE];
            let len = {
                let peer_cache = self.peer_caches.entry(from).or_default();
                responder.poll(&mut msg_buf, client.provider(), peer_cache)?
            };
            log::info!(
                "sync_respond: responding to {from} with len {} loop {}",
                len,
                c
            );
            c += 1;
            msg_buf.truncate(len);
            let response_message =
                SyncMessage::new(SyncMessageType::Response, msg_buf.into_boxed_slice());
            let msg = response_message.into_message(self.network.my_address(), from)?;
            self.network.send_message(msg).await?;
        }

        Ok(())
    }

    async fn process_response(
        &mut self,
        from: N::Addr,
        bytes: &[u8],
        client: &mut Client,
    ) -> Result<()> {
        let req_session = self
            .sessions
            .get_mut(&from)
            .ok_or(SyncError::SessionMismatch)?;
        req_session.last_seen = Instant::now();
        let requester = &mut req_session.requester;

        let cmds = requester.receive(bytes)?;
        if let Some(cmds) = cmds {
            if !cmds.is_empty() {
                let peer_cache = self.peer_caches.entry(from).or_default();
                add_commands(
                    &cmds,
                    &mut req_session.trx,
                    peer_cache,
                    &mut self.sink,
                    client,
                    self.graph_id,
                )?;
            }
        } else {
            // We're done, destroy the requester
            log::info!("process_response: sync ended with {from}");
            // SAFETY: we know the session exists because we've been using it
            let mut req_session = self.sessions.remove(&from).unwrap();
            if let Some(trx) = &mut req_session.trx {
                log::info!("process_response: commiting");
                client.commit(trx, &mut self.sink)?;
                log::info!("process_response: done commiting");
            } else {
                log::error!("process_response: No transaction!!")
            }
        }

        Ok(())
    }

    async fn handle_messages(&mut self, client: &mut Client) -> Result<()> {
        // Process any messages waiting in the queue, but do not wait for any more.
        while let Poll::Ready(rmsg) = poll_once(self.network.recv_message()) {
            self.handle_message(rmsg?, client).await?;
        }
        Ok(())
    }

    async fn handle_message(&mut self, msg: Message<N::Addr>, client: &mut Client) -> Result<()> {
        let (from, sm) = SyncMessage::from_message(msg)?;
        log::info!(
            "received SyncMessage {:?} from {from}, len {}",
            sm.t,
            sm.bytes.len()
        );
        match sm.t {
            SyncMessageType::Request => {
                let st: SyncType<<N as NetworkInterface>::Addr> = postcard::from_bytes(&sm.bytes)?;
                match st {
                    SyncType::Poll { request, .. } => {
                        self.sync_respond(from, request, client).await?
                    }
                    _ => unimplemented!(),
                };
            }
            SyncMessageType::Response => {
                self.process_response(from, &sm.bytes, client).await?;
            }
            SyncMessageType::Hello => {
                let hello: HelloMessage<N> = postcard::from_bytes(&sm.bytes)?;

                // BUG: check if it the same as our head before accessing storage.

                let has_address = {
                    let provider = client.provider();
                    let storage = provider.get_storage(self.graph_id)?;
                    storage.get_location(hello.head)?.is_some()
                };

                if has_address {
                    self.syncable_peers.remove(&hello.address);
                } else {
                    if self.syncable_peers.len() == MAX_PEERS {
                        // remove the first peer
                        // SAFETY: the peer definitely exists because the map is full
                        let peer = self.syncable_peers.first().unwrap().clone();
                        self.syncable_peers.remove(&peer);
                    }
                    // SAFETY: we have guaranteed there is space above
                    self.syncable_peers.insert(hello.address).ok();
                }
            }
        }
        Ok(())
    }
}

fn add_commands(
    cmds: &[impl Command + core::fmt::Debug],
    trx: &mut Option<Transaction<SP, PE>>,
    peer_cache: &mut PeerCache,
    sink: &mut PubSubSink<'_>,
    client: &mut Client,
    graph_id: GraphId,
) -> Result<()> {
    let trx = trx.get_or_insert_with(|| client.transaction(graph_id));
    dump_commands(cmds);
    client.add_commands(trx, sink, cmds)?;

    // Update peer cache
    let addresses = cmds.iter().filter_map(|cmd| cmd.address().ok());
    let storage = client
        .provider()
        .get_storage(graph_id)
        .map_err(|e| ClientError::StorageError(e))?;
    for addr in addresses {
        if let Some(cmd_loc) = storage
            .get_location(addr)
            .map_err(|e| ClientError::StorageError(e))?
        {
            peer_cache
                .add_command(storage, addr, cmd_loc)
                .map_err(|e| ClientError::StorageError(e))?;
        }
    }

    Ok(())
}

fn dump_commands(cmds: &[impl Command]) {
    for c in cmds {
        log::info!(
            "  priority {:?} {} MAX_CUT {}",
            c.priority(),
            c.id(),
            c.max_cut().unwrap()
        );
    }
}
