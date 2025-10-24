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
    attributes {
        init: true,
    }

    fields {
        nonce bytes,
    }

    seal { return envelope::do_seal(serialize(this)) }
    open { return deserialize(envelope::do_open(envelope)) }

    policy {
        finish {
            emit TeamCreated {}
        }
    }
}

action set_led(r int, g int, b int) {
    publish SetLedColor {
        r: r,
        g: g,
        b: b,
    }
}

effect LedColorChanged {
    r int,
    g int,
    b int,
}

command SetLedColor {
    attributes {
        priority: 0,
    }

    fields {
        r int,
        g int,
        b int,
    }

    seal { return envelope::do_seal(serialize(this)) }
    open { return deserialize(envelope::do_open(envelope)) }

    policy {
        finish {
            emit LedColorChanged {r: this.r, g: this.g, b: this.b}
        }
    }
}
```
