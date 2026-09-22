# M1 — Report the battery from the OS where there is no I2C fuel gauge

Add an OS-battery fallback so a machine without the X120x Maxim gauge (a laptop, a non-Pi SBC, any device with an OS-reported battery) still reports `battery-charge`, `battery-voltage` and `battery-direction`. Spec changes landed in NFO (`device-info.md`) and VIEW (`device-view.md`); this is the build.

## Decisions (the "how" kept out of the spec)

- **Source: sysfs `/sys/class/power_supply`, read directly.** Not upower. sysfs already exposes `manufacturer` / `model_name` / `serial_number` (the metadata upower was wanted for), needs no daemon, and adds no dependency — matching the facts.rs house pattern of reading `/proc` and `/sys` directly. upower would drag in a D-Bus client (`zbus` async runtime, or `dbus`/libdbus which the cross-build avoids) and a running `upowerd` that field SBCs often lack.
- **Which entries count as the device's battery.** Filter to `type` = `Battery`, then exclude `scope` = `Device` (peripherals: wireless mice, keyboards, controllers). Of what remains: prefer entries declaring `scope` = `System`; if none do, take those with no `scope` file at all. On the dev laptop this matters — `BAT0` has no `scope` file while `hidpp_battery_0` (the mouse) is `scope=Device`.
- **I2C stays primary.** The OS path is reached only when the I2C gauge is absent (`i2c::Error::NoDevice`). Where the gauge answers, nothing changes.

## Source-to-wire mapping (sysfs → reading)

- `battery-charge` ← `capacity` (0–100) as a fraction. Where `capacity` is absent, derive from `energy_now`/`energy_full` or `charge_now`/`charge_full`; where none give a number, `broken`.
- `battery-voltage` ← `voltage_now` (µV → volts, rounded to 3 places). Absent, zero or unreadable → `skipped` with a reason that no voltage is available.
- `battery-direction` ← `status`: `Charging` → `charging`, `Discharging` → `discharging`, `Full`/`Not charging` → `idle`, `Unknown`/absent → `skipped`.
- `battery` trait ← `{ name: <dir name, e.g. BAT0>, serial: serial_number, model: model_name, vendor: manufacturer }`, carrying only the members present. `name` distinguishes; the rest describe.
- No `power-source` on this path (no backup-supply GPIO signal), matching NFO's omit-where-no-signal rule.

## Code shape

- New `crates/bliti/src/facts/power/os.rs`: enumerate `/sys/class/power_supply`, apply the filter above, and build one `(battery-charge, battery-voltage, battery-direction)` triple per battery, each carrying the `battery` trait. Stateless — direction comes from the OS, so no voltage `Watch` history is needed here.
- Restructure `power.rs::Watch::readings`: on `i2c::Error::NoDevice`, clear the voltage window and return `os::readings(at)` instead of an empty vec. All I2C-path readings (including the existing single battery) gain the `battery` trait — the backup battery's name is device-supplied (e.g. `backup`).
- Keep `power.rs` under 1000 lines; the sysfs reading is its own module.

## Checklist

- [ ] `os.rs`: enumerate + filter power supplies (type=Battery, scope System/absent, exclude Device)
- [ ] `os.rs`: build the three battery readings with the `battery` trait, per battery
- [ ] `os.rs`: charge / voltage / direction mapping incl. skipped/broken cases
- [ ] `power.rs`: route to `os::readings` on `NoDevice`; add `battery` trait to the I2C-path battery readings with a device-supplied name
- [ ] VIEW client: headline `battery-charge` with the first battery; pair voltage/direction by `battery` trait in the reveal
- [ ] Tests: filter (mouse excluded, BAT0 kept), each status mapping, no-voltage skip, multiple batteries distinguished, no `power-source` on the OS path
- [ ] `cargo fmt`, `cargo test`, run on this laptop and eyeball the emitted readings
