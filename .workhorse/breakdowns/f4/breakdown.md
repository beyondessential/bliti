# Iti v4 HAT EEPROM and provisioning

F4 stays as the umbrella and closes without merging, so each card below carries what it needs in its own description, and copies anything else it needs (board photos, mockups) from the `workhorse/f4` branch, whose working doc holds the full design. The assembly manual is already its own card (G4). The `linux-images` side (dropping its HAT-only `config.txt` lines, retiring its `pi eeprom` SD) needs a card in that repository, once provisioning installs the HAT overlay and writes the bootloader configuration itself.

## Deploy the web app · H4

The web app is a static single-page app: host it in an S3 bucket behind CloudFront in AWS, deployed by building and uploading, likely at iti.tamanu.app with the provisioning client at `/provision` and the assembly manual at `/assemble`. Add per-pull-request temporary deploys there, so every card after this one can be QA'd on a live deploy of its pull request; that is why it comes first. DONE.

## Identify a Tamanu Iti from its HAT EEPROM · J4

Fill in the placeholder C4 left in the `product` fact so a v4 identifies itself from the HAT information the Pi firmware publishes under `/proc/device-tree/hat/`. A board is a Tamanu Iti when its HAT+ `uuid` is the Iti product UUID `7c3b8a88-ad89-4d48-b709-2bd445729867`; its version is `product_id`.`product_ver` (published as text, e.g. `0x0004`), so rev B reads as "Tamanu Iti v4.0" and the third generation stays "Tamanu Iti v3". Amend NFO's Product section to major.minor, and specify the EEPROM's contents (HAT+ format, vendor "BES International", product "Tamanu Iti v4.0", `product_id` 4, `product_ver` 0 for rev B and 1 for rev C, and the `tamanu-iti-v4` overlay name). Bring the `tamanu-iti-v4` overlay source into the repo (SPI0 with the TPM on CE1 as `tcg,tpm_tis-spi`, I²C3 on GPIO22/23, no sensor drivers bound, and the green LED taken off the SD card activity trigger, which with no card can stick and leave it dark) along with the EEPROM image definition; the v4 prototype is already programmed with both. DONE.

## Report the HAT's air and light readings · L4

Add facts for what the rev B HAT's sensors measure, on I²C3 (GPIO22/23) as the Iti overlay sets it up: the Bosch BME680 at `0x77` (temperature, humidity, pressure, and its gas sensor's resistance as a proxy for air quality) and the Lite-On LTR-308ALS at `0x53` (ambient light). The overlay binds no sensor drivers so provisioning's hardware check can read the chips directly; this card decides whether to bind kernel drivers or read the chips from bliti, and keeps that check working either way. It also decides whether the gas sensor's raw resistance is enough, since Bosch's air quality index comes from their closed library. Which readings a device reports follows its hardware version from the HAT EEPROM: rev C is expected to carry a cheaper air sensor that may lack some of these. DONE.

## Report whether the case is open · M4

Add a fact for whether the Iti's case is closed, from the CASE switch on GPIO17 (low while the lid presses it). The card decides whether opening the case is also recorded as an event, with when it happened, beyond the current state. DONE.

## Run provisioning mode on the device · K4

The device side of provisioning, without its steps: bliti in provisioning mode on the provisioning medium only, silent until USER (GPIO27) is pressed, then advertising for five minutes under a well-known presence token with the board's future four-character QR suffix in its name, and stopping when a client connects. A run belongs to the device: chosen steps run in order whether or not a client is connected, the run's state is kept on the medium so it resumes after any reboot, and each step reports whether it is already done so the client can offer it unticked. The protocol stream the client drives runs with, a console command running the same steps with text prompts, and the Pi's LED showing the run's state (steady amber when booted from the medium, green slow blink working, amber blink waiting on a person, red fast blink failed, green steady finished; green is active-low) are part of this card. DONE.

## Build the provisioning image · P4

