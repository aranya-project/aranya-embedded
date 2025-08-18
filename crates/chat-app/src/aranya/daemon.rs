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
    linear::LinearStorageProvider, vm_action, ClientState, GraphId, VmAction, VmEffect,
};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_time::{with_timeout, Duration};
use esp_println::println;

use super::{engine::EmbeddedEngine, error::*, sink::DebugSink};
#[cfg(feature = "net-esp-now")]
use crate::net::espnow::EspNowNetworkInterface;
#[cfg(feature = "net-irda")]
use crate::net::irda::IrNetworkInterface;
use crate::{
    aranya::{sink::PubSubSink, syncer::SyncEngine},
    storage::imp::*,
};

const ACTION_BOOST: u8 = 7;

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

type Channel<T> = embassy_sync::channel::Channel<CriticalSectionRawMutex, T, 10>;
pub type PubSubChannel<T> =
    embassy_sync::pubsub::PubSubChannel<CriticalSectionRawMutex, T, 20, 1, 4>;
pub type Publisher<'a, T> =
    embassy_sync::pubsub::Publisher<'a, CriticalSectionRawMutex, T, 20, 1, 4>;
pub type Subscriber<'a, T> =
    embassy_sync::pubsub::Subscriber<'a, CriticalSectionRawMutex, T, 20, 1, 4>;

// TODO(chip): use actual keys
const NULL_KEY: [u8; 32] = [0u8; 32];

pub static ACTION_IN_CHANNEL: Channel<VmAction<'static>> = Channel::new();
pub static EFFECT_OUT_CHANNEL: PubSubChannel<VmEffect> = PubSubChannel::new();

pub struct Daemon<'a> {
    aranya: Client,
    #[cfg(feature = "net-esp-now")]
    syncer_esp_now: Option<SyncEngine<'a, EspNowNetworkInterface<'a>>>,
    #[cfg(feature = "net-irda")]
    syncer_ir: Option<SyncEngine<'a, IrNetworkInterface<'a>>>,
}

impl<'a> Daemon<'a> {
    pub async fn init(storage_provider: SP) -> Result<Self> {
        log::info!("Loading Crypto Engine");
        let crypto_engine = {
            let key = AeadKey::new(SecretKeyBytes::new(NULL_KEY.into()));
            CE::new(&key, Rng)
        };

        log::info!("Loading Policy");
        let policy = EmbeddedEngine::new(crypto_engine)?;
        log::info!("Creating an Aranya client");
        let aranya = ClientState::new(policy, storage_provider);

        Ok(Daemon {
            aranya,
            #[cfg(feature = "net-esp-now")]
            syncer_esp_now: None,
            #[cfg(feature = "net-irda")]
            syncer_ir: None,
        })
    }

    #[cfg(feature = "net-esp-now")]
    pub fn add_esp_now_interface(
        &mut self,
        network_interface: EspNowNetworkInterface<'a>,
        graph_id: GraphId,
    ) {
        let syncer = SyncEngine::new(graph_id, network_interface);
        self.syncer_esp_now = Some(syncer);
    }

    #[cfg(feature = "net-irda")]
    pub fn add_irda_interface(
        &mut self,
        network_interface: IrNetworkInterface<'a>,
        graph_id: GraphId,
    ) {
        let syncer = SyncEngine::new(graph_id, network_interface);
        self.syncer_ir = Some(syncer);
    }

    pub async fn create_team(&mut self) -> Result<GraphId> {
        let mut sink = DebugSink {};

        // Temporarily fix the nonce for demo purposes, TODO(chip): remove once we have proper onboarding
        let nonce = [0u8; 16];
        //Rng.fill_bytes(&mut nonce);

        let graph_id =
            self.aranya
                .new_graph(&[0u8], vm_action!(create_team(nonce.as_slice())), &mut sink)?;

        Ok(graph_id)
    }

    pub async fn run(&mut self, graph_id: GraphId) -> Result<()> {
        let mut sink = PubSubSink::new();
        #[cfg(feature = "net-esp-now")]
        let syncer_esp_now = self
            .syncer_esp_now
            .as_mut()
            .expect("No ESP Now syncer configured");
        #[cfg(feature = "net-irda")]
        let syncer_ir = self.syncer_ir.as_mut().expect("No IR syncer configured");

        loop {
            match with_timeout(Duration::from_millis(100), ACTION_IN_CHANNEL.receive()).await {
                Ok(action) => match self.aranya.action(graph_id, &mut sink, action) {
                    Ok(_) => {
                        #[cfg(feature = "net-esp-now")]
                        syncer_esp_now.boost_hello(ACTION_BOOST, true);
                        #[cfg(feature = "net-irda")]
                        syncer_ir.boost_hello();
                    }
                    Err(err) => println!("Error from action: {err}"),
                },
                Err(_) => (),
            }
            #[cfg(feature = "net-esp-now")]
            syncer_esp_now.process(&mut self.aranya).await;
            #[cfg(feature = "net-irda")]
            syncer_ir.process(&mut self.aranya).await;
        }
    }
}

#[embassy_executor::task]
pub async fn daemon_task(mut daemon: Daemon<'static>, graph_id: GraphId) {
    daemon.run(graph_id).await.expect("daemon failed");
}

#[macro_export]
macro_rules! vm_action_owned {
    ($name:ident($($arg:expr),* $(,)?)) => {
        ::aranya_runtime::VmAction {
            name: ::aranya_policy_vm::ident!(stringify!($name)),
            args: ::alloc::vec![$(::aranya_policy_vm::Value::from($arg)),*].into(),
        }
    };
}
