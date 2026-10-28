# The Tamanu Iti HAT

The Tamanu Iti v4 has a small EEPROM chip on its HAT that tells the Pi which board it is fitted to, and names a device tree overlay for the Pi to load.
The files here are that overlay and the contents of that chip, as described in the [HAT spec](../.workhorse/specs/hat.md).

| file | is |
| --- | --- |
| `tamanu-iti-v4.dts` | the overlay `tamanu-iti-v4`: SPI0 with the TPM on CE1, and I²C3 on GPIO22/23 |
| `tamanu-iti-v4.0.txt` | the EEPROM image of revision B, in the input format of `eepmake` |

`just hat-overlay` and `just hat-eeprom` build them into `target/hat/`.
The overlay builds with `dtc`, and the image with `eepmake` from the `raspi-utils-eeprom` package.

The compiled overlay goes in the overlays directory of the boot partition, where the firmware loads it from.
The image is written to the EEPROM at address `0x50` of the ID bus (GPIO0 and GPIO1), which is write-protected unless the ID WP pads are bridged.
