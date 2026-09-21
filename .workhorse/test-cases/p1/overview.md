# Test cases: make the feed model structural

Covers the wire shape of [MSG](../../specs/messages.md), the catalogue of [NFO](../../specs/device-info.md), and the rendering of [VIEW](../../specs/device-view.md).

## The envelope and the message set

- [x] One `hello` round-trips, and each end reads its peer's without either type being distinguished by name (verifies spec: MSG)
- [x] A `hello` carrying a critical member is a fault (verifies spec: MSG)
- [x] A known message type arriving at an end with nothing to do about it is a no-op: nothing is sent in reply, the stream stays open, and no fault is reported (verifies spec: MSG)
- [x] An unknown message type is skipped whole and silently (verifies spec: MSG)
- [x] An unrecognised critical member nested in a `reading` costs that reading and not the rest (verifies spec: MSG)
- [x] A `subscribe` arriving without its selector critical is a fault (verifies spec: MSG)

## Facts and readings

- [x] A `fact` and a `reading` of the same catalogue name are different entries and do not collide (verifies spec: NFO)
- [x] Every fact and reading carries a `status` trait, `fact` entries included (verifies spec: NFO)
- [ ] `network-throughput` reports `passed` and never `warning` or `failed`, however busy the link (verifies spec: NFO)
- [x] A numeric `value` is rounded to four decimal places on send (verifies spec: NFO)
- [x] A trait that only qualifies another sits inside it: `route` and `overlay` within `interface`, `device` and `role` within `filesystem` (verifies spec: NFO)

## Status

- [x] `passed`, `warning` and `failed` each carry a `value`; `skipped` and `broken` each carry none (verifies spec: NFO)
- [x] `reason` is present on every status but `passed`, and absent on `passed` (verifies spec: NFO)
- [x] The five wire strings are `passed`, `warning`, `failed`, `skipped` and `broken`, matching what bestool reports checks in (verifies spec: NFO)
- [x] **A measurement that errored and one never attempted are told apart.** An unreadable sensor reports `broken` and a platform that cannot answer reports `skipped`, each with its own reason (verifies spec: NFO)
- [x] A `reason` survives to the client as the device wrote it, rather than being matched against a table (verifies spec: VIEW)

## Telling entries apart

- [ ] Two readings sharing a catalogue name and differing in one trait are two series (verifies spec: VIEW)
- [ ] Two readings identical in name and traits are one series (verifies spec: VIEW)
- [ ] **A trait this build has never heard of still separates two series.** Feed a client two readings alike but for an unknown trait and assert it holds two histories, not one merged (verifies spec: VIEW)
- [ ] **A descriptive trait changing does not fork a series.** Move `route: default` from one interface to another and assert the first interface's history continues rather than starting again (verifies spec: VIEW)
- [ ] **A reading going into difficulty does not fork its series.** Drive a reading from `passed` to `warning` and back and assert one continuous history (verifies spec: VIEW)
- [ ] Trait member order does not affect series keying (verifies spec: VIEW)
- [ ] A v4 and a v6 address on one interface are two entries a reader can tell apart (verifies spec: NFO)

## Values

- [x] An unrecognised `kind` renders as the stringification of `value` followed by `unit` (verifies spec: VIEW)
- [x] An unrecognised `unit` is written out as sent (verifies spec: VIEW)
- [x] A `quantity` with neither `limits` nor a total is not drawn against a scale (verifies spec: VIEW)

## Topics and feeds

- [x] The device opens the `default` feed unprompted after the handshake, without being asked (verifies spec: MSG)
- [x] The feed carries no announcement of the topic it serves (verifies spec: MSG)
- [x] A client that declines by closing the stream still holds the device's name and version from `hello` (verifies spec: MSG)
- [x] A client that resumes with `subscribe` for `default` receives current data (verifies spec: MSG)
- [x] A `subscribe` for a topic already being served is skipped, and the client does not receive two copies (verifies spec: MSG)
- [ ] A `subscribe` for a topic the device does not know is skipped silently and its stream is left open (verifies spec: MSG)
- [x] Sampling continues across a decline, so a resume is served what is current rather than what accumulated (verifies spec: MSG)
- [x] A subscription ends on a reset and a dropped link exactly as on a graceful close, with neither end reporting a fault (verifies spec: MSG)

## The catalogue

