# Network configuration module with wifi and hotspot

Implementation notes and build order for the network configuration module.
Behaviour is specified under [NET](../../specs/network/overview.md) and its siblings.
The reasoning that produced them is in the working doc at `.workhorse/working-docs/x1/working-doc.md`.

## Open decisions

Two decisions are deliberately not in the specs, because neither constrains anything an operator can observe. Both are now settled.

- [x] **Which backend configures the network.** Settled: **iwd** for the wireless client, **hostapd** for the hotspot, **systemd-networkd** for addressing, per-link DNS and the hotspot's DHCP server. iwd carries the strongest WPA3-SAE, transitional and 802.1X-enterprise support of the three and offers WPS, over a clean D-Bus API; its AP mode is the weak one, so hostapd runs the hotspot, and networkd (which iwd already leans on for addressing) carries the wired candidates and the DHCP server. On shared-channel single-radio hardware the STA vif is iwd's and the AP vif hostapd's, both addressed by networkd. Not installed on the prototype image today, but images are rebuilt as part of this work; the Pi image is expected to ship with bliti. A revisable backend detail: the [NET](../../specs/network/overview.md) document is the contract, not the backend.
- [x] **Whether bliti owns the backend's configuration files outright, or merges with what the image already ships.** Settled: **own outright.** bliti writes the iwd/hostapd/networkd configuration wholesale from the document, so what a device runs always equals what was accepted.

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

- [x] Settle the backend choice and the file-ownership question above
- [x] Model the configuration document and its serialisation in `bliti-core` — `channel/config.rs`: `Document`, `Attachment` (wireless/wired-dynamic/wired-static), `Wireless`, `Security`, `Hotspot`, with `from_json`/`to_json` and the structural validation LINK/WLAN/HOT pin (required members, security kinds, a wired-static without a gateway rejected, at most one wired-dynamic per interface). Capability-dependent validation is left for the device layer, which owns the capabilities shape
- [ ] Capability probing on the device: AP+STA, shared channel, WPS methods, survey support. **This layer owns the capabilities wire shape** — the specs pin what must be reported (concurrency case, WPS methods, survey) but not the member names. Decide it here, and type it in `bliti-core` so the client's pre-proposal validation of NSCR shares it
- [x] The session stream role and its message types, in `bliti-core` alongside the existing channel messages — `configure`, `configuration`, `applied`, `invalid`, `confirm`, `discard`, `busy`, and the acts `scan`, `survey`, `wps` with their answers `networks`, `spectrum`. The document, capabilities and act-answer payloads ride as raw JSON so they survive the envelope round trip and stay forward-compatible, mirroring how `Entry` carries `traits`
- [x] Wire compatibility: the new message types go through `bliti-wire-compat` (generator arms added), and `configuration`'s critical `DOCUMENT` is recorded in `wire-breaks.toml` with a reason
- [ ] Candidate selection and the four verification stages on the device
- [ ] Event-driven reselection on carrier, failure and a higher candidate returning
- [ ] Apply and revert against the chosen backend, with nothing provisional written to disk
- [ ] Wireless joining: PSK, SAE, transitional, enterprise, WPS push-button and PIN
- [ ] Hotspot: bring-up, upstream sharing, client isolation, DHCP range
- [ ] The new NFO entries and their traits, in the sampler, as part of `Facts` (which is the sampler's `Source`, gathered on the blocking pool). The wireless network and hotspot are facts and belong on the slow tick; the client count is a reading and belongs on the fast one
- [ ] Teach the web client that `security` and `channel` are descriptive traits, in `readings.js`
- [ ] The configuration screen in the web app, following [NSCR](../../specs/network/screen.md)
- [ ] Privileges: whichever of the three options above is chosen

## Notes

The version marker of [VER](../../specs/version.md) covers the message encoding and envelope of [MSG](../../specs/messages.md), not the message types a feature adds. This module adds a stream role and message types without changing anything the marker covers, so the marker does not move.

### The client's descriptive-trait list is not automatic

`readings.js` holds the descriptive traits as two hardcoded tables: `DESCRIPTIVE` for wholly descriptive traits, and `DESCRIPTIVE_MEMBERS` for traits only some of whose members describe. Both `security` and `channel` are wholly descriptive under [NFO](../../specs/device-info.md), so both belong in the first.

This matters more than it looks. `identityKey` keys the tile grid as well as the history, and it is built from the distinguishing traits, so a trait the client does not know to be descriptive becomes part of an entry's identity. A `wireless-network` fact whose channel changed would key differently and appear as a second tile beside the first rather than replacing it, which is exactly what the shared-channel behaviour of [HOT](../../specs/network/hotspot.md) causes whenever the hotspot follows a client onto a new channel.

### Large responses are paced, not dropped

[CHN](../../specs/channel.md) caps notification payload at a byte ceiling per second and requires a device that reaches it to hold the remainder rather than discard it. A scan across a busy site, or a spectrum survey, can be large enough to meet that ceiling. Nothing is lost, but a client waiting on `networks` or `spectrum` may wait longer than the device took to gather it, and must not read the delay as a failure.

### Wire shape under review

The capabilities shape and the vocabularies the specs leave open are drafted in the configuration session wire shape mockup (`.workhorse/design/mockups/x1/wire-shape.html`). In brief: `capabilities.document` mirrors the document (absent means not offered, `true` any value, an array exactly these, an object constrained member by member), `capabilities.radio.alongside` carries the concurrency fact HOT requires, and `capabilities.acts` is keyed by message type. Because the mirror rule is generic, one checker in `bliti-core` can serve the device's rejection and the client's pre-proposal check.

`reached` names the stage an attempt stopped at, so a failure at `carrier` is told apart from a fault found before anything was applied.

The mockup also lists five gaps needing spec edits once settled: a `state` message for per-candidate state (NSCR needs it and no CFG message carries it), what `wps` is answered with, capabilities that change with the country, the recorded configuration of an unconfigured device, and holding an `sae` candidate to SAE.

The device session handler, the iwd/hostapd/networkd renderers, and the web screen are being built in parallel on local branches `x1-session`, `x1-render` and `x1-web`, and are cherry-picked onto the card branch as each lands. The web screen keeps all knowledge of the capabilities shape in one module so a change from review stays contained.
