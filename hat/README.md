# The Tamanu Iti HAT

What a Tamanu Iti v4's HAT EEPROM holds and the device tree overlay it names, as [HAT](../.workhorse/specs/hat.md) specifies.

| file | is |
| --- | --- |
| `tamanu-iti-v4.dts` | the overlay `tamanu-iti-v4`: SPI0 with the TPM on CE1, and I²C3 on GPIO22/23 |
| `tamanu-iti-v4.0.txt` | the EEPROM image of revision B, in the input format of `eepmake` |

`just hat-overlay` and `just hat-eeprom` build them into `target/hat/`.
The overlay builds with `dtc`, and the image with `eepmake` from the `raspi-utils-eeprom` package.

The compiled overlay goes in the overlays directory of the boot partition, where the firmware loads it from.
The image is written to the EEPROM at address `0x50` of the ID bus (GPIO0 and GPIO1), which is write-protected unless the ID WP pads are bridged.