- [x] Throughput is reported per interface and direction, never aggregated on the device (verifies spec: NFO)
- [x] Each filesystem is its own reading, with a boot partition marked by the `boot` role (verifies spec: NFO)
- [x] One filesystem is reported per block device, the shortest mount path winning (verifies spec: NFO)
- [x] Each temperature sensor is its own reading, and the `cpu` sensor carries the board's thresholds in `limits` (verifies spec: NFO)
- [ ] `cpu-frequency` reports `warning` when the platform reports throttling, and `passed` when the frequency is merely low (verifies spec: NFO)
- [ ] `last-boot` is omitted where the device cannot answer for the instant (verifies spec: NFO)
- [x] Hardware that is not fitted produces no entry at all (verifies spec: NFO)
- [x] Loopback and virtual interfaces are not reported (verifies spec: NFO)
- [x] `power-source` reports one of `via-backup`, `battery` or `bypassing-backup`, and `bypassing-backup` reports `warning` (verifies spec: NFO)
- [x] `battery-direction` reports one of `charging`, `discharging` or `idle`, and follows `power-source` where that reading exists (verifies spec: NFO)
- [x] `battery-direction` reports `skipped` with a reason until the voltage has been watched long enough, and reports plainly thereafter rather than carrying a standing warning (verifies spec: NFO)
- [x] A direction disagreeing with the power source reports `battery-charge` as `warning` (verifies spec: NFO)
- [x] Units are spelled out in full on the wire (verifies spec: NFO)

## Sampling

- [ ] Sampling begins at boot and stops after thirty minutes with no session open (verifies spec: NFO)
- [ ] The device holds the cell-voltage history its direction derivation needs, and holds no readings for replay (verifies spec: NFO)

## Rendering what the client knows

- [x] The header names the device from `hostname`, `board`, `os` and `kernel`, which get no tiles of their own (verifies spec: VIEW)
- [ ] The order is fixed, and a reading going to `warning` does not move it (verifies spec: VIEW)
- [ ] **Every recognised entry has a home.** `memory-total`, `filesystem-total`, `cpu-frequency-max`, `battery-voltage` and `battery-direction` each appear in the right reveal, and none is appended as unrecognised or tiled on its own (verifies spec: VIEW)
- [x] Storage headlines the fullest non-boot filesystem, and shows each filesystem in the reveal against its own total (verifies spec: VIEW)
- [x] Network headlines one combined figure, and the per-direction split appears only on tap (verifies spec: VIEW)
- [x] Per-interface throughput draws a mirrored graph per interface, each direction scaled to its own peak and labelled (verifies spec: VIEW)
- [x] **A failed network direction shows why.** A per-interface `out` reporting `broken` shows its reason in the reveal alongside the other interfaces' graphs (verifies spec: VIEW)
- [x] Addresses and throughput render as separate tiles (verifies spec: VIEW)
- [x] Address headlines the default-route address with the overlay address, headlines both where an interface holds a v4 and a v6, and shows every address in the reveal (verifies spec: VIEW)
- [ ] `last-boot` renders as an elapsed time (verifies spec: VIEW)
- [ ] A reveal shows each reading's own `limits`, status reason and scale (verifies spec: VIEW)
- [x] A `passed` face carries no colour, and a `skipped` face is distinguishable from a `broken` one (verifies spec: VIEW)

## Rendering what it does not

- [x] An unrecognised measurement renders from its own name, is appended after everything recognised, and still colours by `status` (verifies spec: VIEW)
- [x] An unrecognised fact renders as a tile rather than in the header (verifies spec: VIEW)
- [x] **Two unrecognised readings alike but for their traits render distinguishably**, with trait values as the qualifier rather than dropped (verifies spec: VIEW)
- [x] A history accumulates for an unrecognised `reading`, and none for an unrecognised `fact` (verifies spec: VIEW)

## Regressions this must not reintroduce

- [x] No `group`, `label`, `note`, `graph`, `detail`, `error` or ordering meaning anywhere on the wire
- [x] Nothing on the wire is a string the client parses for structure
- [x] No rendering rule sits in NFO; the catalogue binds every reader and the layout binds only ours
- [x] A graph fills forward from connection; no history is sent (U1 carries bringing it back)

## Manual, on real hardware

- [ ] The full view renders on a phone against a real device, with the device's own readings rather than fixtures
- [ ] Pulling mains from the UPS input moves `power-source` and `battery-direction` together
- [ ] Loading the network moves the per-interface graphs independently of each other
