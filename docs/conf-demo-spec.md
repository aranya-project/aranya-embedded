# ESP (Conference) Demo

This document describes the ESP32 demo we would like to complete by the end of February 2025.

## Goal

We would like a demo that shows how Aranya is: 

- Easy to use 
- Easy to integrate 
- Robust to network partitions 
- Robust to packet loss

When people have finished interacting with the demo, we want them to have an intuitive understanding of how Aranya can secure communications in tactical edge and space environments. They should be able to interact with the demo by:

- Pressing any button and watch the lights change as the command is propagated.
- Block or partition some of the devices to simulate packet loss or broken links.
- Arrange the devices in any way they want to view how the commands move throughout the system.
- Partition the devices and run two different "networks" at once, and watch them reconcile after being reconnected.
- Turn off or destroy one of the devices to show resilience.

## Approach

We would like the demo to consist of: 

- A phone or laptop as the admin station. ([ATAK](https://www.civtak.org/) would be a stretch goal.) 
- Four or more battery powered microcontroller boards. 
- A short code listing showing the Aranya call points.

In the demo we want to show the easy onboarding and off-boarding of devices and the assignment roles and permissions using the admin station.

On the microcontrollers, we want each to have an assigned color, a button, and and multi color LED. When the button is pressed the others devices should light up in the assigned color. Communication between devices should be via IR. Using IR will allow us to easy partition and degrade the network. Each microcontroller will be labeled with a specific color to indicate what color it will change the network to when pressed.

The code listing need only show the calls to the Aranya APIs and we should attempt to fit it on a single page emphasizing how easy it is to integrate Aranya in to a system.

### Functionality we want to demonstrate

- Easy onboarding of new devices
- Role management
- Graph message syncing

### BOM

This is the Bill of Materials (BOM) for the version of this demo that uses on-board LEDs. The buttons might be optional if we choose a board with an on-board programmable button like the Cytron Maker Feather AIOT S3.

| Item | Qty | Details |
| ---- | --- | ------- |
| Laptop | 1 | Mac or Linux |
| Microcontroller | 4 | See options below |
| Button | 4 | - |
| IrDA hardware | 4 | Laptop can use serial/bluetooth/wifi for talking to the microcontrollers |
| Battery | 4 | 500+ mAh |


#### Board Options

| Board | Memory | Details |
| ----- | ------ | ------- |
| [Adafruit ESP32-S3 Feather](https://www.adafruit.com/product/5477) | 2MB | LED on board, lipo power circuit |
| [Cytron Maker Feather AIOT S3](https://www.cytron.io/p-v-maker-feather-aiot-s3-simplifying-aiot-with-esp32) | 8MB | LED on board, Single-cell LiPo connector, piezo buzzer, programmable button | 


## TODOs

- Policy
    - LED Command
    - Onboarding/Management commands
    - Role commands
- Aranya Integration on microcontroller
- Device Prototyping and manufacturing
    - Circuit diagram
    - Parts list (finished BOM)
    - Device flashing procedure
    - Device Aranya integration
    - Device assembley procedure
    - Peripheral integration (IrDA)
- Management Software
    - Standalone application for managing devices
    - Can initiate onboarding with a device
    - Can send and receive commands from the microcontrollers
    - Ability to reflash devices for troubleshooting