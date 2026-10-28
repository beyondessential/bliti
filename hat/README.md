# The Tamanu Iti HAT

The Tamanu Iti v4 has a small EEPROM chip on its HAT that tells the Pi which board it is fitted to, and names a device tree overlay for the Pi to load.
The files here are that overlay and the contents of that chip, as described in the [HAT spec](../.workhorse/specs/hat.md).

| file | is |
| --- | --- |
| `tamanu-iti-v4.dts` | the overlay: turns on SPI0 for the TPM on CE1, and I²C3 on GPIO22/23 |
| `tamanu-iti-v4.0.txt` | what the EEPROM holds on revision B, in the format `eepmake` reads |

`just hat-overlay` and `just hat-eeprom` build them into `target/hat/`.
The overlay needs `dtc`, and the EEPROM image needs `eepmake` from the `raspi-utils-eeprom` package.

The compiled overlay goes in the `overlays` folder of the boot partition, where the Pi looks for it.
The EEPROM image is written to the chip at address `0x50` on the ID bus (GPIO0 and GPIO1).
The chip is write-protected unless the ID WP pads are bridged.
