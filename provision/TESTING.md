# Testing a provisioning medium on a board

A checklist for taking a provisioning image through the whole flow on a real Tamanu Iti v4, after changing provisioning or before relying on a new image. Each case cites the spec it checks. Copy the list somewhere you can tick it, such as the test cases of the card doing the work, rather than ticking it here.

The sections are in the order they can be run. Several steps destroy what is on the board, so later sections depend on earlier ones having left the board as they say.

## The bench

- **The image** comes from CI, never a laptop: main publishes it under `iti.tamanu.app/downloads/`, and a pull request with "Build the provisioning image" ticked publishes it on its preview. Every push to such a pull request replaces the preview's image, so make no change while one is downloading.
- **Debugging the medium** without opening the case: before the stick goes in, open its root volume (the empty passphrase unlocks it), mount `subvol=@`, and put a public key in `/home/ubuntu/.ssh/authorized_keys`. The medium then answers SSH on the board's wired address. A tailnet route covering the LAN can send that traffic into Tailscale instead, which `ip route get` shows.
- **The serial console** is on the Pi's UART (the 3-pin connector between the HDMI ports), at 115200 baud. The medium opens the console command there with no login, and quitting it leaves a root shell.
- **A power cut**, on a board running on its backup supply, means cutting the backup supply itself (on the X1208, its three-press power cut). Pulling the mains cuts nothing.
- **The power button.** Raspberry Pi's bootloader of 2026-09-25 ignores `usb_max_current_enable` in `config.txt` (rpi-eeprom issue 899, fixed from 2026-10-21). A board carrying it stops a USB boot on the backup supply until its power button is pressed. Check which bootloader a board has before reading a stalled boot as a fault.

## Board setups

A board is set up with the bootloader configuration and the disk each section needs. Both are written from a root shell on the board, on whichever system it is running, with `rpi-eeprom` and `gdisk` installed.

The bootloader configuration is changed on the board's own bootloader, so its date stays as it is:

```sh
sudo flashrom -p linux_spi:dev=/dev/spidev10.0,spispeed=16000 -r /tmp/flash.bin
printf '[all]\nBOOT_UART=1\nPOWER_OFF_ON_HALT=0\nBOOT_ORDER=0xf461\n' > /tmp/stock.conf
sudo rpi-eeprom-config --apply /tmp/stock.conf /tmp/flash.bin
```

- **A factory board:** the Pi's stock configuration above (`BOOT_ORDER=0xf461`, so USB is tried last) and an NVMe holding something other than an Iti's system. `sudo sgdisk -Z /dev/nvme0n1 && sudo sgdisk -n 1:131072:0 -c 1:data -t 1:8300 /dev/nvme0n1` gives it one partition that starts clear of where an Iti's firmware partition was, which `disk` reads as another system. `sudo wipefs -a /dev/nvme0n1` from the medium leaves it blank.
- **A board over an earlier system:** USB first (`BOOT_ORDER=0xf614`) and an NVMe holding a system laid out as the `pi` image, such as one provisioned before.

Once `bootloader` has run, USB is out of the boot order, so a provisioned board goes back to the medium only after one of these setups. A provisioned system lets `ubuntu` in on its serial console with the password `bes`, changed at the first login, and has passwordless sudo.

## Medium

- [ ] The provisioning image builds and publishes, with `provision-image-test` passing (verifies spec: PMED)
- [ ] `tamanu-iti-provision.img.zst`, its `.sha256` and its `.txt` manifest download, and the digest matches (verifies spec: PMED)
- [ ] The stick's payload verifies against the manifest's root hash (verifies spec: PMED)
- [ ] The manifest has every entry in order, and `BLITI_COMMIT` is the commit under test (verifies spec: PMED)
- [ ] The image written to a USB stick boots the board into provisioning mode (verifies spec: PMED)
- [ ] On the backup supply, the stick boots without the power button being pressed, on a bootloader that honours `usb_max_current_enable` (verifies spec: PMED)
- [ ] With a `pi`-image system on the NVMe, the stick boots its own root, and the NVMe's partition table and LUKS identifiers come through the boot unchanged (verifies spec: PMED)
- [ ] The data partition reads on a laptop as FAT32 `ITIPROV`, with `manifest.txt` at its root (verifies spec: PMED)
- [ ] The console command opens on the serial console with no login, and exiting it gives a root shell (verifies spec: PMED)
- [ ] The console command opens on HDMI with no login (verifies spec: PMED)
- [ ] `/proc/device-tree/hat/` shows the EEPROM, `/dev/i2c-3` and the TPM are up, and only the HAT overlay declares the TPM (verifies spec: PMED, HAT)
- [ ] Over an earlier system with USB already first, the device leaves the bootloader unwritten (verifies spec: PRV)
- [ ] On a factory board, the device puts USB first before any step runs, keeping SD then NVMe after it (`0xf614`) (verifies spec: PRV)
- [ ] The medium runs no `tailscaled` of its own (verifies spec: PMED)

