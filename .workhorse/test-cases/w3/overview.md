# Simple network screens

What verifies the Network, Cable, Join wifi and Share hotspot screens.

## The simple states

- [x] Cable, cable and hotspot, and wifi are told apart; a configuration never saved is the cable (verifies spec: NSET)
- [x] Two wireless candidates, both uplinks on, wifi beside the hotspot, wired candidates partly on, and no wired candidate are outside the simple states (verifies spec: NSET)
- [x] Every move from every simple state lands where NSET's table has it, and stays simple (verifies spec: NSET)
- [x] A move turns things off by `enabled` false, keeping their settings, and leaves every other member as it is (verifies spec: NSET)

## The session

- [x] Change and Set up wait on the network row until the first `state`, then open the Network screen (verifies spec: NSET, STAT)
- [x] A busy session is closed, the operator stays on the Status screen, and the row says someone else is making changes until asked again or left (verifies spec: NSET)
- [x] Returning to the Status screen by Back or the browser's way back closes the session; moving between simple screens keeps it (verifies spec: NSET)
- [x] Opening the network settings from a simple screen closes the session first, and coming back opens it again
- [x] A session ending with the channel open says so and opens again on request (verifies spec: NSET)
- [x] A reconnect under a simple screen opens the session again, keeping what was typed; a busy one returns to the Status screen with the note (verifies spec: NSET, WEB)

## The Network screen

- [x] The three ways in order, each on one marked, with how each stands from the facts (verifies spec: NSET)
- [x] Outside the simple states, only the way to the network settings is offered (verifies spec: NSET)

## The Cable screen

- [x] Waits for a cable and proposes nothing before one is plugged in (verifies spec: NSET)
- [x] Uses the cable once one is in, wifi off and the hotspot as it is, with no Cancel, and returns to the Status screen once applied (verifies spec: NSET)
- [x] A failure says the network gave no connection, with the device's reason (verifies spec: NSET)
- [x] With the cable on, proposes nothing; says it is connected, or not connecting, and returns once a cable starts working (verifies spec: NSET)

## Join wifi

- [x] Scans on opening, lists networks once each by SSID with signal as a word, strongest first, and says where none were heard (verifies spec: WIFI)
- [x] Marks the network joined, and shows one the device cannot join as not pickable (verifies spec: WIFI)
- [x] Says joining stops using the cable and turns the hotspot off (verifies spec: WIFI)
- [x] An enterprise network goes to the network settings or back to the list (verifies spec: WIFI)
- [x] A network named by the operator is joined hidden, by the first of `psk-sae`, `psk` and `sae` offered (verifies spec: WIFI)
- [x] The password shows and hides, is filled in for the network wifi names, and is held to 8 to 63 characters (verifies spec: WIFI)
- [x] Connecting replaces wifi in place with the members WIFI lists, and returns to the Status screen once applied (verifies spec: WIFI)
- [x] A refused password returns to the password step with the field marked (verifies spec: WIFI)
- [x] Out of range waits with no Try again, and returns once the candidate is the default route (verifies spec: WIFI)
- [x] Other failures and refusals show the device's reason, with another network and another password offered (verifies spec: WIFI, NSET)
- [x] Leaving with a password typed and not proposed asks first, by Back and the browser's way back (verifies spec: NSET, WIFI)

## Share hotspot

- [x] The switch turns the hotspot on or off at once, held while applying, with the devices joined beneath it (verifies spec: SHARE)
- [x] Turning it on from wifi says it leaves the network for the cable, or goes offline, and proposes the cable on (verifies spec: SHARE, NSET)
- [x] A failure shows the switch as the device holds it, with the reason (verifies spec: SHARE)
- [x] Name shown, password hidden until shown, and the QR code drawn (verifies spec: SHARE)
- [x] The password changes in place with the rejoin line, the length check, Cancel, and the leave guard (verifies spec: SHARE)
- [x] A hotspot not yet set up is named after the hostname or the Status screen's fallback, with a passphrase drawn once per session, proposed on the first switch on or save (verifies spec: SHARE)

## On the prototype

- [x] Wifi to cable by plugging one in
- [ ] Cable and hotspot together, a phone joining by the QR code
- [ ] Joining wifi from the hotspot, and a wrong password
- [ ] The busy note from a second phone
