# Provisioning an Iti v4 on the prototype

On-device test pass over the whole provisioning flow, run on `tamanu-iti-v4-prototype` (rev B, Pi 5).
The sections are in the order they can be run. Several steps destroy what is on the board, so later sections depend on earlier ones having left the board as they describe.

## The bench

Read off the board on 2026-11-02, before any of this ran:

- The HAT EEPROM already holds the 4.0 image (programmed by hand): `/proc/device-tree/hat/` reads Tamanu Iti v4.0, `product_id` `0x0004`, `product_ver` `0x0000`, and the Tamanu Iti UUID.
- The sensor bus answers at `0x53` (LTR-308) and `0x77` (BME680). The hardware handoff's `0x76` for the BME680 is wrong, and the specs and code are right.
- The bootloader is dated 2025-12-08. Its configuration was PBL's, `BOOT_ORDER=0xf61`, which has no USB.
- The NVMe holds the hand-built dev system, laid out as the pi image with its partition labels.

The medium is a 64 GB USB3 stick, written from the PR preview's image. For debugging without the serial console, which needs the case taken apart, the stick's root gets the laptop's SSH key for `ubuntu`, so the medium is reachable over the LAN on `end0`. Nothing under test reads that key.

Raspberry Pi's bootloader of 2026-09-25, the default channel's and the one `bootloader` installs, ignores `usb_max_current_enable` in `config.txt` (rpi-eeprom issue 899, fixed on the latest channel from 2026-10-21). A board carrying it stops a USB boot on the backup supply until its power button is pressed, which the cycle 2 boot needed.

The board runs on its X1208 backup supply, so pulling the mains does not cut its power. A power cut, wherever a case below calls for one, is the UPS's three-press power cut. The serial console is on the Pi's UART through a Debug Probe.

There is one NVMe, so the pass runs in two cycles:

1. The bootloader configuration is set to the Pi's stock one with USB first (`BOOT_ORDER=0xf614`) on the same bootloader, so the stick boots over the dev system. `disk` takes that system for an earlier one. Once the run finishes, and before the board reboots, the laptop's key goes into the new root from the medium, unlocked by the TPM. That keeps a way back in once `bootloader` has taken USB out of the boot order.
2. From the provisioned system, the bootloader configuration is set to the Pi's stock one (`BOOT_ORDER=0xf461`, USB last) and the NVMe is given a foreign partition table, to stand in for a factory board. After the overwrite prompt is declined, the drive is blanked from the medium for a run on a blank disk.

## Medium

- [ ] The provisioning image builds and publishes from main, with `provision-image-test` passing (verifies spec: PMED)
- [x] `/downloads/tamanu-iti-provision.img.zst`, its `.sha256` and its `.txt` manifest download from the PR preview, and the digest matches (verifies spec: PMED)
- [ ] The same three download from iti.tamanu.app once main publishes (verifies spec: PMED)
- [x] The stick's payload verifies against the manifest's root hash (verifies spec: PMED)
- [x] The manifest has every entry in order, and `BLITI_COMMIT` is the commit under test (verifies spec: PMED)
- [x] The image written to the USB stick boots the prototype into provisioning mode (verifies spec: PMED)
- [x] On the backup supply, which reports less than 5 A, the stick boots without the power button being pressed (verifies spec: PMED)
- [ ] With a pi-image system on the NVMe, the stick boots its own root, and the NVMe's partition table and LUKS identifiers come through the boot unchanged (verifies spec: PMED)
- [x] The data partition reads on the laptop as FAT32 `ITIPROV`, with `manifest.txt` at its root (verifies spec: PMED)
- [x] The console command opens on the serial console with no login, and exiting it gives a root shell (verifies spec: PMED)
- [ ] The console command opens on HDMI with no login (verifies spec: PMED)
- [x] `/proc/device-tree/hat/` shows the EEPROM, `/dev/i2c-3` and the TPM are up, and only the HAT overlay declares the TPM (verifies spec: PMED, HAT)
- [x] Cycle 1: booted from the stick with USB already first, the device leaves the bootloader unwritten (verifies spec: PRV)
- [x] Cycle 2: booted from the stick with USB last, the device puts USB first before any step runs, keeping SD then NVMe after it (`0xf614`) (verifies spec: PRV)

## Going on the air

