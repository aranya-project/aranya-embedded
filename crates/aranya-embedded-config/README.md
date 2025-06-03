# aranya-embedded-config

This is a tool that creates and modifies binary parameter files that can be
written to partitions on embedded devices. It configures aranya parameters like
addresses and peers.

# Usage

```
Usage: aranya-embedded-config [OPTIONS] <FILE>

Arguments:
  <FILE>

Options:
      --ir-address <IR_ADDRESS>  Set the IR interface address
      --ir-peers <IR_PEERS>      Set the IR peer addresses
      --color <COLOR>            Set the device's color r,g,b
  -c, --create
  -v, --verbose
  -h, --help                     Print help
```

Usually you will use it to create a new parameter file for a device:

```
$ aranya-embedded-config -c --ir-address 3 --ir-peers 0,1,2 --color 255,0,0 params-red.bin
```

But it can also print out the parameters from a binary file:

```
$ aranya-embedded-config params-R.bin
Graph ID: None
IR address: 3
IR peer addresses: [0, 1, 2]
Color: RgbU8 { red: 255, green: 0, blue: 0 }
```

Then it can be flashed to the "nvs" partition of the device:

```
$ espflash write-bin 0x9000 params-red.bin
```

(assuming your parameters partition starts at 0x9000)
