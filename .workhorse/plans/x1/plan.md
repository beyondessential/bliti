# Network configuration module with wifi and hotspot

Implementation notes and build order for the network configuration module.
Behaviour is specified under [NET](../../specs/network/overview.md) and its siblings.
The reasoning that produced them is in the working doc at `.workhorse/working-docs/x1/working-doc.md`.

## Open decisions

Two decisions are deliberately not in the specs, because neither constrains anything an operator can observe. Both want settling before the device half is built.

- [ ] **Which backend configures the network.** Candidates below.
- [ ] **Whether bliti owns the backend's configuration files outright, or merges with what the image already ships.**

## What configures the network underneath

The device hands the configuration to something. Candidates:

- **NetworkManager**, over its D-Bus API. Handles wireless client, AP mode, wired addressing, DNS and connection priority itself. Largest dependency, and its model would have to be mapped onto ours. The board this targets runs Ubuntu rather than Raspberry Pi OS and has never had NetworkManager installed, so this is a package to add rather than one already present.
- **systemd-networkd with wpa_supplicant.** Smaller, file-driven, and already what the image runs. AP mode is the assembly, since it means driving hostapd; the hotspot's DHCP server is not, because networkd carries its own.
- **iwd with systemd-networkd.** Modern and small, good WPA3 and enterprise support, weaker AP story.

Whichever it is, the document of [NET](../../specs/network/overview.md) is the contract and the backend is not.

Where our own model has no strong opinion, take the shape from netplan rather than inventing one: it already covers wired addressing, per-link resolvers, wireless credentials and access-point mode in a declarative document close to what this needs. Nothing defers to it at runtime.

## Device-side packages

The packages a device needs at runtime are recorded in [services/README.md](../../../services/README.md), beside the unit file and the bluetoothd configuration, so a Debian package can declare them when there is one to declare them in.

That list forks on the backend choice: the first option costs nothing the image does not already carry, and the other two each add a package.

## Privileges

Changing network configuration needs more than the daemon holds today. Either it runs privileged, it holds specific capabilities, or it talks to something that does.

## Capability reporting

Capability reporting is load-bearing rather than incidental, because it is the only thing standing between a client and an invalid document, and it is what removes partial application as an outcome. A capability the device forgets to mention becomes a feature nobody can use.

Three come straight from the hardware and have to be probed rather than assumed:

- whether the radio runs AP and STA concurrently
- whether it constrains them to one channel when it does
- which WPS methods it offers

On the board this targets (Cypress CYW43455) the answers are yes, yes, and both. Its interface combinations allow one AP plus one managed interface on a single shared channel.

## Trade-offs already made

**Secrets in full rather than a marker.** Read-edit-write is trivially correct and a client can show an operator the hotspot passphrase to read out. The cost is that every client that connects holds every key the device holds, and a screenshot of a configuration screen is a disclosure. The channel's authentication is what makes this defensible, and only for as long as that holds.

**One document rather than targeted operations.** Idempotence and no sequencing, at the cost of a client sending back settings it did not mean to touch. That cost is what makes the secrets decision load-bearing rather than incidental.

**Hotspot credentials outside the identity chain.** Deriving them from the board ID would have made them reproducible from the device alone and printable beside the QR code. Operator-set instead, so a device has no hotspot until someone configures one and a lost passphrase is lost rather than recomputable. What it buys is that the one credential meant to be read aloud has no relationship to the chain the device's identity hangs off.

**Capability up front rather than applied-in-part.** What the device runs is always what was written. The cost lands on capability reporting, as above, and on the client, which cannot render a configuration screen without first asking the device what it is.

**One ordering of candidates rather than of links.** Site-switching comes free and reuses the gateway probe verification already needed. It also means everything about selection rests on a gateway answering, and that an operator has to understand a list where several entries concern one physical port.

**Authentication rather than encryption as the line for joining.** Encryption looked equivalent and is not, since an unauthenticated encrypted link still lets an impersonating access point read and rewrite everything. The cost is real: a device cannot be deployed at a site running an open guest network and nothing else.

**The device keeps no history.** One configuration on disk, no unwind stack, nothing to reconcile after a power cut. The cost moves to the client, which holds the attempt and re-offers it, and to the failure report, which has to be specific enough for that to be worth anything.

## Build order

- [ ] Settle the backend choice and the file-ownership question above
- [ ] Model the configuration document and its serialisation in `bliti-core`
- [ ] Capability probing on the device: AP+STA, shared channel, WPS methods, survey support
- [ ] The session stream role and its message types, in `bliti-core` alongside the existing channel messages
- [ ] Wire compatibility: the new message types go through `bliti-wire-compat`, and any critical member is recorded in `wire-breaks.toml` with a reason
- [ ] Candidate selection and the four verification stages on the device
- [ ] Event-driven reselection on carrier, failure and a higher candidate returning
- [ ] Apply and revert against the chosen backend, with nothing provisional written to disk
- [ ] Wireless joining: PSK, SAE, transitional, enterprise, WPS push-button and PIN
- [ ] Hotspot: bring-up, upstream sharing, client isolation, DHCP range
- [ ] The new NFO entries and their traits, in the sampler
- [ ] The configuration screen in the web app, following [NSCR](../../specs/network/screen.md)
- [ ] Privileges: whichever of the three options above is chosen

## Notes

The version marker of [VER](../../specs/version.md) covers the message encoding and envelope of [MSG](../../specs/messages.md), not the message types a feature adds. This module adds a stream role and message types without changing anything the marker covers, so the marker does not move.
