---
status: complete
---

# Network configuration module with wifi and hotspot

Working doc for the network configuration module: the wireless networks a device joins, the hotspot it runs, its wired addressing and its resolvers, and how a client configures all four over the channel.

This is bliti's first mutating feature.
Everything specified so far flows device to client as the facts and readings of [NFO](../../specs/device-info.md), rendered by [VIEW](../../specs/device-view.md).
Configuration adds a write path, which needs message types under [MSG](../../specs/messages.md), a stream role MSG does not yet have, and a safety story for a change that can cut a device off from the fleet.

## Scope

In: the wireless client, the hotspot, wired addressing, and DNS resolvers.

Out, and deliberately so:

- Hostname. NFO reports it as a fact; nothing here sets it.
- Overlay enrolment. Joining the tailscale overlay NFO reports on is its own problem.
- A recovery hotspot raised automatically when no configured network can be joined. This is one of the reasons hotspot mode exists at all, and the design here should not foreclose it, but it is its own card.

## Behaviour

### A document, plus operations

Durable settings are one declarative document covering the whole machine.
The client reads it, edits it, and writes it back whole; the device makes reality match it.
What is absent from the document is absent from the device, so the client never sequences anything and a repeated write changes nothing.

Transient acts sit outside the document as their own verbs, because a document reads badly for them:

- scan for visible networks now
- survey the spectrum now, for what the radio can see of what is occupied and usable
- run WPS against an access point now
- confirm the configuration now in force

It is one document for the machine rather than one per interface, and the flat ordering below settles that: an ordering whose members span links cannot live inside any one of them.

Absence means two different things in the document, and the distinction has to be explicit or it will be got wrong.
An *entity* that is absent is gone: a wireless network not in the document is one the device has forgotten.
A *setting* that is absent is unset, and the device supplies its own behaviour: a hotspot with no band named is one the device chooses a band for, not one with no band.

### One ordering of candidate attachments

The document holds an ordered list of ways the device might attach to a network, and works down it.
The ordering is flat: a wireless network and a wired configuration are peers in one list, and the wired ones sit above the wireless by default.

A candidate is one of:

- a wireless network, with its credentials
- a wired port taking its addressing from DHCP or SLAAC, at most one per port
- a wired port with a static configuration, of which a port may have several

The flat ordering is what lets a site say "our own access point, then the wall port, then these others", the case where a site's wireless is better than whatever the wired socket reaches.
A nested ordering would have been easier to render and could not have expressed it.

Several statics on one port is what carries a device that moves between sites.
Two sites both offer a wall port, on different subnets, and neither hands out DHCP; the device holds a static candidate for each and works out which site it is in by trying them.

That works because the gateway probe is already there.
Verification of a change and selection of a candidate are the same act: a static candidate whose gateway answers is the site the device is plugged into, and one whose gateway does not is the other site's.
DHCP and SLAAC need no such disambiguation, which is why one per port is enough for them.

A static candidate must name a gateway, because the gateway is what makes it distinguishable.
A wired network with no router is configured as the only static on its port, where there is nothing to tell apart.

At most one candidate is up per port and per radio, and several interfaces may be up at once.
The ordering decides which of them carries the default route rather than which one exists.
A device on a wall port and a wireless network at the same time is reachable on both, which matters on a device an operator may be trying to find.
Two statics on one port are alternatives, so only one of them is ever up.

This makes selection an ongoing behaviour rather than something that happens once at configuration time.
A device that is unplugged, moved and plugged in again works down the ordering afresh, with nobody present and no client connected.

Selection re-runs on three events:

- a link's state changes, a cable going in or out, or an access point coming into or out of range
- the candidate in force stops verifying, which catches the site that changed around a link that never lost carrier
- a candidate higher in the ordering becomes available again, so a device does not sit on a fallback after the preferred attachment has come back

All three are events rather than a poll, so a settled device does no work.

### The configuration session

A client configures a device by opening a stream and holding a conversation on it.
The stream is the session: it carries the configuration in force, each change, what became of it, and the confirmation, and its ending is the discard.

```
client → start
       ← current effective configuration                  device
         ... the operator edits, entirely on the client ...
client → proposed configuration
       ← applied, or invalid and why                      device
client → confirm
       ← confirmed                                        device
       ← current effective configuration                  device
         ... further edits on the same stream ...
client → discard, or end
```

