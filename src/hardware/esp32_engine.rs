use alloc::vec;
use alloc::{boxed::Box, vec::Vec};
use aranya_crypto::keystore::memstore::MemStore;
use aranya_crypto::{default::DefaultEngine, UserId};
use aranya_crypto::{Engine, Rng};
use aranya_policy_vm::{Machine, Module};
use aranya_runtime::memory::MemStorageProvider;
use aranya_runtime::{EngineError, PolicyId, VmEffect};
use aranya_runtime::{FfiCallable, VmPolicy};
use rkyv::rancor::Error;
use rkyv::util::AlignedVec;

pub const SERIALIZED_POLICY: &[u8] = include_bytes!("../built/serialized_policy.bin");

pub struct ESP32Engine<E>
where
    E: Engine,
{
    // VM  Policy implements crypto engine not runtime engine
    pub policy: VmPolicy<E>,
}

// todo: When we have a no-std policy parser remove rust build files and simplify
impl<E> ESP32Engine<E>
where
    E: Engine,
{
    pub fn new() -> ESP32Engine<DefaultEngine> {
        // Setting alignment 8 prevents errors in deserialization
        let mut vec = AlignedVec::<8>::new();
        vec.extend_from_slice(SERIALIZED_POLICY);
        let module: Module =
            rkyv::from_bytes::<Module, Error>(&vec).expect("Failed to serialize Module");
        let machine = Machine::from_module(module).expect("Couldn't get machine from module");
        let (engine, _key) = DefaultEngine::from_entropy(Rng);
        // In memory crypto keystore
        // !TODO Make a on file keystore, not in memory.

        let store = MemStore::new();
        // Meant to be unique for every user/device
        let user_id = UserId::random(&mut Rng);
        let ffis: Vec<Box<dyn FfiCallable<DefaultEngine> + Send + 'static>> = vec![
            Box::from(aranya_envelope_ffi::Ffi),
            Box::from(aranya_crypto_ffi::Ffi::new(store.clone())),
            Box::from(aranya_device_ffi::FfiDevice::new(user_id)),
            Box::from(aranya_perspective_ffi::FfiPerspective),
            Box::from(aranya_idam_ffi::Ffi::new(store)),
        ];
        let policy = VmPolicy::new(machine, engine, ffis).expect("Could not load policy");
        ESP32Engine { policy }
    }
}

impl<E> aranya_runtime::Engine for ESP32Engine<E>
where
    E: aranya_crypto::Engine + ?Sized,
{
    type Policy = VmPolicy<E>;
    type Effect = VmEffect;

    fn add_policy(&mut self, policy: &[u8]) -> Result<PolicyId, EngineError> {
        Ok(PolicyId::new(policy[0] as usize))
    }

    fn get_policy(&self, _id: PolicyId) -> Result<&Self::Policy, EngineError> {
        Ok(&self.policy)
    }
}
