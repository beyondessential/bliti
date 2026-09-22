# Supply voltage from the board's power management

Working notes for W1.
Everything under "What the hardware gives" was measured on `tamanu-iti-v4-prototype` (Raspberry Pi 5 Model B Rev 1.1, Ubuntu raspi kernel 7.0.0-1017), not taken from documentation.

## The route to the PMIC

`vcgencmd` is not a necessary dependency, and sampling does not mean executing a binary.
The whole of `vcgencmd pmic_read_adc` is one `open` of `/dev/vcio` and one ioctl, `_IOWR(0x64, 0, 8)` = `0xC0086400`, carrying the firmware's gencmd property tag `0x00030080`.
The tag's value buffer holds a `u32` return code followed by the NUL-terminated command at offset 4, and the response comes back in the same shape.
This was confirmed by driving the mailbox directly with no `vcgencmd` involved.

The daemon already runs as root, for the board-ID reasons its service file documents, so `/dev/vcio` at mode 0600 root:root needs no udev rule and no group membership.

Measured cost per call:

| route | cost |
| --- | --- |
| plain sysfs read | 0.038 ms |
| mailbox ioctl, one named rail | 2.65 ms |
| mailbox ioctl, all 26 rails | 23.2 ms |
| `vcgencmd` subprocess | 13.8 ms |

The slow tier samples every five seconds, so a single narrow read is about 0.05% duty, and it is the same order as the I2C gauge read already taken on that tick.
It introduces no new class of blocking, so it does not need to wait for G1.

## What the PMIC exposes

Twenty-six named rails: twelve currents and fourteen voltages.

`EXT5V_V` is what reaches the Pi.
`VDD_CORE_A` is the legible load signal, moving from 0.82 A idle to 5.50 A under four-core load.
`EXT5V_V` and `BATT_V` are voltage-only, so there is no input current to pair with the input voltage.

## Three distinct battery voltages

These are separate quantities and must not be conflated:

| quantity | typical | what it is |
| --- | --- | --- |
| X120x cell, I2C 0x36 | 3.87 V | the backup cell, already reported as `battery-voltage` |
| PMIC `BATT_V` | 2.46 V | the Pi's own RTC coin cell; `charging_voltage` is 0 |
| PMIC `EXT5V_V` | 5.23 V | what the backup board delivers into the Pi |

## EXT5V does not see a power cut

Across a mains pull, with the backup board carrying the device:

| state | EXT5V mean | sd | n |
| --- | --- | --- | --- |
| mains present | 5.2449 V | 0.0100 | 213 |
| on battery | 5.2359 V | 0.0029 | 64 |

The difference of means is 9 mV, smaller than the ordinary sample-to-sample noise while on mains.
Over the same window the cell fell 127 mV, so the cell voltage is the far stronger signal.

Under a 5.5–5.7 A core load on battery for ninety seconds, EXT5V held 5.187–5.197 V, and neither the throttle word nor the undervoltage alarm ever left zero.
Load on battery and load on mains both give about 5.19 V, so the 40 mV sag under load is a response to load and carries no information about where the power is coming from.

The backup board regulates its output, so `EXT5V_V` measures that output rather than the incoming supply.
The card's premise that the 5 V input says whether the supply into the board is sagging does not hold while a backup board sits in the path.

## EXT5V does see a bypass

Measured across all three power states:

| state | EXT5V mean | sd | n |
| --- | --- | --- | --- |
| via-backup | 5.2449 V | 0.0100 | 213 |
| on battery | 5.2270 V | 0.0173 | 945 |
| bypassing backup | 5.1077 V | 0.0119 | 84 |

The first two are 18 mV apart and not separable.
The bypassed state sits 137 mV below via-backup and 119 mV below battery, about seven standard deviations clear of either.

This is the state the reading is for.
While the backup board carries the device its regulated output is what `EXT5V_V` sees, which is why the first two states look alike; a device fed through its own socket is reading the incoming supply directly, and that is a different number.
The bypassed state is also the dangerous one, being the only state in which a power cut stops the device dead.

The cell was static across the whole bypass window, moving 1.3 mV, which is the signature the existing stillness heuristic looks for.
That heuristic needs twenty-five seconds before it will assert a bypass; the voltage says it in one sample.

The separation is not a fixed threshold, because 5.1077 V is this particular supply rather than a property of being bypassed.
A supply that happened to deliver what the backup board delivers would not be distinguishable this way.
The voltage corroborates a bypass and is worth reporting as a number; it does not replace the hardware signal and the cell's movement as the means of establishing one.

## The undervoltage bits assert

In the bypassed state the throttle word read `0x50000` steadily, with bit 16 and bit 18 set, while the current-condition bits stayed clear and `in0_lcrit_alarm` stayed at zero.

The bit positions are now observed on hardware rather than taken from documentation.
Their meaning is not independently proven: it is consistent with the documented layout and with the circumstances, a supply 137 mV low that browned out during boot.

This is the case the sticky history exists for.
The device had already passed through an undervoltage by the time anything could look, the current-condition bits said nothing was wrong, and no other signal the device reports would have shown it.

## Throttling detection is dead on this hardware

`crates/bliti/src/facts/compute.rs` reads the throttle word from `/sys/devices/platform/soc/soc:firmware/get_throttled`.
That path does not exist on a Pi 5: the firmware node is `soc@107c000000:firmware`, and no `get_throttled` attribute exists anywhere under `/sys` on this kernel.
`throttled_reason` therefore returns nothing unconditionally, and `cpu-frequency` can never report throttling, which NFO requires it to.
The mailbox is the only route to that word on this board, so fixing this falls out of the same work.

## Decisions

Undervoltage state comes from the throttle word over the mailbox rather than the `rpi_volt` `in0_lcrit_alarm` bit.
The word carries both the current condition and a sticky "has occurred" history, which is worth having on a device nobody was watching, and it is needed regardless to fix the defect above.

A core current reading is reported from `VDD_CORE_A`.

Where a supply voltage is reported it is its own catalogue entry rather than an instance of a rail dimension.
NFO's trait rule holds that measurements which merely resemble each other are separate entries, and the 5 V input and the SoC core rail are not instances of one measurement.

The `rp1_adc` hwmon is not used, despite costing a plain file read.
Its second channel tracks the 5 V supply at a ratio near two, but the ratio moves under load, so treating it as a voltage means inventing a calibration constant.
The mailbox is needed for the throttle word in any case.

## Open

Whether the current-condition undervoltage bits and `in0_lcrit_alarm` assert under load while bypassed, which would establish the remaining bit positions.
Only the sticky history bits have been seen set.
