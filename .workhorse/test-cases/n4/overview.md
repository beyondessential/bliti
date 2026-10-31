# Fake device in dev mode

## Page that reaches a device

- [x] Out of dev mode, the scan step offers no fake device (verifies spec: WEB)
- [x] In dev mode, the scan step offers the fake device. Choosing it goes to finding with the fake held, then connecting without a chooser, then the Status screen (verifies spec: WEB)
- [x] Finding the fake device offers no "save code" (verifies spec: WEB)
- [x] The connected fake device shows a hostname and software identity that say it is fake, and `FAKE` in place of the suffix (verifies spec: WEB)
- [x] Every screen reachable from Status shows content and none is left loading: Readings, Power, Network and its sub-screens, Tailscale, Tamanu (verifies spec: WEB)
- [x] A Tamanu or Tailscale act is accepted, and the state shown doesn't change (verifies spec: WEB)
- [x] A proposed network configuration is answered as saved. Reopening the network screen shows the canned configuration again (verifies spec: WEB)
- [x] Restart and reboot show the act under way, then return to Status. Power-off shows the act under way, then offers finding the device again (verifies spec: WEB)
- [x] The Activity record shows the fake device's traffic (verifies spec: WEB)
- [x] After disconnecting, the fake device is listed among remembered devices in dev mode and hidden out of dev mode (verifies spec: WEB)
- [x] Reloading while connected to the fake device comes back holding it, ready to connect (verifies spec: WEB)
- [x] Turning dev mode off while connected leaves the fake device connected. Disconnecting then lets it go, back at the scan step (verifies spec: WEB)
- [x] Out of dev mode, typing the fake device's code reads as an unreadable code. In dev mode, it reads to the fake (verifies spec: WEB)
- [x] A fake code held off a channel is let go of when dev mode is turned off (verifies spec: WEB)
- [x] Reloading while on the fake device with dev mode off comes back holding it, and disconnecting then lets it go (verifies spec: WEB)
- [x] Hiding and showing the page while connected to the fake device resumes the feed with no second hello (verifies spec: WEB)
- [x] Turning off the first candidate gives the default route to the next one enabled (verifies spec: WEB)
- [x] The fake device says `accepted` before `going-away` in the Activity record (verifies spec: WEB)
- [x] In a browser without Web Bluetooth, the unsupported screen offers the fake device in dev mode. After disconnecting, the unsupported screen shows again (verifies spec: WEB)
- [x] The fake device works in a production build (`vite build`, not the test mode)

## Provisioning client

- [x] Out of dev mode, finding offers no fake device. In dev mode, it does (verifies spec: PWEB)
- [x] Choosing the fake device connects without the chooser and names it `FAKE` (verifies spec: PWEB)
- [x] The fake device is idle with every step listed and ticked, and no medium is refused (verifies spec: PWEB)
- [x] Starting a run finishes at once with every chosen step passed, unchosen steps skipped, and no record shown (verifies spec: PWEB)
- [x] The fake run adds no label to the list, and Done doesn't say a label was added (verifies spec: PWEB, PSHT)
- [x] Choosing steps again after finishing runs again (verifies spec: PWEB)
- [x] The unsupported screen offers the fake device in dev mode (verifies spec: PWEB)
- [x] The fake device is held back while the chooser is open, as Find Iti is (verifies spec: PWEB)
- [x] Letting the fake device go partway through a run leaves nothing more from it in the Activity record, and it can be reached again (verifies spec: PWEB)
- [x] Turning dev mode off while connected leaves the fake device connected (verifies spec: PWEB)

## Shared

- [x] The page's client sends each member to whichever client the channel is with, and fails as the page loads where the fake lacks one
- [ ] On the find step, Scan a different code while the chooser is open leaves the fake device's button held back until the attempt ends (verifies spec: WEB)
- [ ] Turning dev mode on or off during a camera scan changes whether the fake device's code reads, without restarting the scan (verifies spec: WEB)
