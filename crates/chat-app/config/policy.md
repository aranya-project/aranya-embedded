---
policy-version: 2
---

```policy
use envelope

enum AmbientColor {
    Black,
    Blue,
    Red,
    Green,
    Magenta,
    Cyan,
    Yellow,
    White,
}

fact CurrentColor[]=>{color enum AmbientColor}

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

    seal { return envelope::do_seal(serialize(this)) }
    open { return deserialize(envelope::do_open(envelope)) }

    policy {
        finish {
            create CurrentColor[]=>{color: AmbientColor::Black}
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

    seal { return envelope::do_seal(serialize(this)) }
    open { return deserialize(envelope::do_open(envelope)) }

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

## Rainbow

```policy
action send_rainbow(author id) {
    // TODO: publish command
}

effect RainbowEffect {
    author id
}

command Rainbow {
    fields {
        author id
    }

    seal { return envelope::do_seal(serialize(this)) }
    open { return deserialize(envelope::do_open(envelope)) }

    policy {
        finish {
            emit RainbowEffect {
                author: this.author
            }
        }
    }
}
```

# Ambient LED Color

```policy
// TODO: write action to set ambient LED color

effect AmbientColorChanged {
    author id,
    color enum AmbientColor,
}

command SetAmbientColor {
    fields {
        author id,
        color enum AmbientColor,
    }

    seal { return envelope::do_seal(serialize(this)) }
    open { return deserialize(envelope::do_open(envelope)) }

    policy {
        finish {
            emit AmbientColorChanged {
                author: this.author,
                color: this.color,
            }
        }
    }
}
```