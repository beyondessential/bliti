# Device control

Scenarios for restarting, rebooting and powering off a device from the client. The automated ones run in `cargo test -p bliti -p bliti-core -p bliti-web` and in the browser harness; the rest need the prototype device and a phone.

## The wire

- [x] `control`, `acts`, `act`, `accepted`, `refused` and `going-away` round trip, and `act` carries `ACT` critical (verifies spec: CTL)
- [x] An `acts` listing an act this build does not know is still read (verifies spec: CTL)
- [x] The oracle generates every control message, and the ledger records `act`'s critical selector
- [x] The wasm client describes the control messages to the application with the wire's member names

## The device

- [x] A control stream is answered with the acts the device lists (verifies spec: CTL)
- [x] An act not listed is refused with a reason (verifies spec: CTL)
- [x] Once one act is accepted, every later one is refused, saying what the device is doing (verifies spec: CTL)
- [x] An accepted act is announced as `going-away` on the open feed of every session, the asking one included (verifies spec: CTL)
- [x] No session ends until every open feed has announced the act, and no connection is dropped until every session has closed (verifies spec: CTL)
- [x] The act is carried out only after the connections are dropped (verifies spec: CTL)
- [x] A feed opened while the device is going away is told at once
- [x] An act that fails to carry out leaves the device taking acts again (verifies spec: CTL)
- [ ] Under `bliti.service`, as root, the device lists restart, reboot and power off
- [ ] Run by hand as root, outside a service, the device lists reboot and power off, and not restart
- [ ] Run as another user, the device lists no act
- [ ] The daemon log names the act, whether it was accepted, and the client's name and version (verifies spec: CTL)
- [ ] Restart stops and starts `bliti.service`, and leaves the rest of the system running
- [ ] Reboot stops every service in turn and brings the device back
- [ ] Power off leaves the device off
- [ ] The BLE link drops as the device goes away, rather than lingering until a timeout

## The application

- [x] The device view is titled Info, with Control as its primary action and Disconnect beside it (verifies spec: VIEW)
- [x] Network settings sit on the Control screen, and Back returns from Network to Control and from Control to Info (verifies spec: CSCR)
- [x] The control stream opens with the Control screen and closes on leaving it (verifies spec: CSCR)
- [x] Only the acts listed and known are offered, in the order restart, reboot, power off (verifies spec: CSCR)
- [x] The power section is left out until the device lists its acts, and where it lists none (verifies spec: CSCR)
- [x] Every act is confirmed each time, and nothing is asked for until it is (verifies spec: CSCR)
- [x] The confirmation names the act, and a power off's says it stays off (verifies spec: CSCR)
- [x] The confirmation says unsaved network settings will be lost only while the device reports them provisional (verifies spec: CSCR)
- [x] The confirmation says network edits not applied will be lost, and the kept session's bar shows on the Control screen (verifies spec: CSCR, NSCR)
- [x] A refusal is rendered as the device wrote it (verifies spec: CSCR)
- [x] An act accepted keeps the Info title and the device's name, shows the act under way in place of the tiles, and offers only Disconnect (verifies spec: WEB)
- [x] An act announced on the feed is shown the same way, whoever asked for it (verifies spec: WEB)
- [x] An act this build does not know is left to end the channel as anything else does
- [x] After a reboot the application reaches the device again on its own and shows the device view (verifies spec: WEB)
- [x] A device that does not come back is offered to be found again (verifies spec: WEB)
- [x] A device reachable only through the chooser is offered to be found again (verifies spec: WEB)
- [x] Disconnecting stops the attempts to reach the device (verifies spec: WEB)
- [x] A power off shows "Shutting down…" for a moment after the channel closes, then the ordinary code screen, and nothing reconnects (verifies spec: WEB)
- [ ] On a phone, a reboot of the prototype ends with the device view back, without the chooser (verifies spec: WEB)
- [ ] On a phone, a restart of bliti ends with the device view back, without the chooser (verifies spec: WEB)
- [ ] A second phone watching the device sees the act under way and reconnects too (verifies spec: WEB)
- [ ] A phone with the page hidden during the act sees an ordinary disconnect when it comes back
