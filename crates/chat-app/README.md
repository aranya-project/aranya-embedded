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

## Platform-specific shenanigans

### Windows 

I think what's happening is that accessing USB devices through
unprivileged applications requires that the devices use the WinUSB
driver. The firmware has some extra descriptors to tell Windows to do
this, so that it can be accessed via WebUSB. But this means it won't
show up as a COM port. If you'd like to access it as a COM port instead,
you can use something like [Zadig](https://zadig.akeo.ie/) to override
the WinUSB device selection.

Sometimes accessing it over WebUSB fails despite this. I don't know why.

### Linux (and maybe Android)

You will need to have read-write access to the usb device under
`/dev/bus/usb/...`. This is probably not the case by default. You will
probably have to add a udev rule to add access. Add this as a file under
/etc/udev/rules.d, e.g. `50-spideroak-demo-board.rules`.

```
SUBSYSTEM=="usb", ATTR{idVendor}=="303a", ATTR{idProduct}=="3001", MODE="0664", GROUP="plugdev"
```

Then unplug and replug the device. `plugdev` is the group that has
access to devices. If you're using an Ubuntu or Mint or something that's
probably correct, but as always, YMMV.

In the default configuration, the device will show up as a composite
device with a CDC-ACM serial port. By default, Linux will bind the
`cdc_acm` driver to this, which prevents Chrome from connecting to the
interface. To unbind it, go to `/sys/bus/usb/drivers/cdc_acm` and find
the devices listed. They'll look like `1-2:1.0` or similar. Echo these
to the `unbind` file.

```
$ cd /sys/bus/usb/drivers/cdc_acm
$ ls
1-2:1.0  1-2:1.1  bind  module  new_id  remove_id  uevent  unbind
$ sudo sh -c "echo '1-2:1.0' > unbind"
```

The interface should now be available to WebUSB.

Alternatively, you can compile the project with the
`vendor-specific-usb` feature flag, which will use the "Vendor specific"
device class (0xFF), which doesn't bind to the `cdc_acm` driver. But
then it won't show up as a serial port.