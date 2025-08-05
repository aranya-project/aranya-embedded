# Aranya Mesh Chat Tutorial

This tutorial will walk you through building the "chat app" and
deploying it on the [SpiderOak Demo Board
V2](https://github.com/aranya-project/demo-board-v2).

## Prerequisites

This tutorial works with either macOS or Linux or something that
sufficiently pretends to be Linux, like WSL or FreeBSD Linux Binary
Compatibility.

While this might be buildable on Windows, we haven't tried it. Let us
know if it works!

## Mise en Place

Let's start by getting an ESP32 Rust development environment set up.
First you'll want to get Rust itself via [rustup](https://rustup.rs/).

Once you have Rust installed, install `espup`. `espup` is a toolchain
manager for the ESP32 rust toolchain, and it automatically installs the
supporting compilers and tools you need.

```
$ cargo install espup --locked
```

Once it's installed, use it to install the ESP Rust toolchain.

```
$ espup install
```

In order to compile things with the ESP Rust toolchain, some environment
variables need to be set. `espup install` automatically places a script
in your home directory called `export-esp.sh`.

Source this file to add these environment variables to your current
environment.

```
$ source $HOME/export-esp.sh
```

If you want this to be done automatically, add `source
$HOME/export-esp.sh` to your shell startup (`.bashrc`, `.zshrc`, etc).

And finally, you will need `espflash`. Install version 3.

```
$ cargo install espflash@^3 --locked
```

Version 4 works with a beta version of the toolchain and will not work.

You should now be ready to continue.

## Get the app

Clone the Aranya Embedded git repository and enter the root of the
repository.

```
~/src$ git clone https://github.com/aranya-project/aranya-embedded.git
~/src$ cd aranya-embedded
~/src/aranya-embedded$
```

Change directory into the `chat-app` crate.

```
~/src/aranya-embedded$ cd crates/chat-app
~/src/aranya-embedded/crates/chat-app$
```

And build it.

```
~/src/aranya-embedded/crates/chat-app$ cargo build
```

If that worked, let's continue with configuration and deployment.

## Configuration

Each node in the mesh has to have a unique address. Addresses in our
ad-hoc ESP-NOW network are 16-bit unsigned integers, and 0 is reserved
for broadcast.

Ask the workshop organizers for an address, or pick one and ask if it's
been used. For this example, we'll pick address 42.

Change directory back to the repository root.

```
~/src/aranya-embedded/crates/chat-app$ cd ../..
~/src/aranya-embedded$ cd ../..
```

Run `aranya-embedded-config` to create a configuration for the device.

```
~/src/aranya-embedded$ cargo run --bin aranya-embedded-config -- -c --ir-address 42 crates/chat-app/params.bin
```

Change directory back to the `chat-app` crate.

```
~/src/aranya-embedded$ cd crates/chat-app
~/src/aranya-embedded/crates/chat-app$
```

In order to write to the device, it must be in bootloader mode. Hold
down the main button while plugging it in to connect it in bootloader
mode.

Now use `espflash` to write the configuration to the device.

```
~/src/aranya-embedded/crates/chat-app$ espflash write-bin 0x9000 params.bin
```

## Running

Now it's time to deploy.

```
~/src/aranya-embedded/crates/chat-app$ cargo run
```

This should write the program to the device. You will have to reset the
device to get it working. The easiest way to do this is to unplug it and
replug it.

If it's working, you should get a brief orange flash from the main LED
and see some blinking on the smaller LEDs next to the button.

## Logging On

Open Chrome (or anything Chrome-like like Edge or Opera) to
[https://chip-so.github.io/chat/](https://chip-so.github.io/chat/) or
open `web/client.html` to open the chat interface. Click "Connect". It
should list the "Demo Board V2". Select it and click "Connect".

You did it! You should now be able to send and receive messages.
