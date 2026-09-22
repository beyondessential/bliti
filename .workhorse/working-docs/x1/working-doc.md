---
status: draft
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
- run WPS against an access point now
- confirm the configuration now in force

It is one document for the machine rather than one per interface, and the flat ordering below settles that: an ordering whose members span links cannot live inside any one of them.

### Link preference is one ordering

The wired link and the wireless networks sit in a single ordering that decides which link carries the default route, with the wired link above the wireless ones by default.
A device holds several wireless networks and tries them in the operator's order rather than picking for itself.

The ordering is flat: the wired link and each individual wireless network are peers in one list.
This is what lets a site say "our own access point, then the wall port, then these others", the case where a site's wireless is better than whatever the wired socket reaches.
A nested ordering would have been easier to render and could not have expressed it.

### The configuration session

A client configures a device by opening a stream and holding a conversation on it.
The stream is the session: it carries the configuration in force, each change, what became of it, and the confirmation, and its ending is the discard.

```
client → start
       ← current effective configuration                  device
client → proposed configuration
       ← applied, or invalid and why                      device
client → confirm
       ← confirmed                                        device
       ← current effective configuration                  device
         ... further changes on the same stream ...
client → discard, or end
```

A change stays provisional for exactly as long as the session, with no timer.
The operator confirms when they are satisfied, and ending the session without confirming discards.
This matches how MSG already makes a stream's life the whole story of a subscription, and leaves nothing to tune.

> **Blocking spike.** No-timer rests entirely on the BLE session surviving a network reconfiguration. That is not free on the hardware this targets: wifi and Bluetooth share a chip on Pi-class boards, and `brcmfmac` can reload firmware when the interface switches into AP mode, which resets the shared radio and would take the channel with it. If the session does not survive, then reconfiguring is itself the disconnect, every change discards the moment it is made, and the confirm model has to change. Verify on real hardware before this is specified: a client connection change, a switch into AP mode, and AP+STA coming up together.

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

Note what the confirmation is for. Assuming the spike above comes back clean, the operator holds a BLE channel that a network change cannot break, so it is not their own access being protected: it is whatever the device was reachable over remotely, and whatever it was serving locally.

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

The operator sets the SSID and the password; neither is derived from the device and neither has a default.
The client offers to generate a password, so the operator is not inventing one, but the generated value is an ordinary field they can replace and the device knows nothing about where it came from.

The hotspot exists only because a configuration says so.
A device out of the box raises nothing: it is reachable over BLE alone, which is what the QR code is for.

Open: band and channel, the address range it hands out, and whether clients on it are isolated from one another.

### Wired addressing and DNS

Resolvers belong to a link, alongside its addressing, rather than sitting globally over the device.
A site's internal names commonly resolve only on that site's own network, and per-link resolvers are what make that work on a device holding several links at once.

The document defines its own model rather than projecting whatever the device's network stack happens to support.
A spec has to constrain a re-implementation, and one that deferred to a backend would constrain nothing and would change meaning whenever the image changed.
A device maps the document onto the stack it has, and reports a setting it cannot honour as not applied rather than silently dropping it.

Where our own model has no strong opinion, take the shape from netplan rather than inventing one.
It already covers wired addressing, per-link resolvers, wireless credentials and access-point mode, in a declarative document that is close to what this needs, so it is a good source of naming and structure even though nothing here defers to it at runtime.

Open: what a device does with a setting its stack cannot honour. Is the document invalid, or applied-in-part with that setting reported unhonoured?

### Wireless security

A device joins WPA2-PSK and WPA3-SAE networks, including the transitional mode most access points ship with, and 802.1X enterprise networks.

It also joins by WPS, both push-button and PIN, which is worth something on a device with no keyboard.
PIN mode is brute-forceable and WPA3 drops WPS entirely in favour of DPP, and neither is a reason to withhold it here.

The principle that settles this, and others like it: **bliti supports what a network can do, and does not set policy on it.**
The network belongs to the site.
A device that refused WPS PIN because the mechanism is weak would be making a decision that is not its to make, on a network it is a guest of, and the operator would be left with a device that will not join and no good account of why.

DPP is the WPA3-era replacement, and is its own card.

The hotspot is the other side of this, and the principle does not carry over: bliti does not implement WPS or DPP *as* an access point.
Joining by whatever a site's access point offers is supporting what exists; offering a deprecated onboarding mechanism to clients would be a policy choice, and that one is ours to decline.

A device does not join an open, unencrypted network.
This is the one place bliti does impose a policy, and it is worth being clear why it is not a contradiction of the principle above: the principle is that bliti does not judge *how* a site authenticates its devices, and this is a requirement that the link be encrypted at all.
These devices carry health data, and an unencrypted link is not a site decision.

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
| the wireless network in use | fact | the SSID joined, and the security in force |
| hotspot state | fact | whether the hotspot is up, and its SSID |
| hotspot clients | reading | how many clients are joined |

