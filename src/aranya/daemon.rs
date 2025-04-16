use alloc::sync::Arc;
use aranya_crypto::{
    aead::{Aead, AeadKey},
    default::*,
    keys::SecretKeyBytes,
    keystore::memstore::MemStore,
    CipherSuite,
};
use aranya_runtime::{linear::LinearStorageProvider, vm_action, ClientState, GraphId};

use crate::storage::imp::*;

use super::{engine::EmbeddedEngine, error::*, sink::DebugSink};

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
        let mut sink = DebugSink {};

        // Temporarily fix the nonce for demo purposes, TODO(chip): remove once we have proper onboarding
        let nonce = [0u8; 16];
        //Rng.fill_bytes(&mut nonce);

        let mut aranya = self.aranya.lock().await;
        let graph_id =
            aranya.new_graph(&[0u8], vm_action!(create_team(nonce.as_slice())), &mut sink)?;

        Ok(graph_id)
    }

    pub async fn set_led<I>(&mut self, storage_id: GraphId, red: I, green: I, blue: I) -> Result<()>
    where
        I: Into<i64>,
    {
        let mut aranya = self.aranya.lock().await;
        let mut sink = DebugSink {};
        aranya.action(
            storage_id,
            &mut sink,
            vm_action!(set_led(red.into(), green.into(), blue.into())),
        )?;
        Ok(())
    }

    pub fn get_client(&self) -> Arc<Mutex<Client>> {
        Arc::clone(&self.aranya)
    }
}