Note where the editing sits.
It is entirely on the client and never reaches the wire: the operator types, and nothing is proposed until they say so.
Applying is a deliberate act, which is what stops a half-typed gateway being handed to the device as though it were a configuration.
See the editing stage below for what that means for the screen.

`discard` is valid at any point after a proposal, not only after one has been applied.
Sent while the device is still verifying it aborts the attempt; sent after it has applied it reverts.
Both land in the same place, which is the last confirmed configuration, so it is one verb rather than two.

A change stays provisional for exactly as long as the session, with no timer.
The operator confirms when they are satisfied, and ending the session without confirming discards.
This matches how MSG already makes a stream's life the whole story of a subscription, and leaves nothing to tune.

> **Spike resolved: the BLE session survives.** No-timer rests entirely on the BLE session surviving a network reconfiguration, and on target hardware it does.
>
> The concern was that wifi and Bluetooth share a chip on Pi-class boards, and that `brcmfmac` might reload firmware when the interface switches into AP mode, resetting the shared radio and taking the channel with it. That reload does not happen on this hardware.
>
> Run against a Raspberry Pi 5 (Cypress CYW43455, `brcmfmac43455-sdio` firmware 7.45.265, Bluetooth on UART), holding a live LE and GATT connection from a laptop in the same room while reconfiguring wifi on the device over an SSH control path that rides ethernet:
>
> 1. a wireless client association, then a forced disconnect and reconnect, while the session was open
> 2. a switch into AP mode, the suspected firmware-reload path
> 3. AP and STA up together on one channel
>
> Through all three the link kept carrying GATT reads, the device's controller logged one LE connection and never a disconnect, and `brcmfmac` loaded its firmware once at boot and never again. Switching wifi into AP mode left `hci0` untouched.
>
> So the confirm model stands as written: a change stays provisional for the life of the session with no timer, and the in-memory-across-a-dropped-channel fallback is not needed.
>
> One hardware note for the split. This chip runs AP and STA only on a single shared channel (its interface combinations allow one AP plus one managed interface, but constrain them to the same channel). A hotspot and a wireless client cannot sit on different channels at once, so where both are up the hotspot's channel follows the client's rather than being freely chosen.

Only one configuration session runs at a time.
A second is refused while one is open, and told why rather than left to guess.
Both operators are standing at the device and can sort it out between themselves.

This is deliberately not request/response.
MSG names three stream roles (the hello stream, a feed, a subscription) and this is a fourth, defined by this feature rather than added to the base protocol.
Squeezing a stateful conversation into request/response would be the wrong move when the channel already offers a full bidirectional stream.

### Applying a change: the device verifies, the client confirms the rest

A change is applied and then judged, rather than taken on trust.

The device judges what it can see from where it sits, and reverts on its own as soon as it can tell the change failed.
Where the device cannot judge the outcome, the change stays provisional until the client confirms it.

The device verifies as far as the first hop: the interface comes up, a wireless interface associates, an address is acquired or configured, and the gateway answers.
That catches a wrong key, an absent SSID, a network that associates and then hands out nothing, and a lease on a network that does not route.
It stops short of resolving a name or reaching a host beyond the gateway, which would fail on a network that is deliberately offline, and a device serving a clinic with no internet is a device on exactly such a network.

A revert restores the last *confirmed* configuration, not the step before.
A provisional configuration is never written down: it is applied to the running system and nowhere else.
That one property does most of the work, because everything that ends a session (an explicit discard, the stream closing, the channel dropping, a power cut, a reboot) then restores the last confirmed configuration as a consequence rather than as a rule anyone has to implement.

The device keeps no record of what was attempted.
The client holds that, and offers it back for editing when a change fails, so an operator fixes the field that was wrong instead of re-entering the whole document.

This puts a real requirement on the failure report, which carries three things:

- a pointer to the part of the document at fault, so the client can put the operator's cursor in the field that was wrong
- a free-text reason in the device's own words, on the same argument NFO's `status.reason` already makes: the useful half of a failure is the half nobody anticipated, and a code carries only the half that was foreseen
- which verification stage it reached: link, association, addressing, gateway. How far it got is often the most diagnostic thing available

