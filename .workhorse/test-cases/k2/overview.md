# Remembering recent devices

Scenarios for the web client's Recent list, reloads, and the fragment.

## Remembering

- [x] A device is listed under Recent, by hostname and the last group of its rendering, once a channel to it has opened (verifies spec: WEB)
- [x] A code read but never connected with is not listed (verifies spec: WEB)
- [x] A device that reported no hostname is listed by the last group alone (verifies spec: WEB)
- [x] At most three devices are listed, the most recently opened first, and reconnecting to one moves it to the top (verifies spec: WEB)
- [x] Choosing a remembered device holds its code on "QR code read" with its hostname, and "Find the device" goes on as for any code (verifies spec: WEB)
- [x] The list survives a reload (verifies spec: WEB)
- [x] A new tab starts with no list (verifies spec: WEB)
- [x] A remembered code that no longer reads is dropped from the list (verifies spec: WEB)

## Reloading

- [x] A reload while connected comes back on "QR code read" for that device (verifies spec: WEB)
- [x] A reload while a device is restarting comes back on "QR code read" for that device (verifies spec: WEB)
- [x] A reload after disconnecting comes back holding no code (verifies spec: WEB)
- [x] A reload whose device's code no longer reads comes back holding no code (verifies spec: WEB)

## The fragment

- [x] The fragment is removed from the address once read (verifies spec: WEB)
- [x] An unparseable fragment is removed too, and reported (verifies spec: WEB)

## Manual

- [ ] On Chrome for Android, an accidental pull-to-refresh while connected lands on "QR code read" for the device, and one pick in the chooser reconnects
- [ ] On Chrome for Android, closing the tab and opening the app again shows no Recent list