`network-address` and the `interface` trait's `route` member already carry which link is carrying traffic, so nothing new is needed for that.

Signal strength was considered and left out. Worth revisiting if siting a device turns out to be a thing operators struggle with, since it is the obvious reading for "is here good enough" and VIEW would graph its history for free.

Open: whether VIEW gives these tiles, and what the configuration screen itself looks like.

## Implementation options

### What configures the network underneath

The device has to hand the configuration to something. Candidates:

- NetworkManager, over its D-Bus API. Handles wireless client, AP mode, wired addressing, DNS and connection priority itself, and is what a Raspberry Pi OS image ships with. Largest dependency, and its own model would have to be mapped onto ours.
- systemd-networkd with wpa_supplicant. Smaller and file-driven, but AP mode and the hotspot's DHCP server are more assembly.
- iwd with systemd-networkd. Modern and small, good WPA3 and enterprise support, weaker AP story.

Whichever it is, the device's own configuration document is the contract and the backend is not: the spec describes the document, not the tool.

Open: whether bliti owns these files outright, or merges with what an image already ships.

### Privileges

Changing network configuration needs more than the daemon has today.
Either the daemon runs privileged, or it holds specific capabilities, or it talks to something that does.

## Open questions

- [ ] Hotspot band, channel, address range, and client isolation.
- [ ] Is there a regulatory domain or country setting, and who sets it?
- [ ] Wired addressing: DHCP or static per interface, and what a device with several wired ports does.
- [ ] Are a link's DHCP-supplied resolvers used, ignored, or ranked against configured ones?
- [ ] What does a device do with a document setting its stack cannot honour: reject it, or apply the rest and report it unhonoured?
- [ ] Does VIEW give the new NFO entries tiles, and what does the configuration screen look like?
- [ ] **Blocking:** does the BLE session survive a network reconfiguration on target hardware?

## Trade-offs

**Secrets in full over a marker.** Returning secrets makes read-edit-write trivially correct and means a client can show an operator the hotspot password to read out. It also means every client that ever connects holds every key the device holds, and a screenshot of a configuration screen is a disclosure. The channel's authentication is what makes this defensible, and it is only defensible while that holds.

**One document over targeted operations.** Idempotence and no sequencing, at the cost of a client having to send back settings it did not mean to touch. That cost is what makes the secrets decision above load-bearing rather than incidental.

**Hotspot credentials outside the identity chain.** Everything else about a bliti device descends from its board ID, and deriving the hotspot's SSID and password would have fitted that, made them reproducible from the device alone, and let them be printed beside the QR code. They are operator-set instead, which means a device has no hotspot until someone configures one and a lost password is lost rather than recomputable. What it buys is that the hotspot password, the one credential here meant to be read aloud and handed to strangers, has no relationship to the chain the device's identity hangs off.

**One carve-out in "we do not set policy".** Supporting WPS PIN and refusing open networks sit oddly beside each other until the line is drawn in the right place: bliti does not judge how a site authenticates, but does require that the link be encrypted. That line is defensible and it is also the only one. Every further "this network is not good enough" would need the same argument made again, and the answer should usually be no.

**The device keeps no history.** Reverting to the last confirmed configuration, and never persisting a provisional one, means the device holds exactly one configuration on disk and needs no unwind stack, no window that must survive a reboot, and no reconciliation after a power cut. The cost moves to the client, which has to hold the attempt and re-offer it, and to the failure report, which has to be specific enough for that re-offer to be worth anything.

**Concurrent AP+STA where supported.** Honest about hardware, but it makes the device's capability part of the protocol: the client must be told what the device can do before it can offer a coherent configuration. Requiring concurrency outright would have been simpler and would have narrowed the hardware.

## Testing notes

- The BLE session survives a client connection change, a switch into AP mode, and AP+STA coming up together, on real target hardware.
- A device that joins each supported security type: WPA2-PSK, WPA3-SAE, transitional, enterprise, WPS push-button, WPS PIN.
- A second client opening a configuration session while one is open is refused, with a reason.
- A document asking for AP+STA on hardware that cannot do it is rejected, naming the conflict.
- A session abandoned without confirming (stream closed, channel dropped, device powered off) leaves the last confirmed configuration in force.
- A configuration that cannot work (wrong key, absent SSID) reverts, and the device reports both the attempt and the revert.
- A change the device cannot judge stays provisional and reverts when no confirmation arrives.
- The channel drops mid-window; the device behaves as specified rather than as an accident.
- Read the configuration, write it back unmodified, and observe that nothing changes.
- Hotspot with an upstream present and absent; sharing on and off.
- Hotspot and wireless client concurrently on hardware that supports it, and the fallback on hardware that does not.
- Ordering: the default route follows the ordering, and moves when a higher link comes back.
- NFO entries reflect the configuration in force, including immediately after a revert.
- A device whose backend is not running, or which lacks the privilege to configure anything, reports that rather than silently doing nothing.
