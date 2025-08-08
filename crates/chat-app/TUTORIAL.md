# Aranya Mesh Chat Tutorial

This tutorial will walk you through building the "chat app" and
deploying it on the [SpiderOak Demo Board
V2](https://github.com/aranya-project/demo-board-v2).

## What is Aranya and why do I care?

Aranya is an open source library for building secure systems based on decentralized state. 

Our primary application is to provide a Policy Decision Point at the edge for use in applications where depending on a centralized node is infeasible due to degraded or disrupted networks. 

The systems consists of three main technologies. 

  1. A robust network sync protocol for sharing state updates between devices. This protocol is designed to be robust even with high packet loss.
  2. A novel CRDT that has built in defenses against adversarial edits. This provides for automatic resolution of conflicting changes to state.
  3. And a domain specific language and runtime for distributed protocol development.

The tutorial is build on the embedded version of our toolkit which is a technology demonstration. It runs on bare metal on 32bit MCUs such as the esp32. 

The production version can be found at [https://github.com/aranya-project/aranya](https://github.com/aranya-project/aranya) and is targeted at linux environments.

By completing the tutorial and challenges you will gain an understanding on how to use the tool chain to build and flash Aranya to and esp32 as well as lean a few basics about how to build protocols using Aranya.

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
cargo install espup --locked
```

Once it's installed, use it to install the ESP Rust toolchain.

```
espup install
```

In order to compile things with the ESP Rust toolchain, some environment
variables need to be set. `espup install` automatically places a script
in your home directory called `export-esp.sh`.

Source this file to add these environment variables to your current
environment.

```
source $HOME/export-esp.sh
```

If you want this to be done automatically, add `source
$HOME/export-esp.sh` to your shell startup (`.bashrc`, `.zshrc`, etc).

And finally, you will need `espflash`. Install version 3.

```
cargo install espflash@^3 --locked
```

Version 4 works with a beta version of the toolchain and will not work.

You should now be ready to continue.

## Get the app

Change to the directory you would like to work in and clone the Aranya Embedded git repository. 

```
git clone https://github.com/aranya-project/aranya-embedded.git
```

Change directory into the `chat-app` crate in the new aranya-embedded directory.

```
cd aranya-embedded/crates/chat-app
```

And build it.

```
cargo build
```

If that worked, let's continue with configuration and deployment. If it did not work check that you followed the previous steps and ask for help if you're stuck.

## Configuration

Each node in the mesh has to have a unique address. Addresses in our
ad-hoc ESP-NOW network are 16-bit unsigned integers, and 0 is reserved
for broadcast.

Ask the workshop organizers for an address, or pick one and ask if it
has been used. For this example, we'll pick address 42.

Change directory back to the repository root.

```
cd ../..
```

Run `aranya-embedded-config` to create a configuration for the device.

```
cargo run --bin aranya-embedded-config -- -c --ir-address 42 crates/chat-app/params.bin
```

Change directory back to the `chat-app` crate.

```
cd crates/chat-app
```

In order to write to the device, it must be in bootloader mode. Hold
down the main button while plugging it in to connect it in bootloader
mode.

Now use `espflash` to write the configuration to the device.

```
espflash write-bin 0x9000 params.bin
```

## Running

Now it's time to deploy. Plug in your board to your computer using a usb-c cable while holding down the large black button on the board. (You can release the button after the usb cable is plugged into the dev board and computer.)

```
cargo run
```

This should ask you which device you want to connect to. If you don't see a serial device check that it is plugged in. If you can select the device but it will not upload the program you may need to unplug the board and plug it back in while holding down the large button on the board. Once it is plugged in you can release the button and retry the `cargo run` command.

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

## Going further

There are a couple of buttons on the UI that won't do anything yet - A
"rainbow" button and a dropdown and button for setting the ambient LED
color. These won't work as they are intentionally incomplete. It's up to
you to finish the code to make them work.

### Rainbow mode

The first thing you need to do is [complete the `send_rainbow()`
action](config/policy.md#rainbow). In Aranya, an action is the entry
point to creating changes in the system. An action will typically
publish a "command", which is a data definition and the rules for how to
validate that data and update system state. The command published
"effects", which the application receives and responds to as commands
are replicated and processed. This effect is programmed to play a
rainbow color animation on the RGB LED.

### Ambient color

The second thing is to get the ambient LED set working. It's missing
both [its action](config/policy.md#ambient-led-color) as well as [the
code that calls the action](src/application.rs#L178) in response to the
USB serial event.

This command maintains the current color through a "fact", which is a
kind of key-value store that is updated by command policy. The policy
for `SetAmbientColor` queries the current value, then updates it to the
new value.

### Reference material

For a more detailed specification of the policy language, see our
[policy language
specification](https://github.com/aranya-project/aranya-docs/blob/main/docs/policy-v1.md).