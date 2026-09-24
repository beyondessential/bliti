# Network configuration module with wifi and hotspot

Scenarios that verify the module, for manual checking and as the brief for automated tests.

## Hardware and radio

- [x] The BLE session survives a wireless client connection change, a switch into AP mode, and AP+STA coming up together, on real target hardware. Verified on a Raspberry Pi 5 (Cypress CYW43455): the LE session carried GATT reads throughout, the controller logged no disconnect, and `brcmfmac` loaded firmware once at boot. Stays on the list because a firmware or hardware change could take it back.
- [ ] A device reports its radio's real capabilities: AP+STA concurrency, the shared-channel constraint, and the WPS methods offered. Verifies spec: NET
- [x] On shared-channel hardware the hotspot's band, channel and width are offered in capabilities, limited to the channels an access point can start on and the renderer renders. Verifies spec: HOT, NET
- [x] On shared-channel hardware a document setting the hotspot's band, channel or width is valid where no wireless candidate could be carried by that radio, and is rendered on the chosen channel. Verifies spec: HOT
- [ ] On shared-channel hardware with no wireless candidate, the hotspot comes up on the channel the document chooses. Verifies spec: HOT
- [x] On shared-channel hardware a document setting the hotspot's band, channel or width beside a wireless candidate that radio could carry is invalid at the first of the three it sets. Verifies spec: HOT, NET
- [x] The screen offers the hotspot's band, channel and width on a shared-channel adapter while no wireless candidate could share it, and once one could takes them out and says the hotspot runs on that connection's channel. Verifies spec: NSCR
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
- [x] A configuration that cannot work, by wrong passphrase and by absent SSID, is reported with the stage it failed at. Verifies spec: CFG
- [x] A session the daemon drops when its client unsubscribes ends its configuration session, so the next client is not told the device is busy. Verifies spec: CFG
- [x] A client that walks away with a proposal applied and its feed closed leaves the device back on its recorded configuration, on real hardware. Checked on the prototype with a country change and the client killed. Verifies spec: CFG
- [x] An act on an adapter or with a method the device did not offer is invalid at the act's own path. Verifies spec: NET, CFG
- [x] A `wps` naming an `ssid` where the device does not offer one is invalid at `$['ssid']`, and nothing is joined. Verifies spec: NET, CFG
- [x] `applied` carries capabilities only where applying changed them, and the next `state` carries them where a revert changed them back. Verifies spec: CFG
- [x] `state` follows the opening configuration, each change, the proposal once applied, and the recorded configuration after a revert, and is held while a proposal is verified. Verifies spec: CFG
- [x] Joining by WPS PIN sends the PIN before the joined result, while the join is under way. Verifies spec: CFG
- [x] A proposal interrupted by a newer one is answered `invalid` at `$`, and one refused after it superseded another restores the recorded configuration. Verifies spec: CFG
- [x] An unconfigured device holds one dynamic candidate per physical wired interface and no hotspot. Verifies spec: CFG
- [x] An unconfigured device plugged into a network with DHCP is reachable on it, on real hardware. Verifies spec: CFG
- [ ] A power cut with a proposal applied brings the device back on its recorded configuration, with nothing of the proposal brought up at boot, on real hardware. Verifies spec: CFG

- [x] A proposal is judged only on candidates it adds or changes that carry `verify` true, per interface: a wrong passphrase beside a working wall port fails, a static per site on one port applies at either site, and a candidate carrying `verify` false fails nothing. Verifies spec: CFG
- [x] A failure that is not a candidate's, such as a hotspot that does not start, carries no `reached`. Verifies spec: CFG
- [x] A wrong passphrase beside a working wall port is refused, and applying it again unchecked is applied, on real hardware. Verifies spec: CFG

## Several clients at once

- [x] What a client writes reaches only its own session, and one client's session ending leaves another's running. Verifies spec: CHN
- [ ] A client's writes reach its session in the order it made them, both with and without a response, on the prototype. Verifies spec: CHN
- [ ] Two clients connected at once each hold a session of their own, and one leaving or failing its handshake leaves the other's running, on the prototype. Verifies spec: CHN
- [ ] A device serving a session goes on advertising, so a second client finds it, on the prototype. Verifies spec: ADV
- [ ] A second client opening the network settings while another holds a configuration session is told the device is busy, on the prototype. Verifies spec: CFG

## Attachment and selection