## Going on the air

- [ ] The status LED is steady amber from start on a board that is not provisioned (verifies spec: PLED)
- [ ] A press with no run under way shows red and green alternating until a client connects, or the window ends (verifies spec: PLED)
- [ ] A press partway through a run leaves the run's pattern showing (verifies spec: PLED)
- [ ] The device does not advertise after boot until the USER button is pressed (verifies spec: PRV)
- [ ] A press advertises `ITI_PROVISION_1_` and a suffix matching the last four characters of `bliti qr`'s payload on the same board (verifies spec: PRV, QR)
- [ ] Advertising stops five minutes after the press with no client (verifies spec: PRV)
- [ ] A second press while advertising restarts the five minutes (verifies spec: PRV)
- [ ] Advertising stops once a client connects, a press while connected does nothing, and the device stays silent after the client leaves until pressed again (verifies spec: PRV)
- [ ] A second client is told there is no slot free while the page is connected (verifies spec: PRV, PWEB)

## Provisioning client: finding and connecting

- [ ] `/provision` titles itself Provision an Iti, and shows the USER button picture while finding a device (verifies spec: PWEB)
- [ ] The chooser lists only provisioning-mode Itis, and only one just pressed (verifies spec: PWEB)
- [ ] Closing the chooser without a pick says only a just-pressed Iti is listed, and offers it again (verifies spec: PWEB)
- [ ] Cancel while connecting returns to finding a device (verifies spec: PWEB)
- [ ] Connected, the page names the Iti by its suffix and offers to disconnect (verifies spec: PWEB)
- [ ] Connected before the board has found every step's done-state, the page says the Iti is checking the board and lists no step until it has (verifies spec: PWEB)
- [ ] Every step is listed in order with its done-state reason, `done` steps unticked and the rest ticked, and start is unavailable with nothing ticked (verifies spec: PWEB, RUN)
- [ ] Pictures of the USER button, CASE switch and ID WP pads show with the phone offline after a first load (verifies spec: PWEB)

## Step `hardware`

- [ ] The press that put the board on the air passes `user` with no prompt (verifies spec: PHW)
- [ ] From the console with no press yet, `user` prompts to press the USER button and passes on a press (verifies spec: PHW)
- [ ] `case` prompts with the CASE picture, and passes only after the lever is pressed and released (verifies spec: PHW, PWEB)
- [ ] Leaving the `case` prompt for five minutes fails `case` and closes the prompt, and a retry asks again (verifies spec: PHW)
- [ ] `air`, `light` and `tpm` pass, and the board is placed as its version (verifies spec: PHW)
- [ ] The result lists all five checks in order and the version (verifies spec: PHW)
- [ ] The LED blinks amber at 1 Hz while the `case` prompt waits, and green at 1 Hz while the sensor checks run (verifies spec: PLED)
- [ ] After a run of `hardware` alone, the LED goes back to steady amber (verifies spec: PLED)

## Step `eeprom`

To set the EEPROM up for each case below, bridge the ID WP pads and write it from the medium's root shell.

- [ ] Holding the image for the board's version, the step reads `done` with what the EEPROM holds as its reason, and passes without writing (verifies spec: PEEP)
- [ ] Blank, the step reads `not-done` and writes without asking (verifies spec: PEEP)
- [ ] Holding an earlier image of the board's version whose bytes differ, the step writes without asking (verifies spec: PEEP)
- [ ] Holding another HAT's image, the step asks to overwrite, naming its vendor and product and the version it would write (verifies spec: PEEP)
- [ ] Declining the overwrite fails the step, saying the EEPROM was left as it was, and it reads back unchanged (verifies spec: PEEP)
- [ ] The `id-wp` prompt shows the ID WP picture and says to use bare metal and hold until the step passes (verifies spec: PEEP, PWEB)
- [ ] Unbridged, no write lands and the EEPROM reads back unchanged; bridged with scissors, the step passes within seconds (verifies spec: PEEP)
- [ ] Left unbridged for five minutes, the step fails telling the operator to check both pads, and a retry gives five more minutes (verifies spec: PEEP)
- [ ] After a reboot, `/proc/device-tree/hat/` reads the board's product, `product_id` and `product_ver` (verifies spec: HAT)
- [ ] Booted from the medium with any EEPROM contents, blank included, the HAT overlay's devices are present (verifies spec: PMED)

