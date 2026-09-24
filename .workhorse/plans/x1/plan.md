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
- [x] The capabilities shape, settled and written into NET, and the shared checker in `bliti-core` (`channel/capabilities.rs`): one walk resolving the selectors `kind`, `interface` and `band` by value
- [x] Capability probing on the device over nl80211 (`wl-nl80211`), in `network/probe.rs`: per radio its bands, channels, widths, AP support, `alongside`, SAE holdability, scan and survey, and the capabilities built from them in NET's shape. Hotspot channels are only those the renderer renders (`render::hotspot_channel`), so nothing is offered that the device then refuses. Also mappings to `select::Hardware` and `render::Hardware`, and nl80211 setters for the regulatory domain and the AP interface
  - [ ] wl-nl80211 0.7.0 parses HE capabilities one level too shallow, so widths come from HT and VHT alone and 6 GHz reports 20 MHz. Fix upstream before HE or 6 GHz matter
  - [x] Survey support is detected by a `GET_SURVEY` dump, which is read-only in nl80211 but retunes the radio on brcmfmac: on the Pi a dump takes 3.4 seconds, a tenth on each of its 32 channels. So it is asked once, at start, and a probe again after the country changes carries each radio's answer over. A survey act still retunes the radio for as long, which is what the operator asked for
  - [ ] Capabilities cannot say which widths hold on which channel (80 MHz offered on 5 GHz while channel 165 cannot carry it). Either key width under channel, or have the device refuse the pair at `$['hotspot']['channel-width']`
- [x] The session stream role and its message types, in `bliti-core` alongside the existing channel messages — `configure`, `configuration`, `applied`, `invalid`, `confirm`, `discard`, `busy`, and the acts `scan`, `survey`, `wps` with their answers `networks`, `spectrum`. The document, capabilities and act-answer payloads ride as raw JSON so they survive the envelope round trip and stay forward-compatible, mirroring how `Entry` carries `traits`
- [x] Wire compatibility: the new message types go through `bliti-wire-compat` (generator arms added), and `configuration`'s critical `DOCUMENT` is recorded in `wire-breaks.toml` with a reason
- [x] The device's configuration session, in `crates/bliti/src/network/session.rs`: one device-wide session behind a lock (`busy` otherwise), the recorded configuration held raw in one file replaced atomically on `confirm`, proposals applied through a `Backend` trait (`capabilities`, `check`, `apply`, `restore`, `scan`, `survey`, `wps`), and restore on discard, failure, and however the session ends. Runs on an `Inert` backend until the real one lands. The daemon restores the recorded configuration at start
  - [x] A session dropped from outside ends the streams it served (`JoinSet` and an abort-on-drop driver in `session::run`), so a configuration session cannot outlive a client that unsubscribed
  - [x] The recorded configuration of an unconfigured device: one `wired-dynamic` candidate per wired interface (CFG), read from `/sys/class/net`
  - [x] `state`, `pin`, capabilities on `applied` and on `state`, and the capabilities checked before a proposal or act reaches the backend
- [x] Render a document, for a given selection of candidates, as the files iwd, hostapd and networkd read, in `crates/bliti/src/network/render.rs`. Pure: no filesystem, processes or D-Bus. `Paths::owns` tells the applier which files are bliti's so it can delete stale ones
- [x] Candidate selection, in `network/select.rs`: a pure, event-driven selector placing candidates on interfaces and the hotspot on a radio by LINK and HOT, with per-candidate states and the changes to apply. No timers; a retry is an event the caller schedules
- [x] The event sources feeding it: carrier and addresses (rtnetlink), association and range (iwd over D-Bus), and the gateway answering
- [x] A real `Backend` joining the selector, renderer and applier. `--network-backend stack` chooses it; `Inert` stays the default until the image carries iwd and hostapd
- [x] Event-driven reselection on carrier, failure and a higher candidate returning
- [x] Put rendered files in place and have the stack pick them up, in `network/apply.rs`: atomic writes, stale bliti files removed, a record by digest of what was written so iwd's own rewrites are not drift, and regdom, hostapd, iwd, networkd, resolved picked up in that order. Real `System` over systemd and networkd D-Bus, with `iw` until nl80211 replaces it
  - [ ] Replace `apply/iw.rs` with the probe module's nl80211 setters. They are async and `System` is not, so the real `System` holds a runtime handle and blocks on them from the blocking pool apply runs on; then `iw` leaves `services/README.md`
