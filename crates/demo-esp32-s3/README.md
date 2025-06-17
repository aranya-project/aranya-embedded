# Demo

This is a demo that uses Aranya to propagate colors between embedded devices
using various networking technologies.

## Building and Running

If you have a rust esp32 environment properly set up, you should be able to
`cargo run` and it will automatically build and flash to a connected device. By
default it builds for the Adafruit ESP32-S3 Feather with the [SpiderOak Demo V1
carrier board](https://github.com/aranya-project/ir-demo-board) using IR
networking.

You will have to configure the network address and color for each node using
[`aranya-embedded-config`](../aranya-embedded-config/), then write the parameter
data to the `nvs` partition. For example:

```
(in the aranya-embedded workspace root)
$ cargo run --bin aranya-embedded-config -- -c --ir-address 0 --ir-peers 1,2,3 --color 255,0,0 params-red.bin
$ espflash write-bin 0x9000 params-red.bin
```
0x9000 is the flash address of the `nvs` partition specified by
`partitions.csv`.

On first boot, the demo will configure the Aranya graph storage and write the
Graph ID to the parameter partition. To reset the storage, you will have to
erase both the storage and reset the graph ID in the parameter storage to `None`
by rewriting the `nvs` partition with its initially configured state.

## Operation

Each node will start up by setting the neopixel to its configured color.
Pressing the BOOT button adds a command to the graph with that node's configured
color. Periodic syncing between nodes will propagate that command to other
nodes. The effects of processing that command cause other nodes' neopixels to
change to that color, showing the graph propagation as it happens.

## Features

The Demo supports several different networking and storage approaches (not all
of which are currently working) and several boards (which support varying
peripherals). It should work minimally on any ESP32-S3 with 4MB flash and 2MB
PSRAM that has a boot button and a "neopixel" WS2812-like LED.

### Storage

- `storage-internal` - stores data on the internal flash storage in a partition
  named `graph`
- `storage-sd` - stores data in a FAT filesystem on an SD card (currently
  untested and probably non-functional)

### Networking

- `net-wifi` - Talks to other nodes via IPv4 networking over WiFi (currently
  non-functional and likely to be removed)
- `net-irda` - Talks to other nodes via homebrewed broadcast networking over
  IrDA transceivers
- `net-esp-now` - Talks to other nodes via broadcast ESP-NOW

### Boards

| Board         | Description               | Neopixel | IR | SD | Notes |
|---------------|---------------------------|----------|----|----|-------|
| `feather-s3` | Adafruit ESP32-S3 Feather | Y        | N  | N  | The [IR Demo V2 carrier board](https://github.com/aranya-project/ir-demo-board) can provide IR and SD |
| `qtpy-s3`     | Adafruit QT Py ESP32-S3   | Y        | N  | N  | |
