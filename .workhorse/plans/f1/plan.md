# F1: power-loss line on v3 hardware

Measured on `tamanu-iti-v3-test-device` (Pi 5 Model B Rev 1.1, Geekworm X1201) on 2026-09-25, alongside `tamanu-iti-v4-prototype` (X1208).

## Result

The X1201 follows the X120x convention: **GPIO6, high while external power reaches the board's input**.
This is what `POWER_LINE` and `GAUGE` in `crates/bliti/src/facts/power.rs` already assume for the whole family, so the power-source reading and the NFO spec need no change for v3.

- [x] Header chip resolved by line name: `GPIO6` is on the RP1 chip, `gpiochip0` on this kernel, same as v4.
- [x] Mains into the X1201 input: GPIO6 low to high. Mains pulled: high to low. No other free header line moved.
- [x] Level confirmed in both directions: low on cells alone, high with mains at the X1201 (before and after a reboot), low throughout a boot fed by the Pi's own USB-C.
- [x] Pulling mains from the X1201 input leaves the device running on its cells.
- [x] Bypass: booted from the Pi's USB-C with the X1201 input empty, GPIO6 stays low while the device runs, and pulling the USB-C halts it outright. The boot's journal ends mid-run with no shutdown sequence.
- [x] Gauge: Maxim MAX1704x at 0x36 on I2C bus 1, same as v4 (read 3.814 V).

GPIO6 reads low against the Pi's default pull-up whenever the board is fitted and unpowered, so the board drives the line both ways rather than leaving it floating.

## Seating

The X1201 reaches the Pi through pogo contacts pressed onto the underside of the 40-pin header.
On the first boot the gauge did not answer at all, and nothing on bus 1 did, while GPIO6 was already driven: the contacts under header pins 3 and 5 (SDA, SCL) were not touching and the one under pin 31 was.
A reseat fixed it.
Under the current code such a unit reports no battery and no power source, since the power line is only read once the gauge answers, which is the fallback NFO already describes for a device with no backup board.
Flagging it from GPIO6 held low while the gauge is silent was considered and left out: it only works off mains, and v3 is being replaced by v4, whose fit is reliable.

## Other differences from v4

Hardware and boot configuration match: same kernel (`7.0.0-1017-raspi`), Ubuntu 26.04, bootloader, `config.txt`, and NVMe.
Both images enable the `tpm-slb9670` overlay and neither has a TPM fitted.

The differences are in the v3 image's software, and matter when bliti is first deployed there:

- No bliti installed.
- Wireless is wpa_supplicant, enabled, with neither iwd nor hostapd installed; bliti starts iwd.
- The radio is `wld0` rather than `wlan0`. The code discovers interfaces, so no name is assumed outside tests.
- No `raspi-utils` (`pinctrl`, `vcgencmd`).

## Backup supply history in the log

Added on this card once the v4 run-down showed that nothing on a device recorded when mains went or when it died (DEV, "Reporting").

- [x] The daemon reports the backup supply at start, on each change of external power, and each 10 mV the cell falls while it is absent, with how long it has been gone, on its own thread so it runs whether or not sampling is (`facts/power/record.rs`).
- [x] `services/bliti-journald.conf` syncs the journal every 15 s, so those reports survive a hard cut. On v4's cut the journal lost about a second, against four minutes before.
- [x] Running kept on v3 and v4 as `b300300`.

Follow-on work is on other cards: T2 (orderly shutdown at 2.8 V, calibrated charge), V2 (battery control), X2 (a board that protects its own cells).
