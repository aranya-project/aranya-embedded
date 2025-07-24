use alloc::sync::Arc;
use core::ops::DerefMut;

use aranya_crypto::{
    dangerous::spideroak_crypto::{
        aead::{Aead, AeadKey},
        keys::SecretKeyBytes,
    },
    default::*,
    keystore::memstore::MemStore,
    CipherSuite,
};
use aranya_runtime::{
    linear::LinearStorageProvider, vm_action, ClientError, ClientState, Command, GraphId,
    PeerCache, Sink, Storage, StorageProvider, Transaction, VmEffect,
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::MutexGuard};

use super::{engine::EmbeddedEngine, error::*, sink::DebugSink};
use crate::storage::imp::*;

// Use short names so we can more easily add generics.
/// CE = Crypto Engine
pub(crate) type CE = DefaultEngine;
/// CS = Cipher Suite
pub(crate) type CS = DefaultCipherSuite;
/// KS = KeyStore
pub(crate) type KS = MemStore;
/// PE = Policy Engine
pub(crate) type PE = EmbeddedEngine<CE>;
/// SP = Storage Provider
#[cfg(feature = "storage-sd")]
pub(crate) type SP = LinearStorageProvider<GraphManager>;
#[cfg(feature = "storage-internal")]
pub(crate) type SP = LinearStorageProvider<EspPartitionIoManager<FlashStorage>>;
/// Aranya Client
pub(crate) type Client = ClientState<PE, SP>;

type KeyWrapKeyBytes = SecretKeyBytes<<<CS as CipherSuite>::Aead as Aead>::KeySize>;
type KeyWrapKey = <<CS as CipherSuite>::Aead as Aead>::Key;

type Mutex<T> =
    embassy_sync::mutex::Mutex<embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex, T>;

// TODO(chip): use actual keys
const NULL_KEY: [u8; 32] = [0u8; 32];

pub struct Daemon {
    aranya: Arc<Mutex<Client>>,
}

impl Daemon {
    pub async fn init(storage_provider: SP) -> Result<Self> {
        log::info!("Loading Crypto Engine");
        let crypto_engine = {
            let key = AeadKey::new(SecretKeyBytes::new(NULL_KEY.into()));
            CE::new(&key, Rng)
        };

        log::info!("Loading Policy");
        let policy = EmbeddedEngine::new(crypto_engine)?;
        log::info!("Creating an Aranya client");
        let aranya = Arc::new(Mutex::new(ClientState::new(policy, storage_provider)));

        Ok(Daemon { aranya })
    }

    pub async fn create_team(&mut self) -> Result<GraphId> {
        let mut sink = DebugSink {};

        // Temporarily fix the nonce for demo purposes, TODO(chip): remove once we have proper onboarding
        let nonce = [0u8; 16];
        //Rng.fill_bytes(&mut nonce);

        let mut aranya = self.aranya.lock().await;
        let graph_id =
            aranya.new_graph(&[0u8], vm_action!(create_team(nonce.as_slice())), &mut sink)?;

        Ok(graph_id)
    }

    pub fn get_imp<S: Sink<VmEffect>>(&self, graph_id: GraphId, sink: S) -> Imp<S> {
        Imp {
            client: Arc::clone(&self.aranya),
            graph_id,
            sink: Mutex::new(sink),
        }
    }
}

/// A shareable interface to the client that works on a single GraphId.
pub struct Imp<S: Sink<VmEffect>> {
    client: Arc<Mutex<Client>>,
    graph_id: GraphId,
    sink: Mutex<S>,
}

impl<S: Sink<VmEffect>> Imp<S> {
    /// Lock the client and return a MutexGuard for it.
    pub async fn get_client(&self) -> MutexGuard<'_, CriticalSectionRawMutex, Client> {
        self.client.lock().await
    }

    pub fn graph_id(&self) -> GraphId {
        self.graph_id
    }

    pub async fn add_commands(
        &self,
        cmds: &[impl Command + core::fmt::Debug],
        trx: &mut Option<Transaction<SP, PE>>,
        peer_cache: &mut PeerCache,
    ) -> Result<()> {
        let mut client = self.get_client().await;
        let trx = trx.get_or_insert_with(|| client.transaction(self.graph_id()));
        let mut sink = self.sink.lock().await;
        dump_commands(cmds);
        client.add_commands(trx, sink.deref_mut(), cmds)?;

        // Update peer cache
        let addresses = cmds.iter().filter_map(|cmd| cmd.address().ok());
        let storage = client
            .provider()
            .get_storage(self.graph_id)
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

    pub async fn commit(&self, mut trx: Transaction<SP, PE>) -> Result<()> {
        let mut client = self.get_client().await;
        let mut sink = self.sink.lock().await;
        client.commit(&mut trx, sink.deref_mut())?;
        Ok(())
    }

    pub async fn call_action(&self, action: aranya_runtime::VmAction<'_>) -> Result<()> {
        let mut aranya = self.get_client().await;
        let mut sink = self.sink.lock().await;
        Ok(aranya.action(self.graph_id, sink.deref_mut(), action)?)
    }
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

/* TODO(chip): when we have an async version of Actor
impl Actor for Imp {
    fn call_action(
        &mut self,
        action: aranya_runtime::VmAction<'_>,
    ) -> Result<(), aranya_runtime::ClientError> {
        let mut aranya = embassy_futures::block_on(self.get_client());
        let mut sink = DebugSink {};
        aranya.action(self.graph_id, &mut sink, action)
    }
} */
