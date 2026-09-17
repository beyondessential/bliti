# Diagnostic and informational data display

The first feature carried over the envelope E1 landed: the device reports what it knows about itself, and the application shows it to an operator standing in front of the hardware.

Behaviour is specified in [BLI-SYS](../../specs/system-info.md). The reasoning behind the decisions lives in `.workhorse/working-docs/d1/working-doc.md`. This plan executes them.

Two things carry the whole feature: a reading format generic enough that a client renders readings it has never heard of, and a device-side buffer so a graph is populated the moment it appears.

## Reading format in the core

The self-describing reading is wire contract, so it lives in `bliti-core` and both ends use the same types.

- [ ] Add reading and value types to `crates/bliti-core/src/channel/messages.rs`, or a new `readings.rs` beside it if that file grows past comfort: `Reading` with `name`, `label`, `value`, `detail`, `note`, `state`, `limits`, `group`, `direction`, `error`; `Value` as a tagged enum over `fraction`, `quantity`, `duration`, `text`.
- [ ] Skip absent optional members on the wire rather than sending nulls.
- [ ] An unrecognised `kind` deserialises to an unreadable value rather than failing the message, so a newer device does not blank an older client's whole report.
- [ ] Replace `DeviceMessage::Identity { hostname, addresses }` with `SystemIdentity { readings }`. Nothing has shipped to the field, so this is a replacement rather than a migration.
- [ ] Add `SystemSample { at, readings }` and `SystemHistory { samples }`.
- [ ] Round-trip tests for every value kind, and a test that an unknown kind survives a round trip through an older reader.

## Gathering the readings

`crates/bliti/src/facts.rs` currently gathers hostname and addresses for the prototype identity message. It grows into the reading source, and its module docs need rewriting away from the LCD framing.

- [ ] Static readings: `hostname` from the kernel, `board` from `/proc/device-tree/model` plus the `Revision` line of `/proc/cpuinfo`, falling back to DMI vendor/product/version, and `os` from `/etc/os-release`.
- [ ] `address` readings, one per interface holding an address, grouped as `network`. Keep the existing loopback and link-local filtering.
- [ ] `cpu` from `/proc/stat` deltas between samples, not a point read.
- [ ] `memory` from `/proc/meminfo`, using available rather than free.
- [ ] `disk` one reading per block device. Deduplicate by device before building readings, so `/` and `/var/lib/postgresql` on one `/dev/mapper/root` are one reading.
- [ ] `temperature` from `thermal_zone0`, with the NVMe hwmon under `detail`, and `limits` from the zone's declared trip points.
- [ ] `power` from `hwmon/rpi_volt/in0_lcrit_alarm` for undervoltage and `scaling_cur_freq` against `cpuinfo_max_freq` for frequency capping. The Pi firmware throttle bitmask is not reachable: `vcgencmd` is absent and there is no sysfs node for it.
- [ ] `fan` from `hwmon/pwmfan/fan1_input`.
- [ ] `uptime` from `/proc/uptime`.
- [ ] `network-in` and `network-out` from `/proc/net/dev` counter deltas, one pair per reported interface, grouped as `network` with `direction` set. Handle counter wrap.
- [ ] Battery from the fuel gauge on I2C bus 1 at `0x36`: state of charge as the headline, voltage under `detail`. There is no kernel driver bound and no `upower`, so this is a raw register read. Confirm the exact part before trusting register semantics beyond voltage and charge.
- [ ] Battery direction derived from the buffered history, with the `note` saying it is derived. Report no direction until there is enough history for it to be steady.
- [ ] A source that is absent omits its reading; a source that is present but fails produces a reading with `error`. These are different paths and both need covering.

Sources are Pi-specific where the Pi is what we ship, but every reading needs to degrade to something on a development laptop, because that is where most of the view gets built.

## Sampling and the buffer

- [ ] A sampler task holding about five minutes at full resolution, oldest discarded first, no coarsening.
- [ ] Split cadence by volatility: `cpu` and the network readings fast, the rest every few seconds. The device owns this policy and the wire carries no interval.
- [ ] Start sampling at daemon start and on session open; stop after thirty minutes with no session open.
- [ ] `at` is milliseconds since boot, monotonic. A field device may have no set clock, so nothing may depend on wall time.
- [ ] Tests over time: the window bounds, eviction order, that the stop timer fires and that a session restarts it.

## Serving the topic

- [ ] Handle a `subscribe` for topic `system` in `crates/bliti/src/session.rs`, one task per subscription stream.
- [ ] Send `system-history` first, then `system-sample` at cadence, until the stream ends.
- [ ] `system-identity` on the reporting stream after `device-hello`, re-sent when it changes. This replaces the two-second address poll.
- [ ] Unknown topics are already skipped by the envelope; confirm nothing here changes that.

## Rendering

The view is where the feature is, and it renders from the format rather than from a list of known readings.

- [ ] Generic tile: `label` and headline value on the face, everything else behind the tap.
- [ ] Value formatting per kind, including an unreadable kind falling back to the label alone.
- [ ] `state` colours the face and adds no element to it.
- [ ] Group readings sharing `group` into one tile. A client that ignored `group` would show several tiles and still be correct, so this is an improvement rather than a requirement.
- [ ] Bars only where there is a scale: a `fraction`, or a `quantity` carrying `max`. Draw `limits` as marks.
- [ ] Hold history from `system-history` and extend it with each sample. Spacing comes from `at`, not from sample count.
- [ ] Mirrored graph for two grouped readings with opposed `direction`, each side scaled to its own peak with the peak stated.
- [ ] Hand-rolled SVG. No charting dependency.
- [ ] Readings carrying `error` shown as failing with the reason, distinct from a reading that never arrived.

## Recognised readings

Bespoke treatment sits on top of generic rendering and never replaces it. Each of these must still render with recognition removed.

- [ ] `temperature` drawn against its `limits`, with the note shown.
- [ ] `battery` showing the derived direction and its caveat.
- [ ] `network-in` and `network-out` mirrored.

## Mockup

`.workhorse/design/mockups/d1/device-diagnostics.html` holds the four frames: at a glance, battery tapped, network tapped, and a device in trouble. Match the built view to it, and update it if the build finds the layout wrong.

## Open

- [ ] Confirm the fuel gauge part number on the test device before relying on any register beyond voltage and state of charge. The `'48`/`'49` variants expose a charge-rate register the `'40`/`'43` do not, and it would give direction without deriving it.