Assemble the provisioning image in this repo from the `pi` image `linux-images` publishes, rather than building an OS: a starting sketch is to fetch the latest `pi` image, mount it, write the provisioning files into its `FIRMWARE` FAT partition, and pack it again, in GitHub Actions (and push it to S3) or a container build. The card settles how bliti in provisioning mode gets onto that system, where the NVMe payload (the same original `pi` image) sits on the medium and how it is checked (ideally dm-verity, otherwise a checksum), and how the newest bootloader on Raspberry Pi's default channel and the HAT overlay travel with it. The one image is written to a USB stick for the factory or an SD card for repair. Provisioning mode itself is built (K4): the image enables `services/bliti-provision.service` by providing `/etc/bliti/provision.env` with its arguments (`--medium` pointing at the mounted medium, writable for the run store, and `--board-id-source tpm-endorsement-key`), and must not run the normal bliti service, which would hold the CASE switch's GPIO line, advertise as an ordinary Iti, and keep a case history the HAT spec forbids on the medium. The overlay and the EEPROM image are built from `hat/` (`just hat-overlay`, `just hat-eeprom`); the image loads the overlay itself, since a fresh board's EEPROM names nothing, and carries the EEPROM image for the EEPROM step.

refer to #F4's working doc for any missing details

## Build the provisioning client page · Q4
<!-- mockups: provisioning -->

The client at `/provision`, a second entry point in the existing web build, not linked from the operator app, sharing its channel, protocol and wasm code and its components. Finding the board (press USER, then the chooser filtered on the provisioning marker), choosing the steps (already-done ones unticked, with why), the running step list with each step's prompt beneath its row, checking back after closing the page, a failed step's retry, and the finished record with the recovery passphrase. Bring the three circled rev B board photos (USER button, CASE switch, ID WP pads) from the `workhorse/f4` branch into the app, captioned by the client; the uncropped original sits beside them there, for re-cropping. Link to the assembly manual and back. The page is a flat top-level `provision.html` (served at `/provision`), picked up by the build and deployed with no change, and each pull request gets a preview at `pr-<n>.iti.tamanu.app` for QA (H4); whichever of this card and the assembly manual (G4) lands first decides whether the service worker precaches the new pages. It speaks K4's provisioning stream: `provision-state` (run, steps, prompts) on every change, and `provision-act` (start, answer, retry, stop, acknowledge), with the chooser filtered on the `ITI_PROVISION_` name prefix; the device takes one client at a time.

refer to #F4's working doc for any missing details

## Check the hardware and program the EEPROM

The first two provisioning steps. The hardware check counts the USER press that found the board, waits for CASE (GPIO17, active low while the case is closed) to be pressed and released, reads the BME680 (chip ID `0x61` at `0x77`) and the LTR-308 (part ID `0xb1` at `0x53`) on I²C3, and confirms the TPM answers; the BME680 places the board as v4.0, and a board it can't place stops without guessing. The EEPROM step writes the image for that version over the ID bus (CAT24C256 at `0x50`), retrying for up to five minutes while the assembler bridges the ID WP pads with bare metal such as scissors (coated tweezers don't conduct; an unbridged write fails with an I/O timeout), tells them to let go, and verifies by reading back; an EEPROM already holding the same contents counts as done, and one holding something else is overwritten only once the assembler confirms. Both are steps against K4's `Step` trait (`provision/step.rs`), added to its registry; the check's `done()` always says not done. Read the sensors through L4's I²C bus and chip checks (`facts/i2c.rs`, `facts/environment`) and CASE through the GPIO module M4 and K4 use, rather than new code; provisioning mode runs no sampler or case watch, so nothing else holds the bus or the line. The EEPROM step's `done()` reads the chip itself: what the firmware publishes under `/proc/device-tree/hat/` only changes at the next boot. Prompts name parts by their silkscreen labels, as K4's spec requires.

refer to #F4's working doc for any missing details

## Write the disk and set up encryption

