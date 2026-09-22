# M1 — Report the battery from the OS where there is no I2C fuel gauge

Add an OS-battery fallback so a machine without the X120x Maxim gauge (a laptop, a non-Pi SBC, any device with an OS-reported battery) still reports `battery-charge`, `battery-voltage` and `battery-direction`. Spec changes landed in NFO (`device-info.md`) and VIEW (`device-view.md`); this is the build.

## Decisions (the "how" kept out of the spec)

- **Source: sysfs `/sys/class/power_supply`, read directly.** Not upower. sysfs already exposes `manufacturer` / `model_name` / `serial_number` (the metadata upower was wanted for), needs no daemon, and adds no dependency — matching the facts.rs house pattern of reading `/proc` and `/sys` directly. upower would drag in a D-Bus client (`zbus` async runtime, or `dbus`/libdbus which the cross-build avoids) and a running `upowerd` that field SBCs often lack.
- **Which entries count as a battery powering the device.** Keep `type` = `Battery` (the machine's own cell) and `type` = `UPS` (an external supply carrying it); exclude everything else (`Mains`, `USB`). Then exclude `scope` = `Device`, which is what marks a peripheral's cell (wireless mice, keyboards, controllers). Everything left — `scope` = `System` or no `scope` file at all — is reported.
- **No `scope` tier fallback.** Preferring `System` and falling back to no-`scope` only makes sense when picking one battery; reporting every battery, it would drop a no-`scope` UPS whenever some other entry declared `System`. Plain exclusion of `Device` is the rule. On the dev laptop this is visible: `BAT0` has no `scope` file at all, while `hidpp_battery_0` (the mouse) is `scope=Device`.
- **UPS attributes are patchier than a laptop's.** A UPS entry often carries `capacity` and `status` but no `voltage_now`, which is exactly the `skipped` voltage case. Kernel-visible UPSes only; a UPS reachable solely through a userspace daemon is not in `/sys/class/power_supply` and is out of scope here.
- **I2C stays primary.** The OS path is reached only when the I2C gauge is absent (`i2c::Error::NoDevice`). Where the gauge answers, nothing changes.

## Source-to-wire mapping (sysfs → reading)

- `battery-charge` ← `capacity` (0–100) as a fraction. Where `capacity` is absent, derive from `energy_now`/`energy_full` or `charge_now`/`charge_full`; where none give a number, `broken`.
- `battery-voltage` ← `voltage_now` (µV → volts, rounded to 3 places). Absent, zero or unreadable → `skipped` with a reason that no voltage is available.
- `battery-direction` ← `status`: `Charging` → `charging`, `Discharging` → `discharging`, `Full`/`Not charging` → `idle`, `Unknown`/absent → `skipped`.
- `battery` trait ← `{ name: <dir name, e.g. BAT0>, serial: serial_number, model: model_name, vendor: manufacturer }`, carrying only the members present. `name` distinguishes; the rest describe.
- No `power-source` on this path (no backup-supply GPIO signal), matching NFO's omit-where-no-signal rule.

## Code shape

- New `crates/bliti/src/facts/power/os.rs`: enumerate `/sys/class/power_supply`, apply the filter above, and build one `(battery-charge, battery-voltage, battery-direction)` triple per battery, each carrying the `battery` trait. Stateless — direction comes from the OS, so no voltage `Watch` history is needed here.
- Restructure `power.rs::Watch::readings`: on `i2c::Error::NoDevice`, clear the voltage window and return `os::readings(at)` instead of an empty vec. All I2C-path readings (including the existing single battery) gain the `battery` trait, supplied by us rather than read: `name` is `built-in` (the cell sits inside the case, so naming it for that leaves `external`-style names free for a UPS someone might attach later) and `vendor` is `SupTronics`, the X120x's maker. The cell itself is from an unknowable third party, so no `serial` or `model` is carried.
- Keep `power.rs` under 1000 lines; the sysfs reading is its own module.

## Checklist

- [ ] `os.rs`: enumerate + filter power supplies (type Battery or UPS, exclude scope=Device)
- [ ] `os.rs`: build the three battery readings with the `battery` trait, per battery
- [ ] `os.rs`: charge / voltage / direction mapping incl. skipped/broken cases
- [ ] `power.rs`: route to `os::readings` on `NoDevice`; add `battery` trait (`name` `built-in`, `vendor` `SupTronics`) to the I2C-path battery readings
- [ ] VIEW client: headline `battery-charge` with `built-in` where present else the first by name; pair voltage/direction by `battery` trait in the reveal
- [ ] Tests: filter (mouse excluded, BAT0 and UPS kept, Mains/USB excluded), each status mapping, no-voltage skip, multiple batteries distinguished, no `power-source` on the OS path
- [ ] `cargo fmt`, `cargo test`, run on this laptop and eyeball the emitted readings
