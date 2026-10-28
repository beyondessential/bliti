# Report the HAT's air and light readings

## Technical notes

- bliti reads both chips over `/dev/i2c-3`, the way `facts/power/i2c.rs` reads the v3 gauge. The overlay stays driver-free, so provisioning's hardware check reaches the chips unchanged.
- Provisioning's check and the sampler can hit the same bus. Do each register select and read as one combined transfer (`I2C_RDWR`), not a write and then a read, so a second reader can't interleave between them. Within the process, share one handle per bus.
- BME680: read the calibration block once, then take one forced-mode measurement per slow tick with the gas sensor disabled (`run_gas` off, no heater profile), and compensate temperature, humidity and pressure with the datasheet's formulas. Test the compensation against the datasheet's worked values.
- LTR-308: put it in active mode at a fixed gain and resolution, and turn the count into lux with the datasheet's formula, with no window factor: the sensor sits behind the case's vents, not a window.
- Which sensors are fitted comes from the version `facts::product` already reads from the HAT EEPROM. Reuse that read, don't parse it twice.
- Web: wording for `humidity`, `air-pressure`, `light` and the `case` sensor, and tile placement after `fan-speed`.