Write the `pi` image from the medium to the NVMe and read it back, then install the HAT overlay the image carries (built from `hat/`) into the new system (its boot slot's `current/overlays/` and `/etc/flash-kernel/dtbs/overlays/`, so kernel updates keep it). Both are steps against K4's `Step` trait, and must be safe to restart from the beginning, as K4 resumes an interrupted step that way. Rotate the LUKS volume key (or leave it to the image's first-boot rekey, which this card decides), enrol a TPM-only keyslot, and replace the empty placeholder passphrase with a recovery passphrase made the way the `linux-images` installer makes one (six lowercase diceware words joined by hyphens). Append the board's record (QR suffix, version, LUKS partition UUID, recovery passphrase, time) to a tab-separated file on the medium's FAT partition. The card also settles how the device recognises the NVMe already carries the medium's system, and whether a PCR policy is worth binding on the Pi.

refer to #F4's working doc for any missing details

## Configure the bootloader during provisioning

A run started from a USB stick puts USB first in the boot order before any step, so a reboot mid-run comes back to the stick. As the run's last step, write the Iti's whole bootloader configuration (`BOOT_ORDER=0xf61`, `POWER_OFF_ON_HALT=1`, `PSU_MAX_CURRENT=5000`, `PCIE_PROBE=1`, `BOOT_UART=1`, `HDMI_DELAY=0`) and update the bootloader to the one the image carries wherever the board's is older, never downgrading. Provisioning becomes the one place the Iti's bootloader is configured. The card confirms when a configuration written from Linux takes effect. K4's engine runs only steps, so putting USB first is not a step: provisioning mode does it when it starts, whenever it was booted from a USB stick, and the final configuration is the last step in the registry.

refer to #F4's working doc for any missing details

## Sign the Iti in to Tailscale during provisioning

Ask for a reusable Tailscale auth key with a week expiry (if one doesn't exist in local storage yet) and the Iti's name right after the EEPROM step without holding the run up (the disk write starts at once), keep the key in local storage (client-side), and sign in against the new system's Tailscale state once its encrypted root is open, so the Iti's first boot comes up on the tailnet. A power cut before the sign-in loses the key, and the step then waits for it again. Only auth keys reach the device; where the tailnet uses an OAuth client, the browser mints the key. The card settles the naming scheme, how the client holds the credential for a session of boards, and whether a browser can mint keys. K4's step context already provides the ask: `ask_later` prompts without blocking, answers are held in memory by key and never written to the store, and a step run again gets the held answer.

refer to #F4's working doc for any missing details

## Print QR labels for provisioned Itis
<!-- mockups: label-sheet -->

The client produces an A4 PDF of 50 × 30 mm labels in a three-by-nine grid with cut lines, for sticker sheets on a laser printer, holding a label for each board this browser has provisioned since its last sheet. Each label carries the QR code (version 5 at level H, filling the height with its quiet zone), the four-character suffix, and "Tamanu Iti v4.0"; never the recovery passphrase. A production label printer is chosen later. The client needs each board's full QR payload to draw its code, and K4 only derives the suffix (for the advertised name), so this card also has the device hand the payload over, for example as a step's result.

refer to #F4's working doc for any missing details

## Update the TPM firmware during provisioning

Update the ST33KTPM's firmware wherever it is older than the image the medium carries, never downgrading, before anything takes the TPM's identity or keys; the reset is a reboot the run survives. K4 restarts an interrupted step from its beginning after a reboot, so the step checks the firmware version first and passes straight through once it is current. The prototype runs `0x00090101`, and newer firmware needs the LMS-signed manifest path (wolfTPM's `st33_fw_update` handles both). Blocked until BES obtains the firmware images from ST, which are not public.

refer to #F4's working doc for any missing details

## Lock the Iti's boot with Raspberry Pi secure boot

Close the gap the TPM-only keyslot leaves: anyone who can boot their own system on the board can have the TPM unseal the disk. Secure boot signs the boot images and burns a customer key into each board's OTP, irreversibly, which would become another provisioning step.

refer to #F4's working doc for any missing details

## Keep a central register of provisioned Itis

Send each board's record to a central register, in place of gathering the records files from each provisioning medium.

refer to #F4's working doc for any missing details