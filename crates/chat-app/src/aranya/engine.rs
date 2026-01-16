use alloc::{boxed::Box, vec, vec::Vec};

use aranya_crypto::{DeviceId, Engine};
use aranya_policy_vm::{Machine, Module};
use aranya_runtime::{PolicyError, FfiCallable, PolicyId, VmEffect, VmPolicy};
use envelope_ffi::NullEnvelope;
use rkyv::{rancor::Error as RancorError, util::AlignedVec};

use super::error::Result as DaemonResult;

pub const SERIALIZED_POLICY: &[u8] = include_bytes!("../built/serialized_policy.bin");

pub struct EmbeddedPolicyStore<CE>
where
    CE: Engine,
{
    // VM  Policy implements crypto engine not runtime engine
    pub policy: VmPolicy<CE>,
}

// todo: When we have a no-std policy parser remove rust build files and simplify
impl<CE> EmbeddedPolicyStore<CE>
where
    CE: Engine,
{
    pub fn new(crypto_engine: CE) -> DaemonResult<EmbeddedPolicyStore<CE>> {
        // Setting alignment 8 prevents errors in deserialization
        let mut vec = AlignedVec::<8>::new();
        vec.extend_from_slice(SERIALIZED_POLICY);
        let module: Module = rkyv::from_bytes::<Module, RancorError>(&vec)?;
        let machine = Machine::from_module(module)?;
        let ffis: Vec<Box<dyn FfiCallable<CE> + Send + 'static>> = vec![Box::from(NullEnvelope {
            user: DeviceId::default(),
        })];
        let policy = VmPolicy::new(machine, crypto_engine, ffis).expect("Could not load policy");
        Ok(EmbeddedPolicyStore { policy })
    }
}

impl<CE> aranya_runtime::PolicyStore for EmbeddedPolicyStore<CE>
where
    CE: aranya_crypto::Engine,
{
    type Policy = VmPolicy<CE>;
    type Effect = VmEffect;

    fn add_policy(&mut self, policy: &[u8]) -> Result<PolicyId, PolicyError> {
        Ok(PolicyId::new(policy[0] as usize))
    }

    fn get_policy(&self, _id: PolicyId) -> Result<&Self::Policy, PolicyError> {
        Ok(&self.policy)
    }
}
