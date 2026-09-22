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

## What the undervoltage signals actually do

Three signals were watched through a sustained marginal supply: the board bypassed, loaded to about 5.6 A, with `EXT5V_V` between 4.764 V and 4.824 V across 173 samples.

The throttle word read `0x50000` for the entire session, from boot to the end of the load, with bit 16 and bit 18 set and never changing.
Those positions are now observed on hardware rather than taken from documentation, though their meaning is only consistent with the documented layout and the circumstances rather than independently proven.
They are a latch: already set by the time anything could look, because the device browned out during boot, and they never cleared.
They say a brownout happened at some unknown point, and cannot distinguish one three weeks ago from one happening now.

The throttle word's current-condition bits never asserted at all, through the whole of a sustained undervoltage.

`in0_lcrit_alarm` asserted in 16 of the 173 samples, flipping six times.
The supply voltage while it was asserted ranged 4.7691 V to 4.8240 V, and while it was clear, 4.7637 V to 4.8240 V.
The two ranges overlap completely, so the bit did not separate any condition the voltage could distinguish.

The voltage held a mean of 4.7933 V across the same window, in a range of 60 mV.

This is the card's argument, observed.
The condition was unchanging for ninety seconds, the number said so steadily, and the bit reported it sixteen times with six transitions.
A reading driven off the instantaneous bit would move between `warning` and `passed` six times while nothing about the supply changed.

## Throttling detection is dead on this hardware

`crates/bliti/src/facts/compute.rs` reads the throttle word from `/sys/devices/platform/soc/soc:firmware/get_throttled`.
That path does not exist on a Pi 5: the firmware node is `soc@107c000000:firmware`, and no `get_throttled` attribute exists anywhere under `/sys` on this kernel.
`throttled_reason` therefore returns nothing unconditionally, and `cpu-frequency` can never report throttling, which NFO requires it to.
The mailbox is the only route to that word on this board, so fixing this falls out of the same work.

## Decisions

`supply-voltage` is reported from `EXT5V_V` as a quantity in volts.
It carries no notion of being in difficulty, so it is always `passed`, which NFO permits: a device reports `warning` or `failed` only where the measurement has such a notion.

`cpu-power` is reported as a quantity in watts, the product of the core rail's voltage and current.
Measured across both sessions it runs 0.45 W idle to 5.03 W loaded, a range of about eleven to one, which is far more legible than the 40 mV the supply voltage moves over the same swing.

Each is its own catalogue entry rather than an instance of a rail dimension.
NFO's trait rule holds that measurements which merely resemble each other are separate entries, and the 5 V input and the SoC core rail are not instances of one measurement.

No brownout is reported.
Neither undervoltage bit can drive a live state: the throttle word's current-condition bits do not assert, and the alarm bit chatters against an unchanging supply, while the word's sticky history is a latch that never clears and cannot say when.

The throttle word is still read, because it is what `cpu-frequency` needs and the sysfs path it currently reads does not exist on this board.

The `rp1_adc` hwmon is not used, despite costing a plain file read.
Its second channel tracks the 5 V supply at a ratio near two, but the ratio shifts by 1.28% between power states, so treating it as a voltage means inventing a calibration constant.

## Detecting a bypassed backup

A bypass is already reported: `power-source` carries `bypassing-backup`, and the raw signals it keys on were seen behaving correctly through this session, the power line low and the cell static to 1.3 mV.
It is established by the hardware power line together with the cell's stillness, and it will not assert until the cell has been watched for twenty-five seconds.

The two readings above measure the same thing directly rather than inferring it.
The backup board regulates its output, sagging 9.6 mV per watt drawn, where the same load on a supply feeding the Pi directly sags 69.0 mV per watt.
The idle levels differ too, 5.2350 V against 5.1085 V, but that gap is a property of the particular supply rather than of being bypassed, where the sevenfold difference in stiffness is a property of whether a regulator sits in the path.

Reading stiffness needs the load to vary and so is derived across samples, as the cell watch already is.
Whether to fold it into how `power-source` is established is not settled, and is a change to that reading rather than to these.

## What this settles for P1

The catalogue gains no `boolean` kind.
Undervoltage becomes neither a reading of its own with no value nor a state on another, because no bit on this board can carry one.

## Build

- [ ] A mailbox property client: open `/dev/vcio`, one ioctl, the gencmd tag, and a parse of the single-line rail response
- [ ] `supply-voltage` from `EXT5V_V`, in volts, carrying no status of its own
- [ ] `cpu-power` in watts, from the core rail's voltage and current
- [ ] Read the throttle word over the mailbox in `compute.rs`, replacing the sysfs path that does not exist on this board
- [ ] NFO gains the two catalogue entries
- [ ] Tests

## Open

Whether the throttle word's current-condition bits assert under thermal throttling.
They were never seen set, but the board was never made hot, so only the undervoltage path has been ruled out.

Whether the daemon's own `power-source` output was correct while bypassed.
The signals it derives that from were watched and behaved as expected, but what the daemon reported was not read back.