- [ ] A device verifies a candidate through carrier, association, addressing and the gateway answering, in that order. Verifies spec: LINK
- [ ] A candidate whose network associates but hands out no address fails at the addressing stage. Verifies spec: LINK
- [ ] A candidate holding a lease on a network that does not route fails at the gateway stage. Verifies spec: LINK
- [ ] A candidate on a network with no route beyond the gateway is established, not failed. Verifies spec: LINK
- [ ] A device holding two static candidates on different subnets picks the right one at each site, unattended and with no client connected. Verifies spec: LINK
- [x] A static candidate carrying no gateway is rejected as invalid. Verifies spec: LINK
- [x] A second dynamic candidate on an interface that already has one is rejected as invalid. Verifies spec: LINK
- [ ] A wired and a wireless candidate up at once leave the device reachable on both, with the default route following the ordering. Verifies spec: LINK
- [x] The default route moves when a candidate above the one in force becomes available. Verifies spec: LINK
- [ ] A site that changes around a device whose cable never moved is noticed, and the device selects again. Verifies spec: LINK
- [ ] A settled device does no polling to detect any of the three selection events. Verifies spec: LINK
- [ ] Configured resolvers are queried before those a link supplies, and supplied ones are queried where a candidate names none. Verifies spec: LINK
- [x] The selector, unattended: statics on one port told apart at two sites, a lease with no route failing at `gateway`, association without an address failing at `addressing`, the default route following the order and moving back, and a site changing around a cable that never moved. Verifies spec: LINK
- [x] A pinned wireless candidate stays on its radio, and an unpinned one takes a free radio hearing it best. Verifies spec: LINK
- [ ] A site's own names reach the site's resolvers while the candidate names a public resolver of its own, on real hardware. Verifies spec: LINK

## Wireless

