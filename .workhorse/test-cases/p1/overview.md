# Test cases: make the feed model structural

Covers the wire shape of [MSG](../../specs/messages.md) and the catalogue and rendering of [NFO](../../specs/device-info.md).

## The envelope and the message set

- [ ] One `hello` round-trips, and each end reads its peer's without either type being distinguished by name (verifies spec: MSG)
- [ ] A `hello` carrying a critical member is a fault (verifies spec: MSG)
- [ ] A known message type arriving at an end with nothing to do about it is a no-op: nothing is sent in reply, the stream stays open, and no fault is reported (verifies spec: MSG)
- [ ] An unknown message type is skipped whole and silently (verifies spec: MSG)
- [ ] An unrecognised critical member nested in a `reading` costs that reading and not the rest (verifies spec: MSG)
- [ ] A `subscribe` arriving without its selector critical is a fault (verifies spec: MSG)

## Facts and readings

- [ ] A `fact` and a `reading` of the same catalogue name are different entries and do not collide (verifies spec: NFO)
- [ ] A `fact` carrying `state`, `state-reason` or `limits` is rejected (verifies spec: NFO)
- [ ] A message carrying both `value` and `error` is rejected, as is one carrying neither (verifies spec: NFO)
- [ ] A `reading` carrying `error` has `state` of `fault` (verifies spec: NFO)
- [ ] `state-reason` is absent wherever `state` is `ok` (verifies spec: NFO)
- [ ] A numeric `value` is rounded to four decimal places on send (verifies spec: NFO)
- [ ] A trait that only qualifies another sits inside it: `route` and `overlay` within `interface`, `device` and `role` within `filesystem` (verifies spec: NFO)

## Identity

- [ ] Two readings sharing a catalogue name and differing in one trait are two series (verifies spec: NFO)
- [ ] Two readings identical in name and traits are one series (verifies spec: NFO)
- [ ] **A trait this build has never heard of still separates two series.** Feed a client two readings alike but for an unknown trait and assert it holds two histories, not one merged (verifies spec: NFO)
- [ ] **A descriptive trait changing does not fork a series.** Move `route: default` from one interface to another and assert the first interface's history continues rather than starting again (verifies spec: NFO)
- [ ] Trait member order does not affect identity (verifies spec: NFO)

## Values

- [ ] An unrecognised `kind` renders as the stringification of `value` followed by `unit` (verifies spec: NFO)
- [ ] An unrecognised `unit` is written out as sent (verifies spec: NFO)
- [ ] A `quantity` with no scale is not drawn against one (verifies spec: NFO)

## Topics and feeds

- [ ] The device opens the `default` feed unprompted after the handshake, without being asked (verifies spec: MSG)
- [ ] The feed carries no announcement of the topic it serves (verifies spec: MSG)
- [ ] A client that declines by closing the stream still holds the device's name and version from `hello` (verifies spec: MSG)
- [ ] A client that resumes with `subscribe` for `default` receives current data (verifies spec: MSG)
- [ ] A `subscribe` for a topic already being served is skipped, and the client does not receive two copies (verifies spec: MSG)
- [ ] A `subscribe` for a topic the device does not know is skipped silently and its stream is left open (verifies spec: MSG)
- [ ] Sampling continues across a decline, so a resume is served what is current rather than what accumulated (verifies spec: MSG)
- [ ] A subscription ends on a reset and a dropped link exactly as on a graceful close, with neither end reporting a fault (verifies spec: MSG)

## The catalogue

- [ ] Throughput is reported per interface and direction, never aggregated on the device (verifies spec: NFO)
- [ ] Each filesystem is its own reading, with a boot partition marked by the `boot` role (verifies spec: NFO)
- [ ] One filesystem is reported per block device, the shortest mount path winning (verifies spec: NFO)
- [ ] Each temperature sensor is its own reading, and the `cpu` sensor carries the board's thresholds in `limits` (verifies spec: NFO)
- [ ] `cpu-frequency` carries `state-reason: throttled` when the platform reports throttling, and does not when the frequency is merely low (verifies spec: NFO)
- [ ] `last-boot` is omitted where the device cannot answer for the instant (verifies spec: NFO)
- [ ] Hardware that is not fitted produces no entry at all; hardware fitted but unreadable produces one carrying `error` (verifies spec: NFO)
- [ ] Loopback and virtual interfaces are not reported (verifies spec: NFO)
- [ ] A bypassed backup supply reports `warn` with `state-reason: backup-bypassed` (verifies spec: NFO)
- [ ] A derived battery direction carries `state-reason: derived`, and is withheld until there is enough history (verifies spec: NFO)
- [ ] Units are spelled out in full on the wire (verifies spec: NFO)

## Rendering what the client knows

- [ ] Identity renders as a header, not as tiles (verifies spec: NFO)
- [ ] The order is fixed, and a reading going to `warn` does not move it (verifies spec: NFO)
- [ ] Storage headlines the fullest non-boot filesystem, and shows each filesystem in the reveal with its own state (verifies spec: NFO)
- [ ] Network headlines one combined figure, and the per-direction split appears only on tap (verifies spec: NFO)
- [ ] Per-interface throughput draws a mirrored graph per interface, each direction scaled to its own peak and labelled (verifies spec: NFO)
- [ ] **A failed network direction shows why.** A per-interface `out` carrying `error` shows its reason in the reveal alongside the other interfaces' graphs (verifies spec: NFO)
- [ ] Addresses and throughput render as separate tiles (verifies spec: NFO)
- [ ] Address headlines the default-route address with the overlay address, and shows every address in the reveal (verifies spec: NFO)
- [ ] `last-boot` renders as an elapsed time (verifies spec: NFO)
- [ ] A reveal shows each reading's own `limits`, `state-reason` and error reason (verifies spec: NFO)

## Rendering what it does not

- [ ] An unrecognised measurement renders from its own name, is appended after everything recognised, and still colours by `state` (verifies spec: NFO)
- [ ] An unrecognised fact renders as a tile rather than in the header (verifies spec: NFO)
- [ ] **Two unrecognised readings alike but for their traits render distinguishably**, with trait values as the qualifier rather than dropped (verifies spec: NFO)
- [ ] A history accumulates for an unrecognised `reading`, and none for an unrecognised `fact` (verifies spec: NFO)

## Regressions this must not reintroduce

- [ ] No `group`, `label`, `note`, `graph`, `detail` or ordering meaning anywhere on the wire
- [ ] Nothing on the wire is a string the client parses for structure
- [ ] A graph fills forward from connection; no history is sent (U1 carries bringing it back)

## Manual, on real hardware

- [ ] The full view renders on a phone against a real device, with the device's own readings rather than fixtures
- [ ] Pulling mains from the UPS input moves `power-source` and `battery-direction` together
- [ ] Loading the network moves the per-interface graphs independently of each other
