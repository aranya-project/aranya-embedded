---
policy-version: 2
---

```policy
base command BaseInit {
    fields { key bytes }
    get_key { return Some(this.key) }
}

base command Base {
    get_key {
        return match query Key[] {
            Some(f) => Some(f.key)
            None => None
        }
    }
}

fact Key[]=>{key bytes}
```

```policy
action create_team(nonce bytes, key bytes) {
    publish Init { nonce, key }
}

effect TeamCreated {}

command Init with BaseInit {
    attributes {
        init: true,
    }

    fields {
        nonce bytes,
    }

    policy {
        finish {
            create Key[]=>{key: this.key}
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

command SetLedColor with Base {
    attributes {
        priority: 0,
    }

    fields {
        r int,
        g int,
        b int,
    }

    policy {
        finish {
            emit LedColorChanged {r: this.r, g: this.g, b: this.b}
        }
    }
}
```