"Associated, then the gateway did not answer" lets the operator correct one field and tells them the key was right. "The configuration did not work" makes them start again.

Note what the confirmation is for. The spike above confirms the operator holds a BLE channel that a network change cannot break, so it is not their own access being protected: it is whatever the device was reachable over remotely, and whatever it was serving locally.

### The editing stage is the client's alone

The operator moves through four stages, and only two of them involve the device.

| stage | screen | what it offers |
| --- | --- | --- |
| Editing | read-write | Apply, Reset |
| Applying | read-only | Cancel |
| Errored | read-write, filled with what was tried | Apply, Reset |
| Applied | read-only | Confirm, Cancel |

**Editing is local.** Nothing is on the wire, the device is running what it was running before, and the operator can type freely. Reset throws the edits away and returns to the configuration in force.

**Applying is the deliberate act.** Without it the client would be proposing a configuration every time the operator paused between keystrokes, and a gateway typed halfway is a configuration that fails for a reason that is not real. The screen goes read-only while the device works, because a field edited mid-verification belongs to neither the attempt nor the next one.

**Errored returns to read-write, filled with what was tried** rather than with what the device reverted to. This is the concrete form of the earlier decision that the client holds the attempt and the device keeps no record of it: the operator lands back in the document they wrote, with the offending field marked, and fixes that one thing.

**Applied is read-only and not yet saved.** The device is running it; nothing is written down. Confirm saves it, Cancel reverts.

The editing stage is also where the client validates. It already knows what the device supports, because the device said so when the session opened, so a document the device would reject should rarely reach it: Apply is the client's assertion that what it holds is coherent and within what it was told the device can do.

The stages are the client's own and belong in its spec rather than in the wire's.
The wire sees a proposal, a verdict, and then a confirm or a discard; it does not know that an operator spent two minutes typing before any of that.

### Secrets travel in full

The configuration the device reports carries its secrets as they are: pre-shared keys, the hotspot password, enterprise credentials.
The channel is authenticated and encrypted under [CHN](../../specs/channel.md), and the client already holds the QR code, so there is nothing left to withhold from it.

This is what makes read-edit-write work at all: the client writes back what it read, and a secret it did not touch needs no special case.

### Hotspot

The hotspot serves clients that join it directly, and re-shares an upstream link where the device has one.
Sharing is a setting rather than automatic, but it defaults to on.

Wireless client and hotspot run concurrently where the chipset and driver allow AP+STA, and one at a time where they do not.
The device reports which of the two it can do when the session opens, and rejects a document asking for both where it cannot, naming the conflict.
A client that offered such a document ignored what it was told, so this is an invalid document rather than a partial application.

AP+STA is one case of a general rule: **the device states what it supports when the session opens, and a document asking for anything else is invalid.**
There is no applied-in-part outcome and no unhonoured setting to report, because a client that has been told what the device can do has no reason to ask for more.
That keeps the device from ever running a configuration that differs from the one that was written, which is what makes the document's declarative reading true rather than aspirational.

The operator sets the SSID and the password; neither is derived from the device and neither has a default.
The client offers to generate a password, so the operator is not inventing one, but the generated value is an ordinary field they can replace and the device knows nothing about where it came from.

The hotspot exists only because a configuration says so.
A device out of the box raises nothing: it is reachable over BLE alone, which is what the QR code is for.

Band, channel and channel width are settings on hardware whose radio can run an access point and a client on independent channels.
All three default to the device choosing, so an operator who knows their own RF environment can place the hotspot away from the site's access points, and one who does not leaves them unset.
The spectrum survey verb above is what makes the first of those possible: it reports what the radio can see of the occupied and usable spectrum, where the hardware can tell.

Much hardware cannot do that, including the board this targets: its radio runs an access point and a client only on one shared channel, so a hotspot sharing a radio with a wireless client has no channel of its own to be given.
On such a device the three settings are not offered at all, and the device chooses, following the client's channel whenever one is associated.

This is deliberately an absent setting rather than an overridden one.
The device advertises the constraint with the rest of its capabilities when the session opens, so the client never presents a field the device could not honour, and nothing is applied-in-part.
A setting that existed and was quietly overridden the moment a client associated would be the unhonoured setting the capability rule exists to prevent, and it would be worse than most, because whether it held would depend on something that changes while nobody is looking.