- [ ] Apply and revert driven by a session, with nothing provisional surviving a reboot. Everything bliti renders lives under `/run` (`Paths::system()`, and iwd pointed there by its drop-in), so a power cut leaves nothing for the stack to bring up at the next boot, and bliti renders the recorded configuration, kept in `/var/lib/bliti`, as it starts. bliti therefore has to start before anything wants the network. Not checked on hardware: that emptying `StateDirectory=` in the drop-in lets `Environment=STATE_DIRECTORY` win
- [ ] Wireless joining: PSK, SAE, transitional, enterprise, WPS push-button and PIN
  - [x] A wireless candidate out of range is never tried again: with every network `AutoConnect=false` iwd does not scan by itself, and a retry of a candidate the radio does not hear changes nothing, so a network coming into range goes unnoticed. LINK forbids polling to detect it, so this needs a decision on what scans
  - [x] Reasons from iwd reach the client in iwd's words (`Operation failed (net.connman.iwd.Failed)` for a wrong passphrase); a refused key-based join now says the passphrase is most likely wrong
  - [ ] A refused passphrase is `invalid` at the candidate rather than at its `passphrase`, so the screen marks the candidate and not the field. Carrying a member path needs the selector's unavailable state to hold one
  - [x] An apply connects from iwd's cached scan results while its own scan runs, so a network that has just gone away is tried and fails with status 16 before the scan says it is out of range
