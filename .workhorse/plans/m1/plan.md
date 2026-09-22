# M1 — Report the battery from the OS where there is no I2C fuel gauge

Add an OS-battery fallback so a machine without the X120x Maxim gauge (a laptop, a non-Pi SBC, any device with an OS-reported battery) still reports `battery-charge`, `battery-voltage` and `battery-direction`. Spec changes landed in NFO (`device-info.md`) and VIEW (`device-view.md`); this is the build.

## Decisions (the "how" kept out of the spec)

- **Source: upower over D-Bus.** Not sysfs. An external UPS is invisible to sysfs: the Eaton 3S on the dev laptop creates no `/sys/class/power_supply` entry at all, because the kernel's usbhid driver does not expose HID Power Device class hardware as a power supply. upower does see it (`ups_hiddev5`), reading `/dev/usb/hiddev5` directly — a root-only node, which upowerd already has the privilege for and we would otherwise have to acquire.
- **The D-Bus cost is already paid.** `dbus` 0.9.12 is already a direct Linux dependency of this crate, carried so the `vendored-dbus` feature can reach bluer's copy, and it is not yet used by any of our own code. Talking to upower adds no new dependency and no new cross-build burden. The crate likewise already requires a system daemon, since bluer is built with `bluetoothd`.
- **Which entries count as a battery powering the device.** Take upower devices whose `Type` is battery or UPS *and* whose `PowerSupply` is true. `PowerSupply` is upower's own answer to "does this power the machine", and it is exactly the peripheral distinction: the laptop cell and the UPS are both true, the wireless mouse is false. upower goes as far as annotating the mouse's own percentage "should be ignored".
- **Battery naming.** `name` is upower's `Model` where it reports one, giving `DELL T453X` and `Eaton 3S` on this laptop. A model is stable across a replug, which the object path is not. Where a device reports no model, or where two batteries would take the same model, fall back to the object-path basename (`BAT0`, `hiddev5`), which is unique within a running system. `built-in` is the I2C path's name only and is never used here.
- **sysfs is the fallback, not the source.** Where upower cannot be reached at all, fall back to `/sys/class/power_supply`, so a headless machine with an internal cell and no upowerd still reports it. The fallback covers `type` = `Battery` only: a HID UPS is not in sysfs under any filter, so there is nothing there to find.
- **The fallback engages on unreachable, never on empty.** upower answering with no batteries is an authoritative answer and is reported as no batteries. Only a failure to reach upower at all falls through. Rescanning after a valid empty answer would report a peripheral cell on a machine upower had correctly said has no battery.
- **The fallback needs its own peripheral filter.** `PowerSupply` is an upower property with no sysfs equivalent, so the sysfs path excludes `scope` = `Device` instead, and keeps `type` = `Battery` with `scope` = `System` or no `scope` at all. On this laptop that keeps `BAT0` and drops `hidpp_battery_0`.
- **I2C stays primary.** Either OS path is reached only when the I2C gauge is absent (`i2c::Error::NoDevice`). Where the gauge answers, nothing changes.
- **Reaching the gauge had to be widened first (not planned, found by running it).** Only a missing bus node counted as "no gauge", so an ordinary laptop — which carries a dozen root-only I2C buses for its display connectors — failed to open `/dev/i2c-1` with a permission error, reported `battery-charge` as `broken`, and never reached the OS path at all. The whole card was dead on its target machine. A bus that will not open (missing or forbidden) and an address nothing acknowledges (`ENXIO`, `EREMOTEIO`) now both mean no gauge is reachable. A gauge that answered and then failed is still `broken`, so a dead gauge on a Pi is not hidden.

## Source-to-wire mapping (upower property → reading)

- `battery-charge` ← `Percentage` (0–100) as a fraction.
- `battery-voltage` ← `Voltage`, where the device reports one. The Eaton reports none, which is the specced `skipped` case; the laptop reports 12.889 V.
- `battery-direction` ← `State`: charging → `charging`, discharging → `discharging`, fully-charged / pending-charge / pending-discharge / empty → `idle`, unknown → `skipped`.
- `battery` trait ← `{ name: Model or object-path basename, serial: Serial, model: Model, vendor: Vendor }`, carrying only the members that hold a value. `name` and `model` therefore usually coincide, which is fine: one distinguishes and the other describes. Drop placeholder serials: the Eaton reports the literal string `Blank`.
- No `power-source` on either OS path, per NFO.