Where the fields are absent the client says why, rather than leaving an operator hunting for a control that is not there.

The address range has a fixed default, the same on every bliti device, which an operator may override where it clashes with an upstream or where a site has its own conventions.

Clients on the hotspot are isolated from one another by default, and a site that needs them to see each other turns that off.

The regulatory domain is a setting the operator fills in, because the operator is the party standing in the country.
Unset means the most restrictive world-safe behaviour, so an unconfigured device is legal wherever it is switched on and a configured one gets the full local channel set.

### Wired addressing and DNS

Resolvers belong to a link, alongside its addressing, rather than sitting globally over the device.
A site's internal names commonly resolve only on that site's own network, and per-link resolvers are what make that work on a device holding several links at once.

Configured resolvers lead and those the link supplies follow, rather than replacing them.
A site handing out an internal resolver over DHCP keeps resolving its own names even where an operator has added a public resolver for everything else.

The document defines its own model rather than projecting whatever the device's network stack happens to support.
A spec has to constrain a re-implementation, and one that deferred to a backend would constrain nothing and would change meaning whenever the image changed.
A device maps the document onto the stack it has, and what it cannot map it never offers: a setting its stack cannot honour is one the device leaves out of the capabilities it advertises when the session opens, so it never reaches a document at all.
This is the same rule the shared-channel constraint follows, and it is why there is no applied-in-part outcome anywhere in this design.

Where our own model has no strong opinion, take the shape from netplan rather than inventing one.
It already covers wired addressing, per-link resolvers, wireless credentials and access-point mode, in a declarative document that is close to what this needs, so it is a good source of naming and structure even though nothing here defers to it at runtime.

### Wireless security

A device joins WPA2-PSK and WPA3-SAE networks, including the transitional mode most access points ship with, and 802.1X enterprise networks.

It also joins by WPS, both push-button and PIN, which is worth something on a device with no keyboard.
PIN mode is brute-forceable and WPA3 drops WPS entirely in favour of DPP, and neither is a reason to withhold it here.

Two rules are at work, on two different axes, and keeping them apart is what makes both defensible.

**How a device obtains credentials is the site's business.**
bliti supports what a network can do and does not set policy on it.
A device that refused WPS PIN because the mechanism is weak would be making a decision that is not its to make, on a network it is a guest of, and the operator would be left with a device that will not join and no good account of why.

**Whether the link authenticates the access point is bliti's business.**
A device joins only networks that give it some way to establish that the access point is the one it meant to join.
WPA2-PSK, WPA3-SAE and 802.1X all do: the first two by proving possession of the key, the last by certificate.

