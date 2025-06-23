# Hardware Tester

This is a basic tool for testing board hardware - buttons, RGB LED, Qwiic, SD,
IR etc. It presents a menu over USB serial which you can use to test various
aspects of the hardware.

## Running

If you have a rust esp32 environment properly set up, you should be able to
`cargo run --features <board>` and it will automatically flash and connect to
USB serial. See below for the selection of boards.

## Board selection

Boards are chosen by feature flag. Currently the boards supported are:

- `feather-s3` - An Adafruit ESP32-S3 Feather with the [SpiderOak Demo V1 carrier
  board](https://github.com/aranya-project/ir-demo-board).
- `spideroak-demo-v2` - A [custom
  board](https://github.com/aranya-project/demo-board-v2) with an
  ESP32-S3-WROOM-1-N4R2 module and the same complement of hardware found on the
  S3 with the carrier board.

No board is selected by default - you will have to specify one of these via the
`--features` flag.
