use alloc::collections::btree_map;
use alloc::string::String;
use alloc::vec;
use alloc::{boxed::Box, collections::btree_map::BTreeMap};
use aranya_crypto::Rng;
use aranya_runtime::{
    PeerCache, SyncError, SyncRequestMessage, SyncRequester, SyncResponder, SyncType, Transaction,
    MAX_SYNC_MESSAGE_SIZE,
};
use embassy_time::{Duration, Instant, Timer};
use parameter_store::MAX_PEERS;

use crate::hardware::neopixel::NeopixelSink;
use crate::{
    aranya::daemon::{PE, SP},
    aranya::error::Result,
    net::{Message, NetworkInterface},
    Imp,
};

type Mutex<T> = embassy_sync::mutex::Mutex<embassy_sync::blocking_mutex::raw::NoopRawMutex, T>;

const SYNC_STALL_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum SyncMessageType {
    Request,
    Response,
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
struct SyncEngine<'a, N>
where
    N: NetworkInterface,
{
    /// Thread-safe Aranya client reference.
    imp: Imp<NeopixelSink>,
    network: N,
    peers: heapless::Vec<N::Addr, MAX_PEERS>,
    sessions: Mutex<BTreeMap<N::Addr, SyncSession<'a, N::Addr>>>,
    peer_caches: Mutex<BTreeMap<N::Addr, PeerCache>>,
}

impl<N> SyncEngine<'_, N>
where
    N: NetworkInterface,
{
    /// Creates a new [`Client`].
    pub fn new(
        imp: Imp<NeopixelSink>,
        network: N,
        peers: heapless::Vec<N::Addr, MAX_PEERS>,
    ) -> Self {
        SyncEngine {
            imp,
            network,
            peers,
            sessions: Mutex::new(BTreeMap::new()),
            peer_caches: Mutex::new(BTreeMap::new()),
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
    async fn sync_peer(&self, peer_addr: N::Addr) -> Result<()> {
        let server_addr = self.network.my_address();
        let mut send_buf = vec![0u8; MAX_SYNC_MESSAGE_SIZE];

        let (len, _) = {
            let mut requesters = self.sessions.lock().await;
            let requester = match requesters.entry(peer_addr) {
                btree_map::Entry::Vacant(entry) => {
                    &mut entry
                        .insert(SyncSession {
                            requester: SyncRequester::new(
                                self.imp.graph_id(),
                                &mut Rng,
                                server_addr,
                            ),
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
                        let ses = entry.remove();
                        if let Some(trx) = ses.trx {
                            self.imp.commit(trx).await?;
                        }
                    }
                    // Otherwise, we wait for this sync to proceed
                    return Ok(());
                }
            };
            let mut client = self.imp.get_client().await;
            let mut peer_caches = self.peer_caches.lock().await;
            let peer_cache = peer_caches.entry(peer_addr).or_default();
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

    /// Loop forever, attempting to sync with known peers
    async fn initiate(&self) -> ! {
        loop {
            for p in &self.peers {
                self.sync_peer(*p)
                    .await
                    .inspect_err(|e| log::error!("sync initiation: {e}"))
                    .ok();
            }
            Timer::after_millis(100).await;
        }
    }

    async fn sync_respond(&self, from: N::Addr, request: SyncRequestMessage) -> Result<()> {
        let mut responder = SyncResponder::new(from);
        responder.receive(request)?;
        let mut c = 0;
        while responder.ready() {
            let mut msg_buf = vec![0u8; MAX_SYNC_MESSAGE_SIZE];
            let len = {
                let mut aranya = self.imp.get_client().await;
                let mut peer_caches = self.peer_caches.lock().await;
                let peer_cache = peer_caches.entry(from).or_default();
                responder.poll(&mut msg_buf, aranya.provider(), peer_cache)?
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

    async fn process_response(&self, from: N::Addr, bytes: &[u8]) -> Result<()> {
        let mut sessions = self.sessions.lock().await;
        let req_session = &mut sessions.get_mut(&from).ok_or(SyncError::SessionMismatch)?;
        req_session.last_seen = Instant::now();
        let requester = &mut req_session.requester;

        let cmds = requester.receive(bytes)?;
        if let Some(cmds) = cmds {
            if !cmds.is_empty() {
                let mut peer_caches = self.peer_caches.lock().await;
                let peer_cache = peer_caches.entry(from).or_default();
                self.imp
                    .add_commands(&cmds, &mut req_session.trx, peer_cache)
                    .await?;
            }
        } else {
            // We're done, destroy the requester
            log::info!("sync ended with {from}");
            // SAFETY: we know the session exists because we've been using it
            let req_session = sessions.remove(&from).unwrap();
            if let Some(trx) = req_session.trx {
                self.imp.commit(trx).await?;
            }
        }

        Ok(())
    }

    async fn handle_message(&self) -> Result<()> {
        let msg = self.network.recv_message().await?;
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
                    SyncType::Poll { request, .. } => self.sync_respond(from, request).await?,
                    _ => unimplemented!(),
                };
            }
            SyncMessageType::Response => {
                self.process_response(from, &sm.bytes).await?;
            }
        }
        Ok(())
    }

    /// Wait forever for requests and handle them. This does not return.
    async fn serve(&self) -> ! {
        loop {
            match self.handle_message().await {
                Ok(_) => (),
                Err(e) => {
                    log::error!("sync serve: {e}");
                    continue;
                }
            };
        }
    }
}

#[cfg(feature = "net-wifi")]
#[embassy_executor::task]
pub async fn sync_wifi(
    client: Arc<Mutex<daemon::Client>>,
    network: crate::net::wifi::WifiNetwork<'static>,
) {
    log::info!("WiFi syncer does nothing, lol");
}

#[cfg(feature = "net-irda")]
#[embassy_executor::task]
pub async fn sync_irda(
    imp: Imp<NeopixelSink>,
    network: crate::net::irda::IrNetworkInterface<'static>,
    peers: heapless::Vec<u16, MAX_PEERS>,
) {
    log::info!("IrDA syncer started");
    let engine = SyncEngine::new(imp, network, peers);

    embassy_futures::join::join(engine.initiate(), engine.serve()).await;
}

#[cfg(feature = "net-esp-now")]
#[embassy_executor::task]
pub async fn sync_esp_now(
    imp: Imp<NeopixelSink>,
    network: crate::net::espnow::EspNowNetworkInterface<'static>,
    peers: heapless::Vec<u16, MAX_PEERS>,
) {
    log::info!("ESP Now syncer started");
    let engine = SyncEngine::new(imp, network, peers);

    embassy_futures::join::join(engine.initiate(), engine.serve()).await;
}
