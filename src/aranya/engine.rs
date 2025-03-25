use alloc::vec;
use alloc::{boxed::Box, vec::Vec};
use aranya_crypto::Engine;
use aranya_crypto::UserId;
use aranya_policy_vm::{Machine, Module};
use aranya_runtime::{EngineError, PolicyId, VmEffect};
use aranya_runtime::{FfiCallable, VmPolicy};
use rkyv::rancor::Error as RancorError;
use rkyv::util::AlignedVec;

use super::envelope::NullEnvelope;
use super::error::Result as DaemonResult;

pub const SERIALIZED_POLICY: &[u8] = include_bytes!("../built/serialized_policy.bin");

pub struct EmbeddedEngine<E>
where
    E: Engine,
{
    // VM  Policy implements crypto engine not runtime engine
    pub policy: VmPolicy<E>,
}

// todo: When we have a no-std policy parser remove rust build files and simplify
impl<E> EmbeddedEngine<E>
where
    E: Engine,
{
    pub fn new(crypto_engine: E) -> DaemonResult<EmbeddedEngine<E>> {
        // Setting alignment 8 prevents errors in deserialization
        let mut vec = AlignedVec::<8>::new();
        vec.extend_from_slice(SERIALIZED_POLICY);
        let module: Module = rkyv::from_bytes::<Module, RancorError>(&vec)?;
        let machine = Machine::from_module(module)?;
        let ffis: Vec<Box<dyn FfiCallable<E> + Send + 'static>> = vec![Box::from(NullEnvelope {
            user: UserId::default(),
        })];
        let policy = VmPolicy::new(machine, crypto_engine, ffis).expect("Could not load policy");
        Ok(EmbeddedEngine { policy })
    }
}

impl<E> aranya_runtime::Engine for EmbeddedEngine<E>
where
    E: aranya_crypto::Engine,
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
