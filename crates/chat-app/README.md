# Aranya Mesh Chat

Aranya Mesh Chat is a decentralized messaging application that runs on
ESP32-S3 devices using ESP-NOW. A client web application connects over
WebUSB, allowing the user to send and receive messages.

## Prerequisites

- [Rust](https://www.rust-lang.org/learn/get-started)
- The [ESP Rust
  toolchain](https://docs.espressif.com/projects/rust/book/installation/riscv-and-xtensa.html)
- [espflash](https://github.com/esp-rs/espflash) - (`cargo install espflash@^3 --locked`)

While this might theoretically build on Windows, getting the ESP
toolchain working on Windows is a _noted_ pain, so we recommend building
via WSL instead.

## Building

`cargo run` should build the application and deploy it with `espflash`.

### Flashing from WSL2

In order for WSL2 to see the device, you'll have to forward the USB
device into the WSL2 VM. Doing this is outside the scope of this README,
but [Microsoft has instructions
here](https://learn.microsoft.com/en-us/windows/wsl/connect-usb).

## Running

Parameters will have to be set for the device's address and flashed to
the device before it can communicate with other devices. The address is
a 16-bit integer. You can set the address with:

```
# (from the repository root)
$ cargo run --bin aranya-embedded-config -- -c --ir-address <ADDRESS> -c params.bin
```

Then you can flash the parameter file to the device with espflash:

```
$ espflash write-bin 0x9000 params.bin
```

The parameter storage is by default at 0x9000. If you've changed it in
`partitions.csv`, use that address.

Once it's flashed, unplug and replug the device. The LED should blink
orange briefly and it will show up as a serial device (except on
Windows, where it shows up as a generic USB device for reasons explained
below).

You can then open `web/client.html` and connect to the device. Or go to
[https://chip-so.github.io/chat/](https://chip-so.github.io/chat/).

## Windows shenanigans

I think what's happening is that accessing USB devices through
unprivileged applications requires that the devices use the WinUSB
driver. The firmware has some extra descriptors to tell Windows to do
this, so that it can be accessed via WebUSB. But this means it won't
show up as a COM port. If you'd like to access it as a COM port instead,
you can use something like [Zadig](https://zadig.akeo.ie/) to override
the WinUSB device selection.
