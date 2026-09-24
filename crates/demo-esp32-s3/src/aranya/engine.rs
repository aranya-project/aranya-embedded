use alloc::vec;

use aranya_crypto::{default::WrappedKey, DeviceId, Engine};
use aranya_policy_vm::{Machine, Module};
use aranya_runtime::{vm_policy::SealCtx, PolicyError, PolicyId, VmEffect, VmPolicy};
use rkyv::{rancor::Error as RancorError, util::AlignedVec};

use super::error::Result as DaemonResult;

pub const SERIALIZED_POLICY: &[u8] = include_bytes!("../built/serialized_policy.bin");
pub const WRAPPED_SIGNING_KEY: &[u8] = &[
    32, 70, 238, 102, 23, 87, 185, 89, 43, 55, 85, 131, 178, 107, 160, 221, 74, 66, 169, 159, 201,
    243, 43, 116, 25, 116, 230, 98, 90, 86, 95, 134, 66, 66, 209, 14, 160, 61, 253, 61, 249, 227,
    34, 37, 188, 5, 32, 243, 158, 89, 136, 38, 1, 58, 36, 199, 225, 13, 24, 25, 252, 107, 15, 20,
    138, 168, 203, 55, 96, 191, 80, 125, 30, 67, 244, 61, 118, 69, 194, 88, 50, 94, 241, 243, 81,
    0, 93, 167, 56, 232, 205, 149, 78, 31,
];

pub struct EmbeddedPolicyStore<CE>
where
    CE: Engine,
{
    // VM  Policy implements crypto engine not runtime engine
    pub policy: VmPolicy<CE>,
    pub seal_ctx: SealCtx<CE>,
}

// todo: When we have a no-std policy parser remove rust build files and simplify
impl<CE> EmbeddedPolicyStore<CE>
where
    CE: Engine,
{
    pub fn new(crypto_engine: CE) -> DaemonResult<EmbeddedPolicyStore<CE>> {
        let key = crypto_engine
            .unwrap(&postcard::from_bytes(WRAPPED_SIGNING_KEY).expect("can deserialize key"))
            .expect("can unwrap key");
        let seal_ctx = SealCtx {
            author: DeviceId::default(),
            key,
        };

        // Setting alignment 8 prevents errors in deserialization
        let mut vec = AlignedVec::<8>::new();
        vec.extend_from_slice(SERIALIZED_POLICY);
        let module: Module = rkyv::from_bytes::<Module, RancorError>(&vec)?;
        let machine = Machine::from_module(module)?;
        let policy = VmPolicy::new(machine, crypto_engine, vec![]).expect("Could not load policy");
        Ok(EmbeddedPolicyStore { policy, seal_ctx })
    }
}

impl<CE> aranya_runtime::PolicyStore for EmbeddedPolicyStore<CE>
where
    CE: aranya_crypto::Engine,
{
    type Policy = VmPolicy<CE>;
    type Effect = VmEffect;

    fn add_policy(&mut self, policy: &[u8]) -> Result<PolicyId, PolicyError> {
        Ok(PolicyId::new(policy[0].into()))
    }

    fn get_policy(&self, _id: PolicyId) -> Result<&Self::Policy, PolicyError> {
        Ok(&self.policy)
    }

    fn seal_ctx(
        &self,
        id: PolicyId,
    ) -> Result<&<Self::Policy as aranya_runtime::Policy>::SealCtx, PolicyError> {
        Ok(&self.seal_ctx)
    }
}