- [x] Hotspot: bring-up, upstream sharing, client isolation, DHCP range (in code; each is still owed a check on hardware in the test cases)
- [x] The new NFO entries and their traits, in the sampler, as part of `Facts` (which is the sampler's `Source`, gathered on the blocking pool). The wireless network and hotspot are facts and belong on the slow tick; the client count is a reading and belongs on the fast one
- [x] Teach the web client that `security` and `channel` are descriptive traits, in `readings.js`, with the wireless and hotspot tiles placed where VIEW puts them
- [x] The configuration screen in the web app, following [NSCR](../../specs/network/screen.md): `Channel::configure` in the wasm crate, `Network.jsx` for the four stages, and `capabilities.js` as the only module that reads the capabilities shape
  - [x] The settled shapes on screen: the shared checker through wasm, adapter pickers, `state`, `pin`, capabilities on `applied` and `state`, scanning by SSID with hidden ones on request, one-adapter scans and the siting view
- [x] A session per connected client ([CHN](../../specs/channel.md), "Several clients at once"), so that only the configuration session is exclusive ([CFG](../../specs/network/session.md)). Both characteristics run over the sockets BlueZ acquires per client (`AcquireWrite`, `AcquireNotify`), which carry one client's writes in order and notify that client alone; the inbound sink routes by client address; one pacer spans every session; the device re-advertises as each session opens as well as when it ends
- [x] Entries that end: NFO's `ended` status, sent once more for a fact the feed no longer carries and a reading a slow tick no longer takes; the client drops the tile and its history
- [x] `network-configuration` in NFO, `provisional` from starting to apply until confirmed or restored, published by the configurator beyond the session holding it; VIEW's notice and trial marking
- [x] The configuration session kept across screens while a proposal applies or is applied, with the device view offering confirm, discard or review (NSCR)
- [x] Check whether capabilities offer the hotspot channels the firmware refuses. They do not. brcmfmac marks disabled in cfg80211 every channel the firmware's own list lacks (on the Pi 14, 34 to 46 and 144 to 165), and cfg80211 keeps them disabled across a change of domain, so the probe, which skips disabled channels, offers 1 to 13 and 36 to 140 under NZ. The firmware's country stays `99` because the Pi's device tree carries no `brcm,ccode-map`, so brcmfmac does not pass the domain on; opening 149 to 165 would take that map in the image. The `set_channel` refusals in the log came from scans or surveys walking channels cfg80211 did not yet mark disabled, in earlier boots; a survey dump on the current boot walks only enabled channels and logs nothing
- [ ] WPS for a chosen network: `wps` takes an optional `ssid` (CFG, WLAN); the device runs WPS as now and accepts the credentials only where they are for that SSID, discarding them and reporting the mismatch otherwise; the screen offers joining by WPS from a network in the scan list (NSCR). iwd's `SimpleConfiguration` takes no network or BSSID (`PushButton()`, `StartPin(s)`, checked on iwd 3.10), so the choice is enforced on the result rather than on the exchange. Push-button's session-overlap rule already aborts where two access points have the button pressed at once, and a PIN is targeted by the router it is entered on
- [ ] WPS push-button against a WPA2/WPA3 router: capture the credential iwd refuses, then decide whether a transitional or WPA3 credential can be joined as `psk` on a radio with SAE off, or the failure worded to say so
- [ ] A hotspot on a shared-channel radio chooses its own band, channel and width where no wireless candidate can take that radio, since there is then no client to follow. HOT forbids it today, omitting the three from the radio's capabilities outright. Needs: HOT rewritten to offer them conditionally; the capabilities to carry them with that condition (the mirror has no selector for it, so a cross-member rule beside the one-at-a-time placement rule, in `bliti-core` for device and client both); the renderer to use a chosen channel; the screen to offer them then and explain their absence once a wireless connection shares the radio. Do with the bring-up ordering below, which is the same code
- [ ] Order the shared-channel radio's bring-up as HOT has it, so the hotspot follows the wireless client: settle the station first, then start or move the hotspot onto its channel. Today a proposal carrying both starts the hotspot first, which pins the radio (see "A hotspot first pins the shared-channel radio" below). Covers the "wlan0 was disassociated" failure. Also decide whether iwd's own autoconnect of a known network is to be left racing the driver's join, and word a dropped link so it says why
- [ ] Two clients at once on the prototype: a phone and the laptop, each holding a session, the second told `busy` on the network settings while the first configures
- [x] Privileges: the daemon already runs as root (`services/bliti.service`, for its board-ID sources), which covers writing under `/run`, the D-Bus calls to systemd, networkd and resolved, and nl80211. Narrowing it to capabilities (`CAP_NET_ADMIN` plus polkit rules for the unit calls) is possible later and not needed for this card

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

`networks` reports one entry per access point (BSSID), hidden ones included with a null `ssid`, and `radio` carries the bands the radio can use, so a scan serves siting a new access point as well as joining. Following sign-off, the code catches up: `Message::Networks` carries `access-points` rather than `networks`, and the web screen's scan list groups by SSID and hides access points with no SSID by default.

The device session handler, the iwd/hostapd/networkd renderers, and the web screen are being built in parallel on local branches `x1-session`, `x1-render` and `x1-web`, and are cherry-picked onto the card branch as each lands. The web screen keeps all knowledge of the capabilities shape in one module so a change from review stays contained.

### What the renderer found about the stack

Findings from building the renderer, each binding on the applier or on capability reporting. None is checked on hardware yet.

- **Resolvers: a link's own ones go in a DNS delegate.** resolved routes a query to a link and uses that link's servers in turn, falling through only on an error, and "no such name" is not one. So a dynamic candidate's configured resolvers cannot share the link with the ones its network supplies. The link carries the supplied ones, with the site's search domains as routing domains (`UseDomains=route`) and `DNSDefaultRoute=no`; the candidate's own go in `/etc/systemd/dns-delegate.d/50-bliti-<interface>.dns-delegate`, bound to the link (`DNS=<server>%<interface>`) with `Domains=~.`. A dynamic link naming none keeps `DNSDefaultRoute=yes`, which routing domains would otherwise turn off. A static link has nothing supplied, so its resolvers sit on it. Needs systemd 258; the applier reloads resolved after networkd. Not checked on hardware: that a reload of resolved rereads delegates.
- **SAE can be held to SAE, where the radio allows it.** A `sae` candidate renders `TransitionDisable=true` with `DisabledTransitionModes=personal`. iwd silently ignores the pin on a radio lacking CCMP or BIP-CMAC, so capabilities should offer `sae` only where the radio has SAE, CCMP and BIP-CMAC, and the device should still check the negotiated key management after association. There is no per-network way to hold iwd to PSK only, so a `psk` candidate upgrades to SAE on a transitional access point, which still authenticates it. The one way to hold iwd to PSK is per driver, `[DriverQuirks] SaeDisable`, which bliti renders for brcmfmac (`render::SAE_DISABLED`).
- **iwd writes back into its known-network files**, so the applier cannot detect drift by comparing file contents.
- **iwd adopts every interface on the radio.** It must run with `--nointerfaces ap0` (a unit drop-in, not `main.conf`), or it takes over the hotspot's interface.
- **hostapd cannot follow another interface's channel.** On shared-channel hardware the renderer takes the station's current channel from the selection (20 MHz, falling back to 2.4 GHz channel 6 with no station), and the applier re-renders and restarts hostapd whenever the station changes channel, bringing the AP up before the station.
- **The CYW43455 firmware has a reported crash in station-plus-AP mode on kernel 6.12** ([raspberrypi/linux#7092](https://github.com/raspberrypi/linux/issues/7092)). The hardware test case above passed on the image in use, but the kernel it runs is worth pinning against this.
- **The regulatory domain takes three pieces**: iwd's `[General] Country=` hint, hostapd's `country_code` with `ieee80211d=1`, and `cfg80211 ieee80211_regdom` in modprobe.d, which only applies at module load. So the applier also runs `iw reg set` (the world domain `00` where unset) at runtime.
- **`share-upstream: false` needs systemd 256 or later**, where `IPv4Forwarding=` exists. Older systemd ignores the key. Worth confirming against the image's systemd.
- **The default hotspot range is `10.41.0.0/24`**: the device is `10.41.0.1` and hands out the rest.
- **Structural rules the renderer enforces that no spec states**: passphrases are 8 to 63 printable ASCII characters, SAE included; two wireless candidates for one SSID with the same kind of key are refused, since iwd keys its files by SSID; enterprise members a method does not use are refused, `phase2` is required for PEAP and TTLS, and `ca-certificate` and `domain` for PEAP, TTLS and TLS.

### Several radios

A wireless candidate and the hotspot may name an `interface`; unset, the device picks (LINK, HOT). Candidates take free radios in order, preferring the one that hears them best, and an unpinned hotspot prefers a radio carrying no wireless candidate. The capabilities proposal keys what differs by radio under `interface`, and carries `radios` keyed by interface with `model`, `bands` and `alongside`.

The document model carries `interface` already, and the renderer refuses any name but its one station interface, since `Hardware` describes one radio. What several radios still need:

- [ ] `Hardware` describing each radio (its station interface, the AP interface bliti creates on it, its `alongside`), and `Selection` saying which radio carries each active wireless candidate and the hotspot
- [ ] iwd's known networks are global to iwd rather than per interface, so a pin is enforced by bliti connecting that interface's station itself, with every network at `AutoConnect=false`
- [ ] Two candidates for one SSID differing only in `interface` share one iwd file. The renderer refuses a repeated SSID today; it should accept one where the credentials match
- [x] The radio assignment of LINK and HOT in candidate selection, re-run on the same events
- [x] `wireless-network` in NFO carries `interface`, distinguishing
- [x] `scan`, `survey` and `wps` take an optional `interface` on the wire and through the `Backend` trait; unset, scan and survey run on every radio able to
- [x] `scan` and `survey` entries carry the `interface` whose radio heard them, from the real backend
- [x] The screen offers scanning one adapter, so a technician can spare a radio carrying the uplink or the hotspot
- [x] The web screen picks an adapter per wireless candidate and for the hotspot, labelled by `model`
- [ ] The one-at-a-time placement rule of HOT lives in `select/check.rs` and again in the web client's `capabilities.js`. Move it into `bliti-core` beside the capabilities checker, reading `radios` and the `interface` keys, so the device and the client share one implementation
- [ ] The screen: a WPS adapter choice, a survey adapter choice, and the siting view counting every channel a wide access point spans (NSCR)

### What the prototype showed

Probed on `tamanu-iti-v4-prototype` (Raspberry Pi 5, Ubuntu 26.04, kernel 7.0.0-1017-raspi, systemd 259.5) on 23 September 2026, read-only apart from setting and restoring the regulatory domain on its unused radio.

- **Interface names are `end0` and `wld0`**, not `eth0` and `wlan0`. Nothing in bliti assumes either; hardware descriptions come from the probe and `/sys/class/net`.
- **The CYW43455 (`brcmfmac (SDIO 02d0:4345)`) is shared-channel**, as expected: its station-plus-AP combination has one channel.
- **It does WPA3-SAE by external authentication**: `NL80211_FEATURE_SAE` with only `CMD_CONNECT`, no `SAE_OFFLOAD`, CCMP-128 and BIP-CMAC present. iwd runs this (the FullMAC case of `wiphy_can_connect_sae`), but it fails against H2E, below, so SAE is off on brcmfmac.
- **The regulatory domain does take effect on brcmfmac**, through cfg80211 applying the global domain to its channel flags, though `phy#0` keeps reporting its own `country 99`. Setting NZ opened channel 13, dropped 14, and cleared the 5 GHz no-IR flags with the radar channels still marked.
- **Before anything sets a domain, the radio is unrestricted.** Straight after boot every channel read no-IR false, radar false, where the world domain should have held 5 GHz passive and 52 to 144 as radar; after an explicit `iw reg set 00` the flags were right. So an unset `regulatory-domain` has to be applied as `00` explicitly, never left to boot, which is what the applier does.
- **The channel widths the probe derives look too wide**: channel 36 read 160 MHz under NZ, whose 5150 to 5250 MHz range allows 80. Harmless while the hotspot never renders past 80, but the derivation from the no-80 and no-160 flags wants checking.
- **The image is not owned outright yet.** netplan renders `/run/systemd/network/10-netplan-all-en.network`, which sorts ahead of bliti's `50-bliti-*` and would win for `end0`, and wpa_supplicant is running. The image has to drop both when bliti takes the network over. iwd is not installed.
- **The management path is Tailscale over `end0`**, so experiments on the device stay on `wld0` until bliti is trusted to run `end0`.
- **iwd's drop-in works as written**: iwd runs with `--nointerfaces ap0`, `STATE_DIRECTORY=/run/bliti/iwd` and `CONFIGURATION_DIRECTORY=/run/bliti/iwd-config`, the unit's own directories emptied. iwd 3.10 from Ubuntu 26.04.
- **The real `System` works**: `bliti network-apply` set the domain with `iw`, restarted iwd over systemd's D-Bus and reloaded networkd, every file landing under `/run`.
- **WPA2 joins end to end** against the laptop's access point once it offered WPA2 alone: iwd from bliti's `.psk` in `/run`, networkd's DHCP from bliti's `.network`, the gateway answering.
- **WPA3-SAE by hunting-and-pecking joins** over brcmfmac's external authentication (hostapd with `sae_pwe=0`).
- **WPA3-SAE fails wherever the access point offers H2E.** iwd sees the access point is H2E-capable and runs H2E, sending commit and confirm through external authentication; the access point reports the Pi "indicates support for SAE H2E, but did not use it" and rejects it as a downgrade, and iwd reports status 16. The commit appears to leave the Pi carrying status 0 rather than 126, which points at the firmware's external-authentication path. A WPA2 candidate fails the same way on a transitional access point offering SAE with H2E, since iwd prefers SAE: that is most current routers, and every 6 GHz network.
- **So SAE is off on brcmfmac.** iwd's `main.conf` carries `[DriverQuirks] SaeDisable=brcmfmac`, and the probe withholds `sae` and `psk-sae` from radios on a driver in `render::SAE_DISABLED`, so capabilities offer only `psk` and `enterprise` on the Pi. A `psk` candidate then joins the laptop's transitional access point (WPA-PSK, PSK-SHA256 and SAE with H2E, PMF optional) over WPA2-Personal, addressed by DHCP and the gateway answering. The cost is that the Pi cannot join a WPA3-only network. Whether SAE can come back is card E2.
- **After a failed attempt iwd asks for the passphrase** although the known network still holds it, until one is given through the agent (`iwctl --passphrase`). The backend has to register an iwd agent that answers from the document, not rely on the file alone.
- **The stack backend runs the Pi** (`bliti daemon --network-backend stack`, driven from the laptop with `bliti configure` over BLE). With nothing recorded it falls back to `end0` wired-dynamic from its own `50-bliti-end0.network`, netplan's file gone and Tailscale undisturbed. A proposal carrying the laptop's network is applied with the wireless candidate `up`, takes the default route when ranked first and leaves it to `end0` when ranked second, and a discard reverts to the fallback. A wrong passphrase reports `association`, and an absent SSID reports `carrier` as out of range.
- **iwd's agent is needed, and bliti now is one** (`observe/iwd/agent.rs`). Without it the retry of a wrong passphrase failed with `NoAgent`; with it the retry fails on the passphrase, and a corrected proposal on the same session joins.
- **dbus hands a signal to the first match alone** unless `set_signal_match_mode(true)`, so iwd's watch starved every scan of its `Scanning` change and each wireless apply waited out the 30-second scan limit. A scan refused as busy while iwd connects now takes what iwd already hears rather than waiting for a scan that is not coming.
- **A better network coming into range is found.** With the wireless candidate ranked first and its network off at apply, the sweep's first scan heard it once it came up, and the candidate verified and took the default route from `end0`. After a failed join from iwd's cached results with the access point off, the reason blamed the passphrase; it now rescans first, and a network gone is out of range at `carrier`.
- **Per-candidate verification on the prototype**: a wrong passphrase beside `end0` with `verify` true is `invalid` at the wireless candidate, `association`, with the reason naming the passphrase; with `verify` false the same document is applied and the candidate reported unavailable; an absent network is `invalid` at `carrier`, out of range.
- **The laptop's Bluetooth drops when its access point starts or stops** on the same card, ending the session (the device reverts, as it should), and can leave BlueZ holding a stale connection that fails the next sessions straight after the handshake until it is disconnected. Repeated scans on the Pi's own combo radio held a session throughout, so the drops are the test rig's. Toggle the access point between sessions, not during one.
- **Installing iwd renames the radio.** iwd ships `/usr/lib/systemd/network/80-iwd.link`, which keeps the kernel's name for wireless interfaces, so after the first boot with iwd installed the Pi's radio is `wlan0` rather than the predictable `wld0`. A document naming an interface is only as good as the name staying put, so the image has to settle the naming policy (keep iwd's link file, or override it with bliti's own) before a device records anything.
- **The dead-man switch works**: every change made for the stack run lived under `/run` (bliti's drop-in, netplan's file removed), and a `systemd-run --on-active` reboot put the Pi back on netplan and the image's bliti.
- **Two default routes at metric 100** while netplan still holds `end0` and bliti's rank-0 wireless candidate is up. Goes once bliti owns `end0` too.
- **A second client took the first one's session.** With notifications on `StartNotify`, BlueZ calls it for every subscriber and sends each notification to all of them, and the one inbound sink went to whichever session opened last, so a laptop connecting ended a phone's session and failed its own handshake. BlueZ 5.85 keeps a notify socket per client under `AcquireNotify` and sends on it to that client alone, which is what lets sessions stand side by side.
- **Writes were taken up out of order.** bluer runs each `WriteValue` call as its own task, so writes a client makes without waiting for a response can reach the session reordered; the CLI client's session failed with a decrypt error just after the handshake. Under `AcquireWrite`, BlueZ 5.85 sends every write from a client, with or without a response, down one socket for that client, in order.
- **Setting the country failed a network heard well as out of range.** The new domain rewrites iwd's main configuration and the applier restarts iwd, which aborted the proposal's scan and emptied what iwd heard; the driver read the emptied list as the network gone. Only a finished scan takes a network out of range now, a render that restarted iwd has the joins wait for a scan after it, and a station iwd has not brought back yet is waited for. Rechecked with a wrong passphrase: refused at association, the passphrase named.
- **A renamed wired candidate failed at its gateway.** Any change to the candidate an interface last brought up marked its lease and gateway stale, awaiting announcements networkd never makes for a new label. Only what a candidate puts on the link counts now.
- **A trial hotspot's tile outlived it.** Leaving the network settings reverted the proposal as CFG requires, hostapd stopped at once, and the tile stayed until a reload: NFO had the device omit an entry that ends, which a reader already holding it cannot see. Hence `ended`.
- **A hotspot first on the shared-channel radio drops a 5 GHz join.** A proposal carrying a hotspot and a wireless candidate for a network on 5 GHz brought `ap0` up on 2.4 GHz and restarted iwd. iwd autoconnected to the known network on its own, on a 5 GHz BSS, and while hostapd ran logged locally generated deauthentications (`reason: 0, from_ap: false`) twice, most likely the firmware refusing a 5 GHz station beside a 2.4 GHz access point on its one channel. The driver reported it as "wlan0 was disassociated" at association. Not the cause, though logged alongside it: the firmware refuses channels 34 to 46 and 144 to 165 (`brcmf_set_channel ... fail, reason -52`) with or without a hotspot, after the domain is set to NZ, so iwd's scans skip them; access points on 36 to 48 are heard. Probably the firmware's own country (`99`) not following cfg80211's. The scan heard only on 2.4 GHz once with a hotspot applied is not explained by either.
- **WPS push-button fails with "No usable credentials obtained".** The exchange with the router completed and iwd refused what it received, which it does where no credential matches a network it hears with that security. Suspected: the router hands a WPA3 or transitional credential and iwd has SAE off on brcmfmac. Needs iwd's debug log of an exchange (iwd `-d` in bliti's drop-in, the router's button pressed during it) to see the credential's auth type.