- [ ] With a hotspot applied on the shared-channel radio, a scan still hears networks on both bands. Once seen on the prototype as 2.4 GHz only; not reproduced in a later run (one 5 GHz access point heard with and without the hotspot). Verifies spec: HOT, NSCR
- [x] Setting the country restarts iwd, and a network the radio still hears is not reported out of range for it: a wrong passphrase on a network heard well is refused at association, on the prototype. Verifies spec: LINK, WLAN
- [x] Renaming a candidate, with nothing else about it changed, keeps the addresses and gateway its link holds, on the prototype. Verifies spec: LINK
- [ ] A device joins WPA2-PSK, WPA3-SAE, the transitional mode, and an 802.1X enterprise network. Verifies spec: WLAN
- [x] A device joins an 802.1X network by PEAP with MSCHAPv2, checking the server by certificate authority and domain, on the prototype (hostapd's own EAP server on the laptop). Verifies spec: WLAN
- [x] A device refuses an 802.1X network whose server does not carry the domain given, at association, naming what to check, on the prototype. Verifies spec: WLAN
- [x] A radio on a driver whose SAE is disabled offers no `sae` or `psk-sae`, and iwd is told not to run SAE on it. Verifies spec: WLAN, NET
- [x] A `psk` candidate on the Pi joins a transitional access point offering SAE with H2E, over WPA2, on real hardware. Verifies spec: WLAN
- [x] A proposal correcting a passphrase that failed joins on the same session, on real hardware.
- [x] A wireless candidate whose network comes into range after the proposal is applied is joined, on real hardware. Verifies spec: LINK
- [ ] A device joins by WPS push-button and by WPS PIN. Verifies spec: WLAN
- [x] Joining by WPS for a named network joins it where the access point hands over that network's credentials. Verifies spec: WLAN, CFG
- [x] Joining by WPS for a named network refuses credentials for another network at `$['ssid']` with no stage reached and a reason naming the network handed over, forgetting them and adding nothing to the configuration. Verifies spec: WLAN, CFG
- [ ] Joining by WPS for a named network with another access point's button pressed leaves iwd holding nothing for that network and the device on its recorded configuration, on real hardware. Verifies spec: WLAN, CFG
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

- [x] On a shared-channel radio the hotspot starts once its wireless client has associated, on the client's channel, and on its own channel where the client does not join. Verifies spec: HOT
- [x] A wireless client associated on a channel no access point may start on fails the proposal at the hotspot, naming the channel. Verifies spec: HOT
- [x] A station knocked off its network as the hotspot starts beside it joins again, and the proposal does not fail for it.
- [x] A join iwd reports without a channel takes the channel from the radio, and leaves a running hotspot where it is.
- [x] A proposal carrying a hotspot and a wireless network on the Pi ends applied, with both up on the network's channel. Verified on 2.4 GHz channel 1 against the laptop's access point; 5 GHz not yet. Verifies spec: HOT

- [x] A hotspot channel and width whose span takes in a channel no access point may start on is invalid at the width, and a width the channel does not bond to is refused. Verifies spec: HOT

## The document

- [ ] A wireless network absent from a document is forgotten by the device. Verifies spec: NET
- [ ] A setting absent from a document leaves the device supplying its own behaviour, rather than leaving the thing unconfigured. Verifies spec: NET
- [x] A document asking for anything outside the device's stated capabilities is invalid, and no part of it is applied. Verifies spec: NET
- [ ] An unset regulatory domain restricts the radio to what every domain permits, and a set one opens the local channel set. Verifies spec: NET
- [ ] The configuration a device reports carries its secrets in full. Verifies spec: NET

## The screen

- [x] Leaving the screen with a proposal applied keeps the session open, and the device view confirms or discards it. Verifies spec: NSCR
- [x] Leaving the screen with nothing changed closes the session. Verifies spec: NSCR
- [x] Leaving the screen with edits not applied keeps them and the session, and the device view says how many wait and offers review or discard. Verifies spec: NSCR
- [ ] A proposal kept open that fails while the operator is on the device view keeps its failure until they return to the screen. Verifies spec: NSCR
- [x] A network picked from a scan is not hidden, and cannot be marked hidden. Verifies spec: NSCR
- [ ] Applying, scanning, surveying and opening a session each show they are waiting on the device. Verifies spec: NSCR
- [ ] The ordering says how the device chooses among the candidates, and the candidate being edited is plain to see, on a phone. Verifies spec: NSCR
- [x] Editing puts nothing on the wire: a device watched through a session sees no proposal until apply is pressed. Verifies spec: NSCR
- [x] A half-typed gateway is never proposed. Verifies spec: NSCR
- [x] Reset during editing returns the fields to the configuration in force. Verifies spec: NSCR
- [x] Fields are not editable while a proposal is being verified. Verifies spec: NSCR
- [x] After a failure the fields hold what was proposed, not what the device reverted to, and the field named by the failure is marked. Verifies spec: NSCR
- [x] The verification stages show which passed and which failed. Verifies spec: NSCR
- [x] Only the candidate a failure names offers to go unchecked, and proposing again leaves every other candidate checked. Verifies spec: NSCR
- [x] A refused passphrase marks the passphrase field, and the candidate still offers to go unchecked. Verifies spec: CFG, NSCR
- [x] An unchecked candidate is marked as such, and checking can be turned back on while editing. Verifies spec: NSCR
- [x] Candidates left unavailable after applying one unchecked show their state. Verifies spec: NSCR
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
- [x] The siting view counts every channel a wide access point spans. Verifies spec: NSCR
- [x] A survey can go to one adapter, and WPS can join on a chosen adapter. Verifies spec: NSCR
- [x] A network the scan lists by SSID, and the device can join, offers joining it by WPS for that network alone, on the adapter the candidate is pinned to, where the device takes a named network; a refusal names the network and shows the device's reason. Verifies spec: NSCR

## Reporting

- [x] An entry the device stops reporting is sent once more as ended, and its tile and history go. Verifies spec: NFO, VIEW
- [ ] A trial hotspot's tiles go when it reverts, without a reload, on the prototype. Verifies spec: NFO, VIEW
- [x] While a proposal is being tried, the device says so, and the device view marks the network tiles and says they revert. Verifies spec: NFO, VIEW
- [ ] A second client watching the device sees the trial marked too, on the prototype. Verifies spec: NFO, VIEW
- [ ] NFO reports the wireless network joined, with its security and channel, and omits the entry where the device is joined to none. Verifies spec: NFO
- [ ] NFO reports the hotspot and its client count, and omits both where no hotspot runs. Verifies spec: NFO
- [ ] The NFO entries reflect the configuration in force immediately after a revert. Verifies spec: NFO
- [ ] VIEW renders the new entries as tiles in its own order. Verifies spec: VIEW

## Operations

- [ ] A scan reports the wireless networks the device can see. Verifies spec: CFG
- [ ] A survey reports the occupied and usable spectrum, and a device that cannot survey omits the capability. Verifies spec: CFG
- [ ] A device whose backend is absent, or which lacks the privilege to configure anything, says so rather than silently doing nothing.