- [x] The status LED is steady amber from start on a board that is not provisioned (verifies spec: PLED)
- [ ] A press with no run under way shows red and green alternating until a client connects, or the window ends (verifies spec: PLED)
- [ ] A press partway through a run leaves the run's pattern showing (verifies spec: PLED)
- [x] The device does not advertise after boot until the USER button is pressed (verifies spec: PRV)
- [x] A press advertises `ITI_PROVISION_1_` and a suffix matching the last four characters of `bliti qr`'s payload on the same board (verifies spec: PRV, QR)
- [x] Advertising stops five minutes after the press with no client (verifies spec: PRV)
- [ ] A second press while advertising restarts the five minutes (verifies spec: PRV)
- [x] Advertising stops once a client connects, a press while connected does nothing, and the device stays silent after the client leaves until pressed again (verifies spec: PRV)
- [ ] A second client, such as `bliti connect` from the laptop, is told there is no slot free while the page is connected (verifies spec: PRV, PWEB)

## Provisioning client: finding and connecting

- [x] iti.tamanu.app/provision titles itself Provision an Iti, and shows the USER button picture while finding a device (verifies spec: PWEB)
- [x] The chooser lists only provisioning-mode Itis, and only one just pressed (verifies spec: PWEB)
- [x] Closing the chooser without a pick says only a just-pressed Iti is listed, and offers it again (verifies spec: PWEB)
- [ ] Cancel while connecting returns to finding a device (verifies spec: PWEB)
- [x] Connected, the page names the Iti by its suffix and offers to disconnect (verifies spec: PWEB)
- [ ] Connected before the board has found every step's done-state, the page says the Iti is checking the board and lists no step until it has (verifies spec: PWEB)
- [x] Every step is listed in order with its done-state reason, `done` steps unticked and the rest ticked, and start is unavailable with nothing ticked (verifies spec: PWEB, RUN)
- [ ] The fake device in dev mode connects as `FAKE`, passes every chosen step at once, and adds no label (verifies spec: PWEB)
- [ ] Pictures of the USER button, CASE switch and ID WP pads show with the phone offline after a first load (verifies spec: PWEB)

## Step `hardware`

- [x] The press that put the board on the air passes `user` with no prompt (verifies spec: PHW)
- [ ] From the console with no press yet, `user` prompts to press the USER button and passes on a press (verifies spec: PHW)
- [x] `case` prompts with the CASE picture, and passes only after the lever is pressed and released (verifies spec: PHW, PWEB)
- [ ] Leaving the `case` prompt for five minutes fails `case` and closes the prompt, and a retry asks again (verifies spec: PHW)
- [x] `air`, `light` and `tpm` pass, and the board is placed as 4.0 (verifies spec: PHW)
- [x] The result lists all five checks in order and the version (verifies spec: PHW)
- [ ] The LED blinks amber at 1 Hz while the `case` prompt waits, and green at 1 Hz while the sensor checks run (verifies spec: PLED)
- [ ] After a run of `hardware` alone, the LED goes back to steady amber (verifies spec: PLED)

## Step `eeprom`

To set the EEPROM up for each case below, bridge the ID WP pads and write it from the medium's root shell.

- [ ] With the 4.0 image already on it, the step reads `done` with what the EEPROM holds as its reason, and passes without writing (verifies spec: PEEP)
- [ ] Blank, the step reads `not-done` and writes without asking (verifies spec: PEEP)
- [ ] Holding an earlier 4.0 image whose bytes differ, the step writes without asking (verifies spec: PEEP)
- [ ] Holding another HAT's image, the step asks to overwrite, naming its vendor and product and the version it would write (verifies spec: PEEP)
- [ ] Declining the overwrite fails the step, saying the EEPROM was left as it was, and it reads back unchanged (verifies spec: PEEP)
- [ ] The `id-wp` prompt shows the ID WP picture and says to use bare metal and hold until the step passes (verifies spec: PEEP, PWEB)
- [ ] Unbridged, no write lands and the EEPROM reads back unchanged; bridged with scissors, the step passes within seconds (verifies spec: PEEP)
- [ ] Left unbridged for five minutes, the step fails telling the operator to check both pads, and a retry gives five more minutes (verifies spec: PEEP)
- [ ] After a reboot, `/proc/device-tree/hat/` reads Tamanu Iti v4.0 with `product_id` `0x0004` and `product_ver` `0x0000` (verifies spec: HAT)
- [ ] Booted from the medium with any EEPROM contents, blank included, the HAT overlay's devices are present (verifies spec: PMED)

