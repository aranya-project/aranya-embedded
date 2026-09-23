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

action create_team(nonce bytes, key bytes) {
    publish Init { key, nonce }
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
            create CurrentColor[]=>{color: AmbientColor::Black}
            emit TeamCreated {}
        }
    }
}

action send_message(author id, msg string) {
    publish ChatMessage { author, msg }
}

effect MessageReceived {
    author id,
    msg string,
}

command ChatMessage with Base {
    attributes {
        priority: 0,
    }

    fields {
        author id,
        msg string,
    }

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

command Rainbow with Base {
    attributes {
        priority: 0,
    }

    fields {
        author id
    }

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

command SetAmbientColor with Base {
    attributes {
        priority: 0,
    }

    fields {
        author id,
        color enum AmbientColor,
    }

    policy {
        match query CurrentColor[] {
            Some(old) => {
                finish {
                    update CurrentColor[]=>{color: old.color} to {color: this.color}
                    emit AmbientColorChanged {
                        author: this.author,
                        color: this.color,
                    }
                }
            }
            None => {
                finish {
                    create CurrentColor[]=>{color:this.color}
                    emit AmbientColorChanged {
                        author: this.author,
                        color: this.color,
                    }
                }
            }
        }
    }
}
```
