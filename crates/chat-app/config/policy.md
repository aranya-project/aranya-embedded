---
policy-version: 2
---

```policy
use envelope

action create_team(nonce bytes) {
    publish Init {
        nonce: nonce,
    }
}

effect TeamCreated {}

command Init {
    fields {
        nonce bytes,
    }

    seal { return envelope::seal(serialize(this)) }
    open { return deserialize(envelope::open(envelope)) }

    policy {
        finish {
            emit TeamCreated {}
        }
    }
}

action send_message(author id, msg string) {
    publish ChatMessage {
        author: author,
        msg: msg,
    }
}

effect MessageReceived {
    author id,
    msg string,
}

command ChatMessage {
    fields {
        author id,
        msg string,
    }

    seal { return envelope::seal(serialize(this)) }
    open { return deserialize(envelope::open(envelope)) }

    policy {
        finish {
            emit MessageReceived {
                author: this.author,
                msg: this.msg,
            }
        }
    }
}
```