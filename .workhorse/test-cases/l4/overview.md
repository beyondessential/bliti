# Report the HAT's air and light readings

## Which sensors are fitted

- [x] A v4.0 reports `temperature` on the `case` sensor, `humidity`, `air-pressure` and `light` (verifies spec: NFO)
- [x] No HAT, another HAT, an unreadable version, v4.1 and v5.0 report none of them (verifies spec: NFO)

## When a sensor doesn't answer

- [x] A v4.0 whose bus is missing reports all four as `broken`, each reason naming its chip, on every tick (verifies spec: NFO)
- [x] A bus denied for permission reports the readings as `skipped` (verifies spec: NFO)
- [x] Another part answering at the LTR-308ALS's address is `broken`, and the chip is left untouched (verifies spec: NFO)
- [x] An unexpected BME680 chip ID is named in the reason (verifies spec: NFO)
- [x] An LTR-308ALS that never finishes a measurement is reported
- [x] An LTR-308ALS reset since it was set up is set up again before it's read

## Readings

- [x] The BME680's measurement is reported in celsius, a humidity fraction and pascals (verifies spec: NFO)
- [x] Humidity a touch outside 0–100 % is held to a fraction
- [x] Light is the datasheet's lux at gain 3 and 100 ms, with no window factor (verifies spec: NFO)
- [x] The BME680's gas heater never runs (verifies spec: NFO)

## I²C

- [x] A register read goes out as one transfer, a write then a read
- [x] Adjacent operations of one kind go out as one message, and reads come back to their own buffers
- [x] A denied bus is told from a missing one, and an unacknowledged address from a failed exchange
- [ ] The four readings keep coming, with no `broken` blips, while something else reads the same bus

## Device view

- [x] Humidity, air pressure and light sit after the fan and before power, in %, hPa and lx (verifies spec: VIEW)
- [x] The case temperature is in the temperature reveal as "Inside case", and the processor as "Processor" (verifies spec: VIEW)
- [x] A broken humidity shows unavailable, with its reason (verifies spec: VIEW)

## On the prototype

- [x] Pressure is near local sea-level pressure adjusted for altitude (agreed with an Aranet on v4)
- [ ] Humidity and case temperature are plausible for the room, the case a few degrees warmer
- [x] Light rises under a torch
