#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use core::convert::Infallible;

use aranya_crypto::{BaseId, DeviceId};
use aranya_policy_vm::{ffi::ffi, CommandContext, MachineError};

/// An Envelope that does no crypto
pub struct NullEnvelope {
    pub user: DeviceId,
}

#[ffi(
    module = "envelope",
    def = r#"
struct Envelope {
    // The parent command ID.
    parent_id id,
    // The author's user ID.
    author_id id,
    // Uniquely identifies the command.
    command_id id,
    // The encoded command.
    payload bytes,
    // The signature over the command and its contextual
    // bindings.
    signature bytes,
}
"#
)]
impl NullEnvelope {
    #[ffi_export(def = "function do_seal(payload bytes) struct Envelope")]
    fn seal<E>(
        &self,
        ctx: &CommandContext,
        _eng: &mut E,
        payload: Vec<u8>,
    ) -> Result<Envelope, MachineError> {
        let CommandContext::Seal(ctx) = ctx else {
            panic!("envelope::do_seal called outside seal context");
        };

        let parent_id = ctx.head_id;
        let author_id = self.user;

        let command_id: BaseId = {
            use aranya_crypto::dangerous::spideroak_crypto::{hash::Hash, rust::Sha256};
            let mut hasher = Sha256::new();
            hasher.update(parent_id.as_bytes());
            hasher.update(author_id.as_bytes());
            hasher.update(&payload);
            BaseId::from_bytes(hasher.digest().into_array().into())
        };

        Ok(Envelope {
            parent_id: parent_id.as_base(),
            author_id: author_id.as_base(),
            command_id,
            payload,
            // TODO(chip): use an actual signature
            signature: b"LOL".to_vec(),
        })
    }

    #[ffi_export(def = "function do_open(envelope_input struct Envelope) bytes")]
    fn open<E>(
        &self,
        _ctx: &CommandContext,
        _eng: &mut E,
        envelope_input: Envelope,
    ) -> Result<Vec<u8>, Infallible> {
        Ok(envelope_input.payload)
    }
}
