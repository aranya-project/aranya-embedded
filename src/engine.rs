use aranya_runtime::{
    Address, Command, CommandId, CommandRecall, Engine, EngineError, MergeIds, Perspective, Policy,
    PolicyId, Prior, Priority, Sink, VmAction,
};

pub struct EmbeddedAction;
pub struct EmbeddedCommand;

impl Command for EmbeddedCommand {
    fn priority(&self) -> Priority {
        Priority::Init
    }

    fn id(&self) -> CommandId {
        CommandId::default()
    }

    fn parent(&self) -> Prior<Address> {
        Prior::None
    }

    fn policy(&self) -> Option<&[u8]> {
        None
    }

    fn bytes(&self) -> &[u8] {
        &[0]
    }
}

pub struct EmbeddedPolicy;

impl Policy for EmbeddedPolicy {
    type Action<'a> = VmAction<'a>;
    type Effect = EmbeddedEffect;
    type Command<'a> = EmbeddedCommand;

    fn serial(&self) -> u32 {
        0
    }

    fn call_rule(
        &self,
        command: &impl Command,
        facts: &mut impl aranya_runtime::FactPerspective,
        sink: &mut impl Sink<Self::Effect>,
        recall: CommandRecall,
    ) -> Result<(), EngineError> {
        Ok(())
    }

    fn call_action(
        &self,
        action: Self::Action<'_>,
        facts: &mut impl Perspective,
        sink: &mut impl Sink<Self::Effect>,
    ) -> Result<(), EngineError> {
        Ok(())
    }

    fn merge<'a>(
        &self,
        target: &'a mut [u8],
        ids: MergeIds,
    ) -> Result<Self::Command<'a>, EngineError> {
        Err(EngineError::Panic)
    }
}

pub struct EmbeddedEffect;

pub struct PolicyEngine;

impl Engine for PolicyEngine {
    type Policy = EmbeddedPolicy;
    type Effect = EmbeddedEffect;

    fn add_policy(&mut self, policy: &[u8]) -> Result<PolicyId, EngineError> {
        Err(EngineError::Panic)
    }

    fn get_policy(&self, id: aranya_runtime::PolicyId) -> Result<&Self::Policy, EngineError> {
        Err(EngineError::Panic)
    }
}