## Step `disk`

- [ ] Over an earlier system, the step reads it as one and overwrites it without asking (verifies spec: PDSK)
- [ ] On a factory board, the step reads another system and asks to overwrite; declining fails it with the disk left as it was (verifies spec: PDSK)
- [ ] A retry after the overwrite was declined asks again, and confirming it carries on (verifies spec: RUN)
- [ ] Blank, the step reads `not-done` as blank and writes without asking (verifies spec: PDSK)
- [ ] While `disk` is not done, `overlay`, `software`, `encryption` and `tailscale` read not done, saying what the board holds and that writing the disk replaces it, or that writing the disk comes first (verifies spec: RUN)
- [ ] The write and read-back show as one progress bar (verifies spec: PDSK, PWEB)
- [ ] Afterwards the firmware partition carries `iti-image.txt` with the manifest's version and digest, and the step reads `done` (verifies spec: PDSK)
- [ ] The payload's verity mapping is gone once the step ends (verifies spec: PMED)
- [ ] A power cut partway through the write boots the stick again, resumes the run with no client, and restarts `disk` from its beginning without asking (verifies spec: PRV, RUN, PDSK)

## Step `overlay`

- [ ] `tamanu-iti-v4.dtbo` lands in the current boot slot's `overlays` and in `/etc/flash-kernel/dtbs/overlays/`, byte for byte the medium's (verifies spec: PDSK)
- [ ] Run again with both in place, it passes without writing (verifies spec: PDSK)

## Step `software`

- [ ] Each file under `/usr/share/bliti/provision/system/` on the medium lands at the same path in the disk's root filesystem, byte for byte and with its mode (verifies spec: PDSK, PMED)
- [ ] `bliti.service` is enabled through `multi-user.target.wants` (verifies spec: PDSK)
- [ ] With no network connected, `iwd` and `hostapd` are installed from the medium's `debs/`, and `dpkg -l iwd hostapd` in the disk's system shows both `ii` (verifies spec: PDSK, PMED)
- [ ] Afterwards `iwd.service` is disabled and not masked, `hostapd.service` masked, and `wpa_supplicant.service` disabled; the disk's system carries `/etc/systemd/system/iwd.service.d/10-bliti.conf` and `/etc/systemd/system/bliti-hostapd@.service` (verifies spec: PDSK)
- [ ] `/etc/bliti/daemon.env` on the disk passes `--network-backend stack --network /var/lib/bliti/network.json` (verifies spec: PDSK)
- [ ] A power cut while the packages install, then running the step again, finishes the install and the step reads `done` (verifies spec: PDSK, RUN)
- [ ] On the provisioned Iti's first boot, the app offers network configuration and reports the board's radios, not "This device has no wireless radio" (verifies spec: PDSK, NET)
- [ ] Run again with everything in place, it passes without writing (verifies spec: PDSK)

## Step `encryption`

- [ ] Afterwards the root volume has a new UUID and volume key, exactly two keyslots, and the empty passphrase no longer opens it (verifies spec: PENC)
- [ ] The TPM's keyslot carries no PCR policy, and the TPM and the recovery passphrase each open the volume (verifies spec: PENC)
- [ ] The recovery passphrase is six lowercase words of five or more characters joined by hyphens (verifies spec: PENC)
- [ ] `records.tsv` on the data partition gains a tab-separated line: suffix, version, the volume's UUID, the passphrase, and a UTC time (verifies spec: PENC)
- [ ] The passphrase appears in no journal line, on the device or in the page's Activity (verifies spec: PENC, PWEB)
- [ ] A power cut during re-encryption resumes it on the next run, and the volume still opens (verifies spec: PENC)
- [ ] The step reads `done` afterwards, and re-running it passes without changes (verifies spec: PENC)

## Step `tailscale`

- [ ] The `auth-key` and `name` prompts open once `eeprom` has passed, without holding up `disk` (verifies spec: PTS)
- [ ] `name` defaults to `tamanu-iti-` and the suffix in lower case, and a name with an upper case letter or a leading hyphen is refused beneath the prompt (verifies spec: PTS, PWEB)
- [ ] The auth key field hides what is typed, says the key must be reusable, and the key is not in Activity (verifies spec: PTS, PWEB)
- [ ] With `auth-key` unanswered when its turn comes, the step waits, and the LED turns to blinking amber (verifies spec: PTS, PLED)
- [ ] A saved key from a previous board answers the prompt at once, says when it was saved, and can be forgotten (verifies spec: PWEB)
- [ ] A rejected key fails the step, drops the saved key, and the field shows again on retry (verifies spec: PTS, PWEB)
- [ ] A key needing approval passes, and the page says an admin has to approve the Iti (verifies spec: PTS, PWEB)
- [ ] After a power cut between answering and the step, both prompts open again (verifies spec: PTS, RUN)
- [ ] Afterwards the medium holds no Tailscale state (verifies spec: PTS)

