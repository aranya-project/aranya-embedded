use alloc::vec;
use alloc::{boxed::Box, vec::Vec};
use aranya_crypto::{default::DefaultEngine, UserId};
use aranya_crypto::{Engine, Rng};
use aranya_policy_vm::{Machine, Module};
use aranya_runtime::{EngineError, PolicyId, VmEffect};
use aranya_runtime::{FfiCallable, VmPolicy};
use ciborium::de::from_reader;

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
        let module: Module = from_reader(SERIALIZED_POLICY).expect("Failed to deserialize Module");
        let machine = Machine::from_module(module).expect("Couldn't get machine from module");
        let (engine, _key) = DefaultEngine::from_entropy(Rng);
        // todo: FFIs
        let policy = VmPolicy::new(machine, engine, Vec::new()).expect("Could not load policy");
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

    fn get_policy<'a>(&'a self, _id: PolicyId) -> Result<&'a Self::Policy, EngineError> {
        Ok(&self.policy)
    }
}
