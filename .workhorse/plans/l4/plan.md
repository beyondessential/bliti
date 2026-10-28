# Report the HAT's air and light readings

A v4.0 Iti reports the case temperature, humidity, air pressure and light from the HAT's BME680 and LTR-308ALS, which bliti reads directly over I²C3. The device view gives each of them a tile.

## Technical notes

- bliti reads both chips over `/dev/i2c-3`, the way `facts/power/i2c.rs` reads the v3 gauge. On the prototype, `i2c-3` is RP1's `i2c@7c000`. Bus numbers follow the Pi 5's fixed `i2cN` aliases, so the path is stable. The overlay stays driver-free, so provisioning's hardware check reaches the chips unchanged. `bliti.service` runs as root, so the bus opens without extra groups.
- Provisioning's check and the sampler can hit the same bus. Do each register select and read as one combined transfer (`I2C_RDWR`), not a write and then a read, so a second reader can't interleave between them. Each sensor holds its own handle on the bus: the combined transfer already keeps exchanges whole, and separate handles let each sensor reopen on its own after an error.
- `I2C_RDWR` sends adjacent operations of one kind as one message, so embedded-hal's no-restart rule between them holds.
- BME680: use `bosch-bme680` (MIT, depends only on `embedded-hal` 1.0, `bitfield` and `log`). Its `new` resets the chip and checks the chip ID. `gas_config(None)` leaves `run_gas` off, so the heater never runs. It does the datasheet compensation, and it returns humidity in percent and pressure in hectopascals. bliti implements `embedded_hal::i2c::I2c` on its own `Bus`, so `linux-embedded-hal` and its `i2cdev`/`nix` stack stay out. One forced measurement per slow tick.
- LTR-308: no crate exists, so hand-roll it, generic over `embedded_hal::i2c::I2c` so a stand-in chip can test it. Check the part ID, set gain 3 and 18-bit/100 ms at a 500 ms rate (the datasheet's default range, about 0.2 to 52,000 lux), then enable ALS. A set power-on flag in `MAIN_STATUS` means the chip was reset, so it's set up again before reading. Convert counts to lux with the datasheet's formula and no window factor: the sensor sits behind the case's vents, not a window.
- Which sensors are fitted comes from the version `facts::product` already reads from the HAT EEPROM. Reuse that read, don't parse it twice. The EEPROM doesn't change while bliti runs, so read it once per `Facts`.
- A fitted sensor that doesn't answer, answers as the wrong part, or fails an exchange reports all its entries as `broken`, with a reason naming the chip. A bus that won't open for permission is `skipped` (NFO's precondition case). The gauge reader currently treats a permission error as "no device", so the error type has to keep the two apart.

## Checklist

### I²C bus

- [x] `facts/power/i2c.rs`: add a combined write-then-read over `I2C_RDWR` (one ioctl, two messages) and a plain write. Rebuild `read_word` on the combined transfer
- [x] `facts/power/i2c.rs`: split a denied bus from an absent one in `Error`. Have the gauge (`power.rs`, `power/record.rs`) keep treating both as no gauge, and reword the "no I2C gauge" message to fit any device
- [x] Move `i2c.rs` up to `facts/i2c.rs`, since the gauge and the HAT sensors now share it
- [x] Implement `embedded_hal::i2c::I2c` for `Bus`, mapping a transaction's operations onto one `I2C_RDWR`. Add a `DelayNs` backed by `std::thread::sleep`
- [x] `cargo add embedded-hal bosch-bme680` in `crates/bliti`
- [x] Unit-test the message building for `I2C_RDWR` (addresses, read flag, lengths) as a pure function

### Fitted sensors

- [x] `facts/product.rs`: expose the HAT read as `pub(super) fn version() -> Option<Version>`, `None` unless the board is a recognised Tamanu Iti, and have `entry` use the same read
- [x] New `facts/environment.rs`: map a version to its fitted sensors (4.0 → BME680 at `0x77` and LTR-308ALS at `0x53` on `/dev/i2c-3`, everything else → none), with `// spec: NFO`
- [x] Hold an `environment::Sensors` on `Facts`, built from the version in `Facts::new`. Sample it on the slow tick in `Facts::sample`. Rebuild it in `reset` so the drivers are opened again

### BME680

- [x] `facts/environment/bme680.rs`: open the bus, build `Bme680` with oversampling on all three channels, a light IIR filter and `gas_config(None)`, and keep it across ticks
- [x] Each slow tick, measure and emit `temperature` (`sensor: "case"`, `celsius`), `humidity` (`fraction`, percent / 100 clamped to 0–1) and `air-pressure` (`pascals`, hectopascals × 100)
- [x] On any error, emit all three as `broken` with a reason naming the BME680 and the error (unexpected chip ID included), then drop the driver so the next tick reopens it. A denied bus emits all three as `skipped`
- [x] Tests: unit conversion and clamping, the three entries on error, and the `case` sensor trait

### LTR-308ALS

- [x] `facts/environment/ltr308.rs`: check the part ID (`0xb1`), enable ALS in `MAIN_CTRL`, set `ALS_MEAS_RATE` and `ALS_GAIN` once, then read the 20-bit `ALS_DATA` each slow tick, gated on `MAIN_STATUS` data-ready. Check the register addresses and the lux formula against the datasheet in `dantudose/LTR308`
- [x] Emit `light` (`quantity`, `lux`), and on error or a wrong part ID emit `light` as `broken` (or `skipped` for a denied bus) with a reason naming the LTR-308ALS. Re-initialise on the next tick after an error
- [x] Tests: lux from counts at the chosen gain and resolution against a datasheet example, the wrong-part path, and the not-ready path

### Fitted-sensor tests

- [x] `facts/environment.rs` tests: v3 (no HAT), another HAT, an unreadable version, and 4.1 report none of these entries, while 4.0 with neither chip answering reports all four as `broken`

### Web client

- [x] `bliti-web/app/src/readings.js`: add `humidity`, `air-pressure` and `light` to the tile order after `fan-speed`. Add labels "Humidity", "Air pressure" and "Light". In `formatQuantity`, show `pascals` as hPa and `lux` as lx
- [x] `bliti-web/app/src/Readings.jsx`: give the three their own tiles with history: humidity drawn against its fraction scale, pressure and light as plain quantities. Word the `sensor` values in `TemperatureTile` (`cpu` → "Processor", `case` → "Inside case") instead of showing them raw
- [x] `bliti-web/app/tests/readings.spec.js`: tile order and wording for the three, the `case` sensor in the temperature reveal under its wording, and a `broken` humidity showing its reason

### On the prototype

- [ ] Deploy with the `bliti-prototype` skill and check the readings on the device view: pressure near local sea-level pressure adjusted for altitude, humidity and case temperature plausible, and light rising under a torch
- [x] Check `battery-voltage` still matches the gauge read by hand (`i2cget -y 1 0x36 0x02 w`, bytes swapped, top twelve bits at 1.25 mV), since the gauge read now goes out as one combined transfer. Matched on v4: 4.019 V and 89.4 % from bliti against 4.039 V and 89.3 % by hand, 30 s apart on battery
- [ ] Check the four readings keep coming while something else reads the bus (`i2cget -y 3 0x77 0xd0` in a loop alongside bliti), with no `broken` blips