On the sysfs fallback the same three readings come from `capacity` (or `energy_now`/`energy_full`), `voltage_now` (µV → volts) and `status` (`Charging`/`Discharging`/`Full`/`Not charging`/`Unknown`), with the `battery` trait named by `model_name` where present and the directory name otherwise, plus `serial_number`, `model_name` and `manufacturer`.

## Code shape

- New `crates/bliti/src/facts/power/upower.rs`: one system-bus connection, `EnumerateDevices` on `org.freedesktop.UPower`, then `GetAll` on `org.freedesktop.UPower.Device` per device; filter as above and build one `(battery-charge, battery-voltage, battery-direction)` triple per battery, each carrying the `battery` trait. Stateless — direction comes from upower, so no voltage `Watch` history is needed here.
- Use the `dbus` crate's blocking API with a short timeout. `Facts::sample` is synchronous and currently does only fast file reads; a D-Bus round trip that hung on a wedged upowerd would stall the sampler, so the call carries a timeout and a timeout is reported as `broken` rather than waited on.
- New `crates/bliti/src/facts/power/sysfs.rs`: the fallback, reading `/sys/class/power_supply` with the filter above. Reached only when the upower call fails to connect or times out, never when it succeeds.
- Where neither source answers, report nothing rather than erroring: a machine with no battery and no upower is not a machine with a broken battery.
- Restructure `power.rs::Watch::readings`: on `i2c::Error::NoDevice`, clear the voltage window and return `upower::readings(at)` instead of an empty vec. All I2C-path readings (including the existing single battery) gain the `battery` trait, supplied by us rather than read: `name` is `built-in` (the cell sits inside the case, so naming it for that leaves external names free for a UPS someone might attach later) and `vendor` is `SupTronics`, the X120x's maker. The cell itself is from an unknowable third party, so no `serial` or `model` is carried.
- Keep `power.rs` under 1000 lines; each OS source is its own module, with `power.rs` holding only the dispatch between gauge, upower and sysfs.

## Checklist

- [x] `i2c.rs`: a bus that will not open, and an address nothing acknowledges, mean no gauge rather than a fault
- [x] `upower.rs`: enumerate devices; filter to Type battery/UPS with PowerSupply true
- [x] `upower.rs`: build the three battery readings with the `battery` trait, per battery
- [x] `upower.rs`: name by model, falling back to the object-path basename on no model or a name collision
- [x] `upower.rs`: charge / voltage / direction mapping incl. skipped/broken cases and timeout handling
- [x] `sysfs.rs`: fallback reader (type=Battery, exclude scope=Device) with the same three readings
- [x] `power.rs`: dispatch gauge → upower → sysfs, falling through only when upower is unreachable
- [x] `power.rs`: add `battery` trait (`name` `built-in`, `vendor` `SupTronics`) to the I2C-path battery readings
- [x] VIEW client: headline `battery-charge` with `built-in` where present else the first by name; pair voltage/direction by `battery` trait in the reveal
- [x] Tests: upower filter (mouse excluded via PowerSupply, BAT0 and UPS kept, line-power excluded), sysfs filter (mouse excluded via scope), each state mapping, no-voltage skip, multiple batteries distinguished, no `power-source` on either OS path, and no fall-through on a valid empty answer
- [x] `cargo fmt`, `cargo test`, run on this laptop and check both BAT0 and the Eaton 3S are reported

## Verified on the dev laptop

Both paths were run against real hardware, with the Eaton 3S attached.

- upower path: `DELL T453X` at 100% / 12.887 V / idle, and `Eaton 3S` at 100% / voltage skipped / idle. The wireless mouse is absent, the placeholder serial is dropped, and no `power-source` is reported.
- sysfs fallback, forced by pointing `DBUS_SYSTEM_BUS_ADDRESS` at a bogus socket: `DELL T453X` reported identically, and the Eaton correctly absent, since sysfs cannot see it.
