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

On the microcontrollers, we want each to have an assigned color, a button, and and multi color LED. When the button is pressed the others devices should light up in the assigned color. Communication between devices should be via IR. Using IR will allow us to easy partition and degrade the network.

The code listing need only show the calls to the Aranya APIs and we should attempt to fit it on a single page emphasizing how easy it is to integrate Aranya in to a system.

### Functionality we want to demonstrate

- Easy onboarding of new devices
- Role management
- Graph message syncing

### BOM

| Item | Qty | Details |
| ---- | --- | ------- |
| Laptop | 1 | Mac or Linux |
| Microcontroller | 4 | - |
| RGB LED | 4 | - |
| 100 Ohm Resistor | 4 | For LED |
| Button | 4 | - |
| 3d Printed Housing | 4 | Should hold the microcontroller and have easily accessible and understandable buttons. |
| IrDA hardware | 5 | Need an IrDA module for the laptop |


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
- Management Software
    - Standalone application for managing devices
    - Can initiate onboarding with a device
    - Can send and receive commands from the microcontrollers
    - Ability to reflash devices for troubleshooting