## Step `disk`

- [ ] The step's reason describes the hand-built system before the run (verifies spec: PDSK)
- [x] Cycle 1: the dev system reads as an earlier system and is overwritten without asking (verifies spec: PDSK)
- [ ] Cycle 2: a foreign partition table reads as another system, and the step asks to overwrite; declining fails it with the disk left as it was (verifies spec: PDSK)
- [ ] Cycle 2: blanked, the step reads `not-done` as blank and writes without asking (verifies spec: PDSK)
- [ ] The write and read-back show as one progress bar (verifies spec: PDSK, PWEB)
- [x] Afterwards the firmware partition carries `iti-image.txt` with the manifest's version and digest, and the step reads `done` (verifies spec: PDSK)
- [ ] A power cut partway through the write boots the stick again, resumes the run with no client, and restarts `disk` from its beginning without asking (verifies spec: PRV, RUN, PDSK)

## Step `overlay`

- [x] `tamanu-iti-v4.dtbo` lands in the current boot slot's `overlays` and in `/etc/flash-kernel/dtbs/overlays/`, byte for byte the medium's (verifies spec: PDSK)
- [ ] Run again with both in place, it passes without writing (verifies spec: PDSK)

## Step `software`

- [ ] Each file under `/usr/share/bliti/provision/system/` on the medium lands at the same path in the disk's root filesystem, byte for byte and with its mode: the daemon at 0755, the rest at 0644 (verifies spec: PDSK, PMED)
- [ ] `bliti.service` is enabled: `/etc/systemd/system/multi-user.target.wants/bliti.service` links to `/etc/systemd/system/bliti.service` (verifies spec: PDSK)
- [ ] Run again with everything in place, it passes without writing (verifies spec: PDSK)
- [ ] On its first boot from the NVMe the Iti runs bliti, and the operator app finds it (verifies spec: PDSK)

## Step `encryption`

- [x] Afterwards the root volume has a new UUID and volume key, exactly two keyslots, and the empty passphrase no longer opens it (verifies spec: PENC)
- [x] The recovery passphrase is six lowercase words of five or more characters joined by hyphens, and it opens the volume (verifies spec: PENC)
- [x] `records.tsv` on the data partition gains one tab-separated line: suffix, 4.0, the volume's UUID, the passphrase, and a UTC time (verifies spec: PENC)
- [ ] The passphrase appears in no journal line, on the device or in the page's Activity (verifies spec: PENC, PWEB)
- [ ] A power cut during re-encryption resumes it on the next run, and the volume still opens (verifies spec: PENC)
- [ ] The step reads `done` afterwards, and re-running it passes without changes (verifies spec: PENC)

## Step `tailscale`

- [x] The `auth-key` and `name` prompts open once `eeprom` has passed, without holding up `disk` (verifies spec: PTS)
- [ ] `name` defaults to `tamanu-iti-` and the suffix in lower case, and a name with an upper case letter or a leading hyphen is refused beneath the prompt (verifies spec: PTS, PWEB)
- [ ] The auth key field hides what is typed, says the key must be reusable, and the key is not in Activity (verifies spec: PTS, PWEB)
- [ ] With `auth-key` unanswered when its turn comes, the step waits, and the LED turns to blinking amber (verifies spec: PTS, PLED)
- [x] A saved key from a previous board answers the prompt at once, says when it was saved, and can be forgotten (verifies spec: PWEB)
- [x] A typed key is answered once and kept, even where a state still showing the prompt open arrives before the device takes it (verifies spec: PWEB)
- [ ] A rejected key fails the step, drops the saved key, and the field shows again on retry (verifies spec: PTS, PWEB)
- [ ] A key needing approval passes, and the page says an admin has to approve the Iti (verifies spec: PTS, PWEB)
- [ ] After a power cut between answering and the step, both prompts open again (verifies spec: PTS, RUN)
- [ ] Afterwards no `tailscaled` is left running on the medium, and the medium holds no Tailscale state (verifies spec: PTS)

## Step `bootloader`

