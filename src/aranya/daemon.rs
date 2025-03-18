use alloc::sync::Arc;
use aranya_crypto::{
    aead::{Aead, AeadKey},
    default::*,
    keys::SecretKeyBytes,
    keystore::memstore::MemStore,
    CipherSuite,
};
use aranya_runtime::{linear::LinearStorageProvider, vm_action, ClientState, GraphId};

use crate::storage::sd::io_manager::GraphManager;

use super::{engine::EmbeddedEngine, error::*, sink::VecSink};

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

type Mutex<T> = embassy_sync::mutex::Mutex<embassy_sync::blocking_mutex::raw::NoopRawMutex, T>;

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
        let mut sink = VecSink::new();

        // Temporarily fix the nonce for demo purposes, TODO: remove
        //Rng.fill_bytes(&mut nonce);
        let nonce = [0u8; 16];

        let mut aranya = self.aranya.lock().await;
        let graph_id =
            aranya.new_graph(&[0u8], vm_action!(create_team(nonce.as_slice())), &mut sink)?;

        Ok(graph_id)
    }

    pub fn get_client(&self) -> Arc<Mutex<Client>> {
        Arc::clone(&self.aranya)
    }
}
