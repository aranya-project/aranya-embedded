# Aranya Embedded

**Hello DEF CON. If you are here for the Aranya workshop start at [the tutorial](crates/chat-app/TUTORIAL.md).**

This repository holds the various crates that make up the Aranya Embedded
project, which adapts the [main Aranya
project](https://github.com/aranya-project/aranya) to embedded hardware. Most
notably, it contains [a demo that communicates LED state between devices via IR
or ESP-NOW messaging](demo-esp32-s3/README.md).

Aranya Embedded, like its parent project, is licensed under the
[AGPL](LICENSE.md).

## Supported Platforms and Status

Currently, Aranya Embedded supports only the ESP32-S3, though it has been tested
on multiple boards and has some facility for peripheral configuration.

Aranya Embedded is in its early stages and is not yet ready for production use.

## Crates

- [`demo-esp32-s3`](crates/demo-esp32-s3/) - the main demo.
- [`aranya-embedded-config`](crates/aranya-embedded-config/) - a
  configuration tool for the demo's runtime parameters using `parameter-store`.
- [`parameter-store`](crates/parameter-store/) - A parameter
  serialization library that works with both `no_std` storage and `std` file
  I/O.
- [`esp-irda-transceiver`](crates/esp-irda-transceiver/) - hardware
  library for using an IR transceiver via an ESP UART.
- [`esp-rmt-neopixel`](crates/esp-rmt-neopixel/) - hardware library
  for driving a WS2812-style LED via the ESP32's remote control (RMT)
  hardware.
- [`hardware-tester`](crates/hardware-tester/) - a simple program for
  testing hardware on various boards.
- [`aranya-embedded-storage-dumper`](crates/aranya-embedded-storage-dumper/) -
  A tool for creating graphviz dot graphs from an extracted internal-storage
  linear storage partition.

All of these crates are organized into a workspace, but compiling esp32
projects from the root workspace will not work due esp32 projects requiring a
different toolchain (see [Setup](#setup) below). Please change directory into
the individual crates and read their `README`s before attempting any builds.

## Setup

Aranya and Aranya Embedded require a [rust development
environment](https://www.rust-lang.org/), but compiling esp32 projects requires
some extra setup beyond that. Please follow the instructions in [The Rust on
ESP Book](https://docs.espressif.com/projects/rust/book) for installing the toolchain for both RISC-V and Xtensa targets.

## Contributing

Find information on contributing to the Aranya project in
[`CONTRIBUTING.md`](https://github.com/aranya-project/.github/blob/main/CONTRIBUTING.md).

## Maintainers

This repository is maintained by software engineers employed at
[SpiderOak](https://spideroak.com/).