- [x] With the board's 2025-12-08 bootloader older than the medium's 2026-09-25, the step updates it, and leaves exactly PBL's configuration (verifies spec: PBL)
- [x] After the step, the board boots its NVMe with the stick still in (verifies spec: PBL)
- [ ] With a configuration differing by one setting, the step reads `not-done`, names what differs, and writes the configuration (verifies spec: PBL)
- [ ] Run again with both current, it passes without writing (verifies spec: PBL)
- [x] The step never asks the operator anything (verifies spec: PBL)

## Step `label` and the sheet

- [x] The step passes with the board's payload, version 4.0 and a time, and the page says the label was added rather than showing the payload (verifies spec: PLBL, PWEB)
- [ ] `label` run alone on a blank EEPROM still gives version 4.0 from the sensors (verifies spec: PLBL)
- [x] The sheet is an A4 PDF in a 3 by 9 grid of 50 by 30 mm labels, the QR code scans as the board's payload, and the suffix and "Tamanu Iti v4.0" read beside it (verifies spec: PSHT)
- [ ] A label printed at 100% on a laser printer scans with a phone camera (verifies spec: PSHT)
- [x] Downloading marks the label printed, and a printed label can be ticked to print again (verifies spec: PSHT)
- [ ] Checking back at the finished board adds no second label (verifies spec: PSHT)
- [ ] Running `label` again replaces the label as unprinted (verifies spec: PSHT)
- [x] The download action states how full the sheet is (verifies spec: PSHT)
- [ ] From pressing download until the sheet is ready, the page says it is making the sheet and offers no second download (verifies spec: PSHT)

## The run as a whole

- [ ] On a board whose disk holds an earlier system, `overlay`, `software`, `encryption` and `tailscale` are offered ticked while `disk` is not done, since writing the disk undoes them, and the page shows what the board holds apart from the steps to run (verifies spec: RUN, PWEB)
- [ ] The console's offer to acknowledge a finished run does not default to discarding it (verifies spec: PCON)
- [x] A full run from the stick with every step ticked finishes with the LED steady green (verifies spec: RUN, PLED)
- [ ] A provisioned board that boots the medium again shows steady green from start (verifies spec: PLED)
- [ ] Closing the page mid-run leaves the run going, and pressing USER then reconnecting shows where it stands (verifies spec: RUN, PWEB)
- [x] When the channel drops, the page keeps the last state marked stale, and offers only the chooser and a USER press (verifies spec: PWEB)
- [ ] Once the channel has dropped, the page reconnects on its own after a press (verifies spec: PWEB)
- [ ] The console and the page connected together show the same run, and an answer from either is taken (verifies spec: PCON, RUN)
- [ ] A second answer to the same prompt is refused (verifies spec: RUN)
- [ ] A failed run survives a reboot as `failed`, with the LED blinking red at 3 Hz, and needs a retry to go on (verifies spec: RUN, PLED)
- [x] Stop while `waiting` or `failed` returns to `idle` and closes every prompt (verifies spec: RUN)
- [x] The finished screen shows the recovery passphrase with a copy button, says it is also on the medium, and says to power off and remove the medium (verifies spec: PWEB)
- [ ] A finished board rebooted from the medium comes up `idle` (verifies spec: RUN)
- [ ] The console takes a `secret` answer without echoing it, and an empty answer as the default (verifies spec: PCON)
- [ ] The console drives a whole run with no USER press and no client (verifies spec: PCON)

## First boot of the provisioned board

- [x] The board boots its NVMe and unlocks the root volume from the TPM unattended (verifies spec: PENC)
- [x] It comes up on the tailnet under the name given (verifies spec: PTS)
- [x] `/proc/device-tree/hat/` names the board, and `/dev/i2c-3` and the TPM are up from the installed overlay (verifies spec: HAT, PDSK)
- [ ] With bliti's ordinary daemon installed, the `product` fact reports Tamanu Iti v4.0 (verifies spec: NFO, HAT)
- [ ] After a kernel update, the new boot slot still carries the HAT overlay (verifies spec: PDSK)

## Not coverable on this bench

- [ ] Booted from an SD card, the device leaves the boot order alone (verifies spec: PRV)
- [ ] One medium serving two boards keeps each board's run and record apart (verifies spec: RUN, PENC)
- [ ] A board whose TPM gives no board ID advertises the `FAULT` suffix, and `encryption` refuses it (verifies spec: PRV, PENC)
- [ ] A board with no NVMe fails `disk`, `overlay` and `software`, saying so (verifies spec: PDSK)