## Step `bootloader`

- [ ] With the board's bootloader older than the medium's, the step updates it and leaves exactly PBL's configuration, as the flash reads back (verifies spec: PBL)
- [ ] After the step, the board boots its NVMe with the stick still in (verifies spec: PBL)
- [ ] With a configuration differing by one setting, the step reads `not-done`, names what differs, and writes the configuration (verifies spec: PBL)
- [ ] Run again with both current, it passes without writing (verifies spec: PBL)
- [ ] The step never asks the operator anything (verifies spec: PBL)

## Step `label` and the sheet

- [ ] The step passes with the board's payload, version and a time, and the page says the label was added rather than showing the payload (verifies spec: PLBL, PWEB)
- [ ] `label` run alone on a blank EEPROM still gives the version from the sensors (verifies spec: PLBL)
- [ ] The sheet is an A4 PDF in a 3 by 9 grid of 50 by 30 mm labels, the QR code scans as the board's payload, and the suffix and product read beside it (verifies spec: PSHT)
- [ ] A label printed at 100% on a laser printer scans with a phone camera (verifies spec: PSHT)
- [ ] Downloading marks the label printed, and a printed label can be ticked to print again (verifies spec: PSHT)
- [ ] Checking back at the finished board adds no second label (verifies spec: PSHT)
- [ ] Running `label` again replaces the label as unprinted (verifies spec: PSHT)
- [ ] The download action states how full the sheet is, and says it is making the sheet until it is ready (verifies spec: PSHT)

## The run as a whole

- [ ] A full run with every step ticked finishes with the LED steady green and the page saying the Iti is provisioned (verifies spec: RUN, PLED, PWEB)
- [ ] A run of some steps finishes with the page saying the Iti is not provisioned yet, offering to run steps again first (verifies spec: PWEB)
- [ ] A provisioned board that boots the medium again shows steady green from start (verifies spec: PLED)
- [ ] Closing the page mid-run leaves the run going, and pressing USER then reconnecting shows where it stands (verifies spec: RUN, PWEB)
- [ ] When the channel drops, the page keeps the last state marked stale, and offers only the chooser and a USER press (verifies spec: PWEB)
- [ ] Once the channel has dropped, the page reconnects on its own after a press (verifies spec: PWEB)
- [ ] The console and the page connected together show the same run, and an answer from either is taken (verifies spec: PCON, RUN)
- [ ] A failed run survives a reboot as `failed`, with the LED blinking red at 3 Hz, and needs a retry to go on (verifies spec: RUN, PLED)
- [ ] Stop while `waiting` or `failed` returns to `idle` and closes every prompt (verifies spec: RUN)
- [ ] The finished screen shows the recovery passphrase with a copy button, and says it is also on the medium (verifies spec: PWEB)
- [ ] A finished board rebooted from the medium comes up `idle` (verifies spec: RUN)
- [ ] The console's offer to acknowledge a finished run does not take Enter as yes (verifies spec: PCON)
- [ ] The console takes a `secret` answer without echoing it, and an empty answer as the default (verifies spec: PCON)
- [ ] The console drives a whole run with no USER press and no client (verifies spec: PCON)

## First boot of the provisioned board

- [ ] The board boots its NVMe and unlocks the root volume from the TPM, with nobody present (verifies spec: PENC)
- [ ] It comes up on the tailnet under the name given (verifies spec: PTS)
- [ ] `/proc/device-tree/hat/` names the board, and `/dev/i2c-3` and the TPM are up from the installed overlay (verifies spec: HAT, PDSK)
- [ ] It runs bliti, and the app finds it, connects, and shows the product as the board's version (verifies spec: PDSK, NFO, HAT)
- [ ] The app disconnects and connects again in the same tab without a reload (verifies spec: WEB)
- [ ] After a kernel update, the new boot slot still carries the HAT overlay (verifies spec: PDSK)

## Needing more than one board, or a faulty one

- [ ] Booted from an SD card, the device leaves the boot order alone (verifies spec: PRV)
- [ ] One medium serving two boards keeps each board's run and record apart (verifies spec: RUN, PENC)
- [ ] A board whose TPM gives no board ID advertises the `FAULT` suffix, and `encryption` refuses it (verifies spec: PRV, PENC)
- [ ] A board with no NVMe fails `disk`, `overlay` and `software`, saying so (verifies spec: PDSK)
