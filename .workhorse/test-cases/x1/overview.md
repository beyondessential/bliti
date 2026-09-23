# Network configuration module with wifi and hotspot

Scenarios that verify the module, for manual checking and as the brief for automated tests.

## Hardware and radio

- [x] The BLE session survives a wireless client connection change, a switch into AP mode, and AP+STA coming up together, on real target hardware. Verified on a Raspberry Pi 5 (Cypress CYW43455): the LE session carried GATT reads throughout, the controller logged no disconnect, and `brcmfmac` loaded firmware once at boot. Stays on the list because a firmware or hardware change could take it back.
- [ ] A device reports its radio's real capabilities: AP+STA concurrency, the shared-channel constraint, and the WPS methods offered. Verifies spec: NET
- [x] On shared-channel hardware the hotspot's band, channel and width are absent from capabilities, and a document carrying them is invalid. Verifies spec: HOT
- [ ] On shared-channel hardware the hotspot follows the wireless client's channel once one associates, and NFO reports the channel both are on. Verifies spec: HOT
- [ ] On hardware with independent channels the three settings are offered, and a set channel is the one the hotspot comes up on. Verifies spec: HOT
- [x] A device whose radio runs only one of AP and STA at a time rejects a document carrying both, naming the conflict. Verifies spec: HOT

## The configuration session

- [x] Opening a session returns the configuration in force together with the device's capabilities. Verifies spec: CFG
- [x] A second client opening a session while one is open is told the device is busy. Verifies spec: CFG
- [x] Reading the configuration and writing it back unmodified changes nothing. Verifies spec: NET
- [x] A proposal is applied to the running system and written nowhere. Verifies spec: CFG
- [x] Confirming a proposal makes it the recorded configuration. Verifies spec: CFG
- [x] Discard during verification aborts the attempt and leaves the recorded configuration in force. Verifies spec: CFG
- [x] Discard after a proposal is applied reverts to the recorded configuration. Verifies spec: CFG
- [x] A session abandoned without confirming leaves the recorded configuration in force, by stream close, channel drop, and device power-off alike. Verifies spec: CFG
- [x] A proposal is never timed out while its session is open. Verifies spec: CFG
- [x] A device retains nothing of a proposal after reverting. Verifies spec: CFG
- [x] A failure carries the part of the document at fault, a reason in the device's words, and the verification stage reached. Verifies spec: CFG
- [ ] A configuration that cannot work, by wrong passphrase and by absent SSID, is reported with the stage it failed at. Verifies spec: CFG
- [x] A session the daemon drops when its client unsubscribes ends its configuration session, so the next client is not told the device is busy. Verifies spec: CFG
- [ ] A client that walks away with a proposal applied and its feed closed leaves the device back on its recorded configuration, on real hardware. Verifies spec: CFG
- [x] An act on an adapter or with a method the device did not offer is invalid at the act's own path. Verifies spec: NET, CFG
- [x] `applied` carries capabilities only where applying changed them, and the next `state` carries them where a revert changed them back. Verifies spec: CFG
- [x] `state` follows the opening configuration, each change, the proposal once applied, and the recorded configuration after a revert, and is held while a proposal is verified. Verifies spec: CFG
- [x] Joining by WPS PIN sends the PIN before the joined result, while the join is under way. Verifies spec: CFG
- [x] A proposal interrupted by a newer one is answered `invalid` at `$`, and one refused after it superseded another restores the recorded configuration. Verifies spec: CFG
- [x] An unconfigured device holds one dynamic candidate per physical wired interface and no hotspot. Verifies spec: CFG
- [ ] An unconfigured device plugged into a network with DHCP is reachable on it, on real hardware. Verifies spec: CFG
- [ ] A power cut with a proposal applied brings the device back on its recorded configuration, with nothing of the proposal brought up at boot, on real hardware. Verifies spec: CFG

## Attachment and selection

- [ ] A device verifies a candidate through carrier, association, addressing and the gateway answering, in that order. Verifies spec: LINK
- [ ] A candidate whose network associates but hands out no address fails at the addressing stage. Verifies spec: LINK
- [ ] A candidate holding a lease on a network that does not route fails at the gateway stage. Verifies spec: LINK
- [ ] A candidate on a network with no route beyond the gateway is established, not failed. Verifies spec: LINK
- [ ] A device holding two static candidates on different subnets picks the right one at each site, unattended and with no client connected. Verifies spec: LINK
- [x] A static candidate carrying no gateway is rejected as invalid. Verifies spec: LINK
- [x] A second dynamic candidate on an interface that already has one is rejected as invalid. Verifies spec: LINK
- [ ] A wired and a wireless candidate up at once leave the device reachable on both, with the default route following the ordering. Verifies spec: LINK
- [ ] The default route moves when a candidate above the one in force becomes available. Verifies spec: LINK
- [ ] A site that changes around a device whose cable never moved is noticed, and the device selects again. Verifies spec: LINK
- [ ] A settled device does no polling to detect any of the three selection events. Verifies spec: LINK
- [ ] Configured resolvers are queried before those a link supplies, and supplied ones are queried where a candidate names none. Verifies spec: LINK
- [x] The selector, unattended: statics on one port told apart at two sites, a lease with no route failing at `gateway`, association without an address failing at `addressing`, the default route following the order and moving back, and a site changing around a cable that never moved. Verifies spec: LINK
- [x] A pinned wireless candidate stays on its radio, and an unpinned one takes a free radio hearing it best. Verifies spec: LINK
- [ ] A site's own names reach the site's resolvers while the candidate names a public resolver of its own, on real hardware. Verifies spec: LINK

