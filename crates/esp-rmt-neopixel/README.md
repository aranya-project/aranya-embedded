# esp-rmt-neopixel

This library provides support for WS2812-like "Neopixel" RGB LED modules. It
uses the ESP32's remote control (RMT) hardware to sequence the data stream,
leaving the CPU free to do other tasks. It currently only supports one module.
