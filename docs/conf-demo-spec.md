ESP Demo
This document describes the ESP32 demo we would like to complete by the end of February 2025.

Goal
We would like a demo that shows how Aranya is: - Easy to use - Easy to integrate - Robust to network partitions - Robust to packet loss

Approach
We would like the demo to consist of: - A phone or laptop as the admin station. (ATAK would be a stretch goal.) - Four or more battery powered mricocontroller boards. - A short code listing showing the Aranya call points.

In the demo we want to show the easy onboarding and off-boarding of devices and the assignment roles and permissions using the admin station.

On the mricocontrollers, we want each to have an assigned color, a button, and and multi color LED. When the button is pressed the others devices should light up in the assigned color. Communication between devices should be via IR. Using IR will allow us to easy partition and degrade the network.

The code listing need only show the calls to the Aranya APIs and we should attempt to fit it on a single page emphasizing how easy it is to integrate Aranya in to a system.