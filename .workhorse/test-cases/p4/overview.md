# Build the provisioning image: test cases

## Image structure (automated, after the build)

- [ ] Partitions are the `pi` image's, then the payload, then a FAT32 data partition labelled `ITIPROV` (verifies spec: PMED)
- [ ] The payload's first `PI_IMAGE_SIZE` bytes hash to `PI_IMAGE_SHA256`, which matches the `linux-images` release's checksum (verifies spec: PMED)
- [ ] `veritysetup verify` passes on the payload with the manifest's root hash and offset (verifies spec: PMED)
- [ ] Flipping a byte in the payload makes a read through verity fail (verifies spec: PMED)
- [ ] The root holds `bliti`, the provisioning unit enabled, `/etc/bliti/provision.env` with the data mount and `tpm-endorsement-key`, and no `bliti.service` (verifies spec: PMED)
- [ ] `/usr/share/bliti/provision/` holds the bootloader, overlay, EEPROM image and manifest; the overlay and EEPROM image match `just hat-overlay` and `just hat-eeprom` from the same commit (verifies spec: PMED)
- [ ] `config.txt` sets `force_eeprom_read=0`, loads `tamanu-iti-v4`, does not load `tpm-slb9670`; the overlay is in `current/overlays/` (verifies spec: PMED)
- [ ] Nothing on the root upgrades packages unattended: `unattended-upgrades` and the `apt-daily` timers are masked (verifies spec: PMED)
- [ ] The manifest has every entry in order, and `manifest.txt` on the data partition matches it (verifies spec: PMED)
- [ ] A `pi` image whose checksum does not match fails the build (verifies spec: PMED)

## On hardware

- [ ] A blank v4 board boots the stick into provisioning mode: LED steady amber, TPM up, sensors answering, EEPROM readable (verifies spec: PMED, PLED)
- [ ] A programmed board (the prototype, from SD) boots the same way, with no SPI chip-select collision in the log (verifies spec: PMED)
- [ ] tty1 and the serial console open the console command with no login (verifies spec: PMED, PCON)
- [ ] Quitting the console command leaves a root shell on that console (verifies spec: PMED)
- [ ] A run's state lands on `ITIPROV` and survives a reboot (verifies spec: PMED, RUN)
- [ ] With a second provisioning stick inserted, the board uses the data partition of the stick it booted from (verifies spec: PMED)
- [ ] A stick whose data partition is missing or will not mount reports it and does not start provisioning mode (verifies spec: PMED)
- [ ] The first boot of a fresh stick completes, and later boots are not slowed by root growth
- [ ] The image written with Raspberry Pi Imager and with `dd` both boot

## Publishing

- [ ] A merge to `main` replaces the three files under `/downloads/` on iti.tamanu.app (verifies spec: PMED)
- [ ] The weekly run publishes with no bliti change (verifies spec: PMED)
- [ ] A PR with the box ticked gets the files on its preview origin; unticking removes them; a later preview deploy keeps them while ticked (verifies spec: PMED)
- [ ] The downloaded image matches its `.sha256`
