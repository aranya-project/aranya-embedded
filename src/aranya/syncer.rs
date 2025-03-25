use core::marker::PhantomData;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use aranya_crypto::{default::DefaultEngine, Csprng, Rng};
use aranya_runtime::{
    vm_action, ClientState, Engine, GraphId, PeerCache, Sink, StorageProvider, SyncRequester,
    VmEffect, VmPolicy, MAX_SYNC_MESSAGE_SIZE,
};
use embassy_time::Timer;

use crate::{
    aranya::error::Result,
    net::{Message, Network},
};

use super::daemon;

type Mutex<T> = embassy_sync::mutex::Mutex<embassy_sync::blocking_mutex::raw::NoopRawMutex, T>;

/// A response to a sync request.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum SyncResponse {
    /// Success.
    Ok(Box<[u8]>),
    /// Failure.
    Err(String),
}

/// Aranya client.
struct SyncEngine<EN, SP, CE, N> {
    /// Thread-safe Aranya client reference.
    aranya: Arc<Mutex<ClientState<EN, SP>>>,
    network: N,
    _eng: PhantomData<CE>,
}

impl<EN, SP, CE, N> SyncEngine<EN, SP, CE, N> {
    /// Creates a new [`Client`].
    pub fn new(aranya: Arc<Mutex<ClientState<EN, SP>>>, network: N) -> Self {
        SyncEngine {
            aranya,
            network,
            _eng: PhantomData,
        }
    }
}

impl<EN, SP, CE, N> SyncEngine<EN, SP, CE, N>
where
    EN: Engine<Policy = VmPolicy<CE>, Effect = VmEffect> + Send + 'static,
    SP: StorageProvider + Send + 'static,
    CE: aranya_crypto::Engine + Send + Sync + 'static,
    N: Network,
    <N as Network>::Addr: Default,
{
    /// Syncs with the peer.
    /// Aranya client sends a `SyncRequest` to peer then processes the `SyncResponse`.
    async fn sync_peer<S>(&self, id: GraphId, sink: &mut S, peer_addr: N::Addr) -> Result<()>
    where
        S: Sink<<EN as Engine>::Effect>,
    {
        // send the sync request.

        // TODO: Real server address.
        let server_addr = ();
        let mut syncer = SyncRequester::new(id, &mut Rng, server_addr);
        let mut send_buf = vec![0u8; MAX_SYNC_MESSAGE_SIZE];

        let (len, _) = {
            let mut client = self.aranya.lock().await;
            // TODO: save PeerCache somewhere.
            syncer.poll(&mut send_buf, client.provider(), &mut PeerCache::new())?
        };
        log::debug!("sync poll finished, len {len}");
        send_buf.truncate(len);
        let m = Message::new_to(peer_addr, send_buf);
        self.network.send_message(m).await?;
        let response = self.network.recv_message().await?;

        // process the sync response.
        let resp = postcard::from_bytes(&response.contents)?;
        let data = match resp {
            SyncResponse::Ok(data) => data,
            SyncResponse::Err(msg) => panic!("sync error: {msg}"),
        };
        if data.is_empty() {
            log::debug!("nothing to sync");
            return Ok(());
        }
        if let Some(cmds) = syncer.receive(&data)? {
            log::debug!("received {} commands", cmds.len());
            if !cmds.is_empty() {
                let mut client = self.aranya.lock().await;
                let mut trx = client.transaction(id);
                // TODO: save PeerCache somewhere.
                client.add_commands(&mut trx, sink, &cmds)?;
                client.commit(&mut trx, sink)?;
                // TODO: Update heads
                // client.update_heads(
                //     id,
                //     cmds.iter().filter_map(|cmd| cmd.address().ok()),
                //     heads,
                // )?;
                log::debug!("committed");
            }
        }

        Ok(())
    }

    /// Wait forever for requests and handle them. This does not return.
    pub async fn serve(&self) -> ! {
        loop {
            let msg = match self.network.recv_message().await {
                Ok(x) => x,
                Err(e) => {
                    log::error!("{e}");
                    continue;
                } // TODO(chip): process sync request
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
    client: Arc<Mutex<daemon::Client>>,
    network: crate::net::irda::IrNetworkInterface<'static>,
) {
    log::info!("IrDA syncer started");

    let send_fut = async {
        loop {
            let msg = Message::new_to(0, Vec::from(b"hello"));
            network
                .send_message(msg)
                .await
                .inspect_err(|e| log::error!("send_error: {e}"))
                .ok();
            Timer::after_secs(1).await;
        }
    };

    let recv_fut = async {
        loop {
            let r = network.recv_message().await;
            match r {
                Ok(msg) => log::info!("{:?}", msg),
                Err(e) => log::error!("recv error: {e}"),
            }
        }
    };

    embassy_futures::join::join(send_fut, recv_fut).await;
}
