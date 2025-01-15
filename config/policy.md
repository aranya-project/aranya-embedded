---
policy-version: 1
---

```policy
// FFIs act as external libraries we can use
use envelope
use crypto
use device
use perspective
use idam

// An action is a function callable from the application
action set_bool(on bool, sign_pk id) {
    publish SetBool{
        on: on,
        sign_pk: sign_pk,
    }
}

// effect is a specific kind of struct declaration used to emit information from the VM to the application
effect LEDBool {
    on bool,
}

// A user's public SigningKey.
fact UserSignKey[user_id id]=>{key_id id, key bytes}

// Commands define structured data (the fields block), rules for transforming that structured data to and from a transportable format (seal and open blocks), and policy decisions for determining the validity and effects of that data (the policy and recall blocks).
command SetBool {
    // defines the fields of the command (The core data to pass in)
    fields {
        on bool,
        sign_pk id,
    }

    // Serialization mechanism for the command
    // seal has an implicit argument `this`, which contains the pending command's fields
    // should return a valid envelope
    seal {
        let parent_id = perspective::head_id()
        let payload = serialize(this)
        let author_sign_sk_id = idam::derive_sign_key_id(this.sign_pk)

        let signed = crypto::sign(
            author_sign_sk_id,
            payload,
        )

        let author_id = device::current_user_id()

        return envelope::new(
            parent_id,
            author_id,
            signed.command_id,
            signed.signature,
            payload,
        )
    }

    // Deserialization mechanism for the command
    // `open` has an implicit argument `envelope`, an envelope type
    // should return a command struct with the command's fields
    open {
        let author_id = envelope::author_id(envelope)
        let parent_id = envelope::parent_id(envelope)
        let payload = envelope::payload(envelope)
        let cmd = deserialize(payload)
        let author_sign_pk = cmd.sign_pk

        let crypto_command = crypto::verify(
            author_sign_pk,
            parent_id,
            payload,
            envelope::command_id(envelope),
            envelope::signature(envelope),
        )
        return deserialize(crypto_command)
    }



    policy {
        finish {
            emit LEDBool{on: this.on}
        }
    }
}
```