Open networks fail the second rule, and so does OWE, the encrypted-open mechanism of [RFC 8110](https://www.rfc-editor.org/rfc/rfc8110.html) that the Wi-Fi Alliance certifies as Enhanced Open.
OWE was considered on the strength of its encryption: a per-association Diffie-Hellman gives a unique pairwise key, so a passive observer cannot derive traffic keys the way they can on a public-PSK network.
It was rejected on the strength of what that encryption is worth without authentication.
RFC 8110 section 7 says plainly that the client "will have no authenticated identity for the access point, and vice versa", that OWE "is susceptible to an active attack in which an adversary impersonates an access point", after which the adversary can "inspect, modify, and forge any data", and that OWE "is not a replacement for any authentication protocol".
Section 6 goes further and directs that an OWE network not be shown with a lock icon, on the grounds that a user should read it as open.
A mechanism whose own specification declines to be presented as secure is not one to build a rule around.

This matters because application traffic crosses the wifi link directly.
The overlay carries management rather than the application, so bliti cannot assume anything protects that traffic above the link.

Support would not have been the obstacle: `wpa_supplicant` builds it with `CONFIG_OWE=y`, NetworkManager has carried it since 1.24, and iwd exposes it as the `owe` security type.

> At split time, write this as the positive requirement it is: a device joins only a network that authenticates the access point. Do not write "OWE is not supported" or "open networks are not supported". Those are absences, and the spec rules rule them out.

DPP is the WPA3-era replacement for WPS, and is its own card.

The hotspot is the other side of the first rule, and the rule does not carry over to it: bliti does not implement WPS or DPP *as* an access point.
Joining by whatever a site's access point offers is supporting what exists; offering a deprecated onboarding mechanism to our own clients would be a policy choice, and that one is ours to decline.

Captive-portal detection was not asked for.

### Reporting state

Two paths, for two different readers.

A summary goes into the NFO catalogue as facts and readings, so it arrives on the `default` topic with everything else and VIEW renders it beside the existing network entries.
This is the at-a-glance view: an operator reading the device screen sees which network it is on and whether the hotspot is up, without opening a configuration screen.

The exact configuration comes over the configuration session above, which opens by sending it.
A topic is the wrong shape for this: topics in MSG are unidirectional push feeds, and this is a conversation.

The NFO catalogue gains:

| entry | kind | what it reports |
| --- | --- | --- |
| the wireless network in use | fact | the SSID joined, with the security in force and the channel as traits |
| hotspot state | fact | the hotspot's SSID, with the channel as a trait |
| hotspot clients | reading | how many clients are joined |

Channel is reported for the hotspot and for the wireless client link both.
That is what makes the shared-channel behaviour legible instead of mysterious: an operator who cannot set the hotspot's channel can at least see the one it landed on, and see that it is the client's.

It rides as a trait on the two entries rather than as catalogue entries of its own, because it describes the link being reported rather than distinguishing one instance of it from another. It is descriptive in NFO's sense, as `route` and `overlay` already are.

`network-address` and the `interface` trait's `route` member already carry which link is carrying traffic, so nothing new is needed for that.

Signal strength was considered and left out. Worth revisiting if siting a device turns out to be a thing operators struggle with, since it is the obvious reading for "is here good enough" and VIEW would graph its history for free.

VIEW renders all three as tiles, in its own order alongside `network-address` and `network-throughput`.
The hotspot entries appear only on a device actually running a hotspot, so the screen stays quiet on the devices that are not.
This costs nothing under VIEW's existing rules: it already renders every entry it receives and never reports one as missing, so a device that sends no hotspot entries simply has no hotspot tiles.

### The configuration screen

Mocked up as "Network configuration", built on the web app's own variables and component shapes rather than invented, so it sits inside the screen an operator already knows.

Decisions it makes, which are the parts worth holding on to:

- **The ordering is the primary surface.** It renders as a reorderable list, each row carrying the candidate's live state: in use, joined, not here, not offered, out of range. An operator reads which attachment is carrying traffic and why the others are not, without opening anything.
- **The verification stages render as ticks and a cross.** This is what makes the stage worth carrying on the wire: "link, address, then gateway failed" tells an operator the addressing was fine and the network is not routing, without a sentence saying so.
- **The pointer renders as a marked field**, with the device's reason as prose beneath the stages. The two do different jobs: the pointer puts the operator in the right box, the reason says the thing nobody anticipated.
- **The hotspot's radio settings sit behind a disclosure**, all defaulting to "device picks", so the ordinary case is a name, a password and two checkboxes.
- **The session state stays in view.** A provisional change is otherwise invisible, because the device is working and nothing has been written down. It says what is running, that it is not saved, and what leaving costs. It belongs either at the top of the screen or pinned to the bottom of the viewport, not at the end of the document where an operator has to scroll to find out their change is unsaved. The mockup renders it inline at the top, because a sticky element sits over the rest of a mockup and makes it harder to review; that is a concession to the mockup and not the intended placement.

The client supplies its own wording throughout, as VIEW already requires of it: the wire says confirm and discard, the screen says Save and Discard, and the wire's vocabulary never reaches the operator.

**Write the microcopy for a technician, but pick the word someone will reach for.** Two rules that mostly agree and sometimes do not.

Where a plain paraphrase is vaguer about what the thing *is*, use the technical term: SSID rather than name, passphrase rather than password, DHCP range rather than addresses handed out, isolated rather than kept apart. Plain language reads friendlier and is vaguer, and vagueness is what costs someone a site visit.

Where the plain word is what someone scanning the screen would look for, keep it even though a more precise term exists. "Country" stays, because someone hunting for the regulatory domain setting pattern-matches on the country; "Regulatory domain" is more correct and less findable. "Order tried" stays for the same reason.

The distinction is that the first rule is about a label being ambiguous once you have found it, and the second is about finding it at all. Precision loses to discoverability, and only to discoverability.

The same care applies to what a candidate's state says. "No gateway", "No lease" and "Out of range" report what the device observed; "not here" and "not offered" are the same information with the diagnosis taken out of it.

## Implementation options

### What configures the network underneath

The device has to hand the configuration to something. Candidates:

- NetworkManager, over its D-Bus API. Handles wireless client, AP mode, wired addressing, DNS and connection priority itself, and is what a Raspberry Pi OS image ships with. Largest dependency, and its own model would have to be mapped onto ours. Note that the board this targets runs Ubuntu rather than Raspberry Pi OS, and has never had it installed.
- systemd-networkd with wpa_supplicant. Smaller and file-driven, and already what the image runs. AP mode is the assembly, since it means driving hostapd; the hotspot's DHCP server is not, because networkd carries its own.
- iwd with systemd-networkd. Modern and small, good WPA3 and enterprise support, weaker AP story.

Whichever it is, the device's own configuration document is the contract and the backend is not: the spec describes the document, not the tool.

Open: whether bliti owns these files outright, or merges with what an image already ships.

### Device-side packages

The packages a device needs at runtime are recorded in [services/README.md](../../../services/README.md), beside the unit file and the bluetoothd configuration, so that a Debian package can declare them when there is one to declare them in.
That list forks on the choice above, which is the other reason it is written down: the first option costs nothing the image does not already carry, and the other two each add a package to it.

### Privileges

Changing network configuration needs more than the daemon has today.
Either the daemon runs privileged, or it holds specific capabilities, or it talks to something that does.

## Open questions

Behaviour is settled. Nothing below blocks the split.

- [x] **Blocking spike:** does the BLE session survive a network reconfiguration on target hardware? Yes. Verified on a Raspberry Pi 5 (Cypress CYW43455) across a wireless client connection change, a switch into AP mode, and AP+STA together: the LE session carried traffic throughout, the controller never disconnected, and `brcmfmac` never reloaded its firmware. Detail in the configuration session section above.

Two decisions are deliberately left open for the plan rather than the specs, because both are implementation choices that constrain nothing an operator can observe:

- which backend configures the network underneath, of the three candidates above
- whether bliti owns the backend's files outright or merges with what an image already ships

The split should carry these into the plan as open, not treat them as unresolved spec questions.

## Trade-offs

**Secrets in full over a marker.** Returning secrets makes read-edit-write trivially correct and means a client can show an operator the hotspot password to read out. It also means every client that ever connects holds every key the device holds, and a screenshot of a configuration screen is a disclosure. The channel's authentication is what makes this defensible, and it is only defensible while that holds.

**One document over targeted operations.** Idempotence and no sequencing, at the cost of a client having to send back settings it did not mean to touch. That cost is what makes the secrets decision above load-bearing rather than incidental.

**Hotspot credentials outside the identity chain.** Everything else about a bliti device descends from its board ID, and deriving the hotspot's SSID and password would have fitted that, made them reproducible from the device alone, and let them be printed beside the QR code. They are operator-set instead, which means a device has no hotspot until someone configures one and a lost password is lost rather than recomputable. What it buys is that the hotspot password, the one credential here meant to be read aloud and handed to strangers, has no relationship to the chain the device's identity hangs off.

**Capability up front rather than applied-in-part.** The device says what it supports when the session opens, so there is no partial application and no unhonoured setting to report. That is what keeps the document's declarative reading true: what the device is running is always what was written. The cost is that capability reporting has to be complete and honest, because it is now the only thing standing between a client and an invalid document, and a capability the device forgets to mention becomes a feature nobody can use.

**One ordering of candidates, not of links.** Making a wired static a candidate like any other gets site-switching for free and reuses the gateway probe that verification already needed. It also means the ordering is a list an operator has to understand, on a device where most of the entries are about one physical port, and it puts real weight on the probe: everything about selection now rests on a gateway answering.

**Authentication, not encryption, is the line.** Drawing it at encryption looked equivalent and was not: OWE encrypts and authenticates nothing, so an encryption rule would have admitted a link on which a rogue access point reads and rewrites everything. Drawing it at authentication also stops the rule fighting with the one above it, because obtaining credentials and authenticating a link are different axes: WPS PIN is a weak way of getting a key for a link that does authenticate, which is why supporting it costs nothing here. The cost of the line is real, though: a device cannot be deployed at a site that runs an open guest network and nothing else, and no amount of the operator insisting will change that.

**The device keeps no history.** Reverting to the last confirmed configuration, and never persisting a provisional one, means the device holds exactly one configuration on disk and needs no unwind stack, no window that must survive a reboot, and no reconciliation after a power cut. The cost moves to the client, which has to hold the attempt and re-offer it, and to the failure report, which has to be specific enough for that re-offer to be worth anything.

**Concurrent AP+STA where supported.** Honest about hardware, but it makes the device's capability part of the protocol: the client must be told what the device can do before it can offer a coherent configuration. Requiring concurrency outright would have been simpler and would have narrowed the hardware.

**Capability reaches into the screen, not just the document.** The shared-channel constraint is the case that proved this. It could have been handled by accepting a hotspot channel and overriding it, which keeps the document uniform across hardware and makes the running configuration a thing the operator has to reason about separately. Instead the capability removes the field. The document's shape now varies by device, and a client cannot render a configuration screen without first asking the device what it is; what it buys is that a setting on screen is always a setting that will hold.

## Testing notes

- The BLE session survives a client connection change, a switch into AP mode, and AP+STA coming up together, on real target hardware. Verified by the spike, so this lands ticked at the split; it stays on the list because a firmware or hardware change could take it back.
- A device that joins each supported security type: WPA2-PSK, WPA3-SAE, transitional, enterprise, WPS push-button, WPS PIN.
- An open network and an OWE network are both refused, with a reason an operator can act on rather than a bare failure.
- An access point in OWE transition mode, which broadcasts an open BSS beside the OWE one, is refused on both.
- A second client opening a configuration session while one is open is refused, with a reason.
- A document asking for AP+STA on hardware that cannot do it is rejected, naming the conflict.
- Editing puts nothing on the wire: a device watched through a session sees no proposal until Apply is pressed.
- A half-typed gateway is never proposed, and Reset during editing returns the fields to the configuration in force.
- Cancel during verification aborts the attempt, and the device lands on the last confirmed configuration.
- After a failure the fields hold what was tried, not what the device reverted to, and the failing field is marked.
- A session abandoned without confirming (stream closed, channel dropped, device powered off) leaves the last confirmed configuration in force.
- A configuration that cannot work (wrong key, absent SSID) reverts, and the device reports both the attempt and the revert.
- A change the device cannot judge stays provisional, and reverts when the session ends without a confirmation.
- The channel drops mid-verification, and the device lands on the last confirmed configuration rather than somewhere accidental.
- Read the configuration, write it back unmodified, and observe that nothing changes.
- Hotspot with an upstream present and absent; sharing on and off.
- Hotspot and wireless client concurrently on hardware that supports it, and the fallback on hardware that does not.
- On shared-channel hardware the band, channel and width fields are absent, the client says why, and a document carrying them is invalid.
- On shared-channel hardware the hotspot follows the client's channel when one associates, and NFO reports the channel both are on.
- On independent-channel hardware the three fields are present and a set channel is the one the hotspot comes up on.
- Ordering: the default route follows the ordering, and moves when a higher candidate comes back.
- A device holding two static wired candidates on different subnets picks the right one at each site, unattended, with no client connected.
- A site that changes around a device whose cable never moved is noticed, and the device re-selects.
- A device on a fallback candidate moves back up when the preferred one returns.
- A wired and a wireless candidate up at once: the device is reachable on both, and the default route follows the ordering.
- A static candidate with no gateway is rejected where another static shares its port.
- A hotspot with band, channel and width unset comes up on a channel the device chose; with them set, on the one asked for.
- An unset regulatory domain restricts the device to world-safe behaviour; a set one opens the local channel set.
- Configured resolvers lead and DHCP-supplied ones still answer for a site's internal names.
- NFO entries reflect the configuration in force, including immediately after a revert.
- A device whose backend is not running, or which lacks the privilege to configure anything, reports that rather than silently doing nothing.