## Wireless

- [ ] A device joins WPA2-PSK, WPA3-SAE, the transitional mode, and an 802.1X enterprise network. Verifies spec: WLAN
- [ ] A device joins by WPS push-button and by WPS PIN. Verifies spec: WLAN
- [ ] A device does not join a network that cannot authenticate its access point to it, and says why in terms an operator can act on. Verifies spec: WLAN
- [ ] An access point advertising both an unauthenticated network and an authenticated one is joined only on the authenticated one. Verifies spec: WLAN
- [ ] A device's own hotspot offers no WPS to its clients. Verifies spec: WLAN

## Hotspot

- [ ] A device with no hotspot in its configuration runs none. Verifies spec: HOT
- [ ] A hotspot comes up on the configured SSID and passphrase, with no default supplied for either. Verifies spec: HOT
- [ ] Upstream sharing is on where unset, and off where the configuration turns it off. Verifies spec: HOT
- [ ] Client isolation is on where unset, and clients cannot reach each other. Verifies spec: HOT
- [ ] Clients can reach each other where isolation is turned off. Verifies spec: HOT
- [ ] An unset DHCP range gives the same addresses on every device, and a set one overrides it. Verifies spec: HOT
- [ ] A hotspot works with an upstream present and with none. Verifies spec: HOT
- [x] An unpinned hotspot takes a radio carrying no wireless candidate, stays put while it has no better one, and follows its client's channel on a shared-channel radio. Verifies spec: HOT
- [x] Capabilities offer only hotspot channels the device can start an access point on and renders. Verifies spec: NET, HOT

## The document

- [ ] A wireless network absent from a document is forgotten by the device. Verifies spec: NET
- [ ] A setting absent from a document leaves the device supplying its own behaviour, rather than leaving the thing unconfigured. Verifies spec: NET
- [x] A document asking for anything outside the device's stated capabilities is invalid, and no part of it is applied. Verifies spec: NET
- [ ] An unset regulatory domain restricts the radio to what every domain permits, and a set one opens the local channel set. Verifies spec: NET
- [ ] The configuration a device reports carries its secrets in full. Verifies spec: NET

## The screen

- [x] Editing puts nothing on the wire: a device watched through a session sees no proposal until apply is pressed. Verifies spec: NSCR
- [x] A half-typed gateway is never proposed. Verifies spec: NSCR
- [x] Reset during editing returns the fields to the configuration in force. Verifies spec: NSCR
- [x] Fields are not editable while a proposal is being verified. Verifies spec: NSCR
- [x] After a failure the fields hold what was proposed, not what the device reverted to, and the field named by the failure is marked. Verifies spec: NSCR
- [x] The verification stages show which passed and which failed. Verifies spec: NSCR
- [x] The device's reason is rendered as the device wrote it. Verifies spec: NSCR
- [x] A setting the device did not report supporting is not offered, and the screen says why it is absent. Verifies spec: NSCR
- [x] Each candidate's state is shown, and an unavailable one is described by what the device observed. Verifies spec: NSCR
- [x] Whether the running configuration is durable stays in view while the operator scrolls. Verifies spec: NSCR
- [x] The vocabulary of the wire does not appear on screen. Verifies spec: NSCR
- [x] A wireless network whose channel changes replaces its tile rather than adding a second. Verifies spec: NFO
- [ ] Candidate states reach the screen from a real device, once the device sends them. Verifies spec: NSCR
- [x] Scan results are listed by SSID with the strongest signal and access-point count, access points with no SSID are left out until asked for, and a scan can go to one adapter. Verifies spec: NSCR
- [x] The siting view lists every access point by signal, with its channel and the adapter that heard it. Verifies spec: NSCR
- [x] With several radios, each wireless candidate and the hotspot offer an adapter, and only what that adapter supports. Verifies spec: NSCR, NET
- [ ] The siting view counts every channel a wide access point spans. Verifies spec: NSCR
- [ ] A survey can go to one adapter, and WPS can join on a chosen adapter. Verifies spec: NSCR

## Reporting

- [ ] NFO reports the wireless network joined, with its security and channel, and omits the entry where the device is joined to none. Verifies spec: NFO
- [ ] NFO reports the hotspot and its client count, and omits both where no hotspot runs. Verifies spec: NFO
- [ ] The NFO entries reflect the configuration in force immediately after a revert. Verifies spec: NFO
- [ ] VIEW renders the new entries as tiles in its own order. Verifies spec: VIEW

## Operations

- [ ] A scan reports the wireless networks the device can see. Verifies spec: CFG
- [ ] A survey reports the occupied and usable spectrum, and a device that cannot survey omits the capability. Verifies spec: CFG
- [ ] A device whose backend is absent, or which lacks the privilege to configure anything, says so rather than silently doing nothing.
