---
status: draft
---

# Iti v4 HAT EEPROM

What a v4 board's HAT EEPROM holds, how bliti programs and verifies it during provisioning, and how bliti reads it back to identify a Tamanu Iti and its generation.

## Behaviour

### What the EEPROM holds

The EEPROM conforms to the Raspberry Pi HAT+ specification (header format version `0x02`): a header, a vendor-info atom, and a Device Tree overlay name atom, which HAT+ requires.

What HAT+ defines for the vendor-info atom:

| field | HAT+ says | Iti holds | bliti reads it |
| --- | --- | --- | --- |
| `uuid` | 16 bytes, "product UUID (unique)", required; unique per product and preserved across flashes (the tools say to keep the one generated) | `9c2bcc02-123a-4d9d-80e0-72166bec3218`, the Tamanu Iti product UUID | yes: recognises a Tamanu Iti |
| `product_id` | 16-bit integer, optional, zero if unused; meaning is the vendor's | the generation: 4 | yes: major version |
| `product_ver` | 16-bit integer, optional, zero if unused; meaning is the vendor's | the revision within the generation: 0 for rev B, 1 for rev C | yes: minor version |
| `vendor` | ASCII string, up to 255 bytes, required | "BES International" | no |
| `product` | ASCII string, up to 255 bytes, required | "Tamanu Iti v4.0" | no |

A board is a Tamanu Iti when `uuid` is the Iti product UUID, and its version is `product_id`.`product_ver`.
The strings are for anyone reading the board, and bliti ignores them.

There is no vendor ID registry, so `product_id` and `product_ver` mean something only alongside the product UUID; bliti never reads them from a board whose UUID isn't the Iti's.

The legacy HAT format (version `0x01`) made `uuid` unique per board; HAT+ made it per product, which is what lets it be the well-known identifier.
HAT+ has no per-board serial field. Each board is already identified by its board ID (BID, the TPM Endorsement Key name), which the board's record carries through its QR suffix, so the EEPROM carries no serial of its own.

HAT+ also defines a manufacturer custom data atom (type `0x0004`) for anything vendor-defined; nothing needs it yet.

The overlay name atom names the Device Tree overlay the firmware loads for the HAT, from the overlays on the boot partition.
An Iti overlay per generation configures the HAT's devices (TPM, sensors, buttons) from the hardware itself, rather than from `config.txt` lines every image carries (`linux-images` currently enables the TPM overlay unconditionally).

### How C4 consumes it

The `product` fact (NFO, Product) reports "Tamanu Iti v" plus the generation where the HAT EEPROM identifies a Tamanu Iti.
This card fills in the placeholder C4 left (`facts::product::generation`) with the real match, and amends NFO to major.minor (see Reporting the version).
The placeholder takes `vendor` and `product`; the real match reads `uuid`, `product_id` and `product_ver` instead, as the text the firmware publishes (`0x0004`), so its inputs change too.

### Provisioning

A provisioning mode guides the assembler through programming a board's EEPROM.
The EEPROM is write-protected unless two test pads are bridged (the HAT design guide's WP-to-test-point pattern).
The flow:

1. Detect the EEPROM, and whether it is already programmed
2. Tell the assembler to bridge the pads, with a picture of the board showing where: the rev B photo with the ID WP pads circled, captioned in the client (`.workhorse/design/assets/iti-hat-rev-b-wp-pads.jpg`, beside the USER button and CASE switch pictures), shipped with the provisioning client
3. Repeatedly try to write until a write succeeds
4. Tell the assembler they can let go
5. Read back and verify

The v4 prototype goes through the same flow.

Which board version to write is inferred from the hardware, not configured or asked.
Each board version is expected to be detectable from the hardware present via GPIO and peripherals: v4 rev B (this HAT) carries specific devices checkable over I²C, and v4 rev C will differ.
Board versions are reported as major.minor: rev B is v4.0, rev C v4.1.

Where the EEPROM already identifies a Tamanu Iti of the version being provisioned, the step compares the rest of its contents against what it would write, since there may be updates for it.
It skips if they match and rewrites if not.

Where the EEPROM holds something else (another vendor's data, or another Iti version), the step shows what it holds and asks the assembler to confirm before overwriting.

The write retries for up to five minutes waiting for the pads to be bridged, then gives up, tells the assembler to check the bridge, and offers to try again.

Provisioning mode exists only on the provisioning medium; an imaged board's own bliti never enters it.
Two front ends drive the same device-side steps: the provisioning client over the channel, and a command on the provisioning system's console (HDMI and keyboard, or the serial console the image already enables) for a bench without a browser.
The console command runs the same steps in the same order, with text prompts in place of the picture.

### The provisioning medium

Provisioning happens from a medium carrying an OS, bliti in provisioning mode, and a copy of the image for the Iti's NVMe.
Booted on a fresh Iti, it programs the EEPROM, flashes the NVMe, binds the LUKS partition to the TPM, and produces the QR code as a printable.
It needs no QR code to start.

One image serves as either medium:
- **A USB stick, at the factory.** The Iti is built in its final form, case and all, with the case left open so CASE can be tested, and the stick goes in a free USB port. USER can be reached through the case anyway.
- **An SD card, for repair.** The finished hardware physically blocks the Pi's SD slot, and a provisioned board no longer boots from USB (see Configuring the bootloader), so provisioning a board again means taking it apart to reach the slot. That is acceptable for a board coming back for repair, and it is what keeps a passer-by from booting their own system on a deployed Iti.

Each step is offered separately and can be skipped, so a board that is already imaged (the v4 prototype, say) can have only its EEPROM programmed.

#### Order of steps

Before any step, a run started from a USB stick puts USB first in the boot order, so any reboot during the run comes back to the stick (see Configuring the bootloader).

1. Check the hardware (first, so a bad board fails fast, and because the air sensor read places the board's version for the EEPROM step)
2. Update the TPM's firmware (before anything takes the TPM's identity or keys; its reboot comes while the assembler is still at the bench)
3. Program the EEPROM
4. Ask for the Tailscale credential and the Iti's name, without waiting for them
5. Flash the NVMe and read it back
6. Rotate the volume key, enrol the TPM keyslot, and replace the placeholder passphrase with a recovery passphrase
7. Sign the Iti in to Tailscale, under the name chosen for it
8. Configure the bootloader (last, so a run that fails before it can still be re-run from the stick)
9. Render the label and save the board's record

Steps 1 to 3 need the assembler. Step 4 asks and moves on: the disk write starts at once while the Tailscale prompt stays open beside it, and only the sign-in step needs the answer, waiting for it there if it has not come. So the assembler does everything up front, fills in the Tailscale details while the disk writes, and walks away.

The assembler chooses which steps to run; the rest are skipped.
Before offering the choice, the device looks at what each step would change and leaves the steps already done unticked, each saying why (for example "Up to date"), so a board that is already set up is offered only what it lacks; the assembler can tick any back on.
Where it cannot tell, a step stays ticked.

| step | done when |
| --- | --- |
| Check the hardware | never: it is a check, not a change, and always ticked |
| Update the TPM firmware | the TPM's firmware is as new as the image the medium carries |
| Program the EEPROM | it identifies an Iti of the version the hardware shows, with the contents this medium would write |
| Write the system to the disk | the NVMe's plaintext boot partition shows the same system the medium carries; older, or not telling, is not done |
| Set up disk encryption | the LUKS header carries a TPM keyslot and no empty placeholder passphrase |
| Sign in to Tailscale | the system's Tailscale state is signed in (read by unlocking its root through the TPM) |
| Set up the bootloader | its configuration and version match the Iti's |
A step that fails stops the run and says why, and the assembler can run it again without repeating the steps before it.
The label and record are offered at the end of any run whose chosen steps all passed, since the QR code comes from the TPM board ID whichever steps ran.
At the end the client tells the assembler to power the board off and remove the provisioning medium.

#### The run belongs to the device

Writing the image from the provisioning medium to the NVMe is slow, so a run carries on whether or not a client is connected: the client starts it and watches it, and closing the page or walking away leaves it running.
That lets an assembler start several boards, each with its own stick, and check back on each, or do one at a time without keeping a phone beside it.

The steps that need a person come first (the hardware check, the EEPROM's bridge, and the Tailscale credential and name, with the brief TPM firmware update among them), and the long unattended ones after (NVMe, keys, Tailscale sign-in, bootloader), so the assembler is needed only at the start.
The device watches for those actions itself (CASE's lever, the bridge), so a person can complete them with no client connected; only confirming an EEPROM overwrite needs an answer, from a client or the console.
A run that fails stops at that step and waits.

Pressing USER puts the board back on the air for five minutes whatever the run is doing, and connecting shows where it is: the step list with its states, or the finished record. Choosing steps only comes up when nothing is running.
The finished record and label wait on the device for whichever client connects next, and the record is on the provisioning medium regardless.
While provisioning, the Pi's status LED shows the run's state, so a bench of boards can be read at a glance:

| state | LED |
| --- | --- |
| booted from the provisioning medium, nothing running | amber, steady |
| working | green, slow blink |
| waiting on a person (CASE, the bridge, an overwrite to confirm, the Tailscale credential once the sign-in step is reached without it) | amber, blink |
| failed | red, fast blink |
| finished | green, steady |

Amber is red and green on together, and marks a board running from the provisioning medium, so one that would otherwise ship with it still in stands out.
A finished run is not picked up again, so any later boot of a finished board from the medium comes back as steady amber; green steady lasts only until the board is powered off, which is the next instruction anyway.

Only the provisioning medium drives the LED this way; the Iti's own system leaves it as the Pi sets it.
The Pi 5's one LED is two in Linux, green (ACT) and red (PWR), each with its own control; that bliti can drive them from the provisioning system is still to be checked on the prototype.

A run survives a reboot: its state (the steps chosen, and which have passed) is kept on the provisioning medium against the board, and a board that boots back into provisioning picks its run up where it left off, starting an interrupted step over (USB is first in the boot order for the run's duration, so a reboot comes back to the stick).
That covers both the reboot the TPM firmware update needs and a power cut partway through a long run.
A finished run is not picked up again, so a finished board booted with the stick still in shows steady amber.

Each stick keeps its own records file, so a bench of several sticks spreads the records across them until they are gathered; the central register card is what fixes that properly.

#### Updating the TPM's firmware

The ST33KTPM takes firmware updates in the field: the TCG field upgrade sequence (or ST's vendor equivalent), streaming a signed image from ST and then resetting the TPM.
Provisioning carries the newest image it has and updates the TPM wherever its firmware is older, never downgrading; the reset is a reboot, which the run survives.

The TPM 2.0 specification says only that a field upgrade *should* keep the primary seeds, and the board ID is the name of a key derived from the endorsement seed, so an update could in principle change it.
Updating at provisioning makes that harmless: the step runs before the board ID is first used for anything lasting (the label, the record) and before the disk's keyslot is sealed to the TPM.
The client loses the board across that reboot, and the assembler checks back with USER as at any other time; if the update did change the board ID, the suffix shown from then on is the new one, and it is the one the label carries.
Updating a deployed Iti's TPM is not part of this, and would need the board ID shown to survive first.

The firmware images are not public: they come from ST, and someone at BES has to obtain them.
Older ST33 parts allow only a limited number of firmware updates; none is documented for the ST33K, but updating only when older keeps the count down either way.

#### Flashing the NVMe

The image is the `linux-images` `pi` variant.
Its LUKS2 root ships with an empty placeholder passphrase and the image's own volume key; the volume key is rotated per board (`linux-images` already supports this: the `pi` image rekeys itself on first boot, `image.identity.luks-rekey`).
After writing, provisioning reads the NVMe back and checks it against the image.
The image copy on the provisioning medium is ideally carried in a dm-verity partition, as the `linux-images` installer medium does; a plain checksum otherwise.

#### Binding to the TPM

Hybrid: provisioning enrols a TPM-only keyslot (no PCR policy), which works from the provisioning boot and ties the disk to this board, and replaces the empty placeholder passphrase.
A first-boot enrolment on the NVMe system then rebinds with PCRs, if the Pi can measure anything worth binding to.
A recovery passphrase is generated at provisioning. It is never included alongside the QR code, which would display on the outside of the device.

What the TPM-only keyslot protects: the disk away from its board.
Anyone who can boot their own system on the board can have the TPM unseal for them; with no measured boot on the Pi, a PCR policy cannot tell their OS from ours.
A provisioned Iti boots only from SD and NVMe, and its SD slot is physically blocked, so booting another system on it means taking it apart; that keeps out someone passing by, but not someone who can take the device away.
NVMe-first would not close the gap (boot with the NVMe out, then attach it over USB) and would stop a repair SD taking over an imaged board, so SD stays first.
Closing it takes Raspberry Pi secure boot (signed boot images, a customer key burnt into OTP, irreversible per board). That is a separate card, though its OTP programming would naturally be another provisioning step.

#### Signing in to Tailscale

The run signs the Iti in to Tailscale, so it comes off the bench already on the tailnet, in one tool and one pass.
The run executes on the provisioning system, while Tailscale's sign-in belongs to the Iti's own system on the NVMe, so the step signs in against the new system's Tailscale state, with its encrypted root open from the step before; the Iti's first boot comes up signed in.
The auth key and the name are asked for early in the run (step 4), without holding it up: the disk write goes ahead while the prompt waits, and the assembler can walk away once they have answered, and are used by the sign-in step once the encrypted root is open, as CTL's `sign-in` by `auth-key` takes them (reusing its tailnet check).
The device holds the key in memory only and never writes it down, as CTL already requires of a device; only the node state that results is, which is what any signed-in machine keeps.
A power cut between the ask and the sign-in loses the key: the run resumes, and the sign-in step waits for the key again as a step waiting on a person.
The device only ever receives an auth key. Where the tailnet issues keys through an OAuth client, the client mints the auth key from it and the OAuth secret stays with the client, since it can mint keys for the whole tailnet.
The assembler enters the credential in the client once for a session of boards, and picks or accepts a name per board.
The name is asked after the TPM firmware update, the one step that could change the board ID, so a name built from the suffix is built from the final one.
Where the step is skipped or fails, the Iti is signed in afterwards through the operator app's Tailscale screen, which already does this.

#### Configuring the bootloader

The Pi 5's own default, as Raspberry Pi ships it, is `BOOT_ORDER=0xf461` (SD, NVMe, USB, then round again) with `NET_INSTALL_AT_POWER_ON=1`. A fresh board's NVMe is blank, so it falls through to a USB stick; but once a run has written the NVMe, the stock order would boot that over the stick.
So the bootloader is touched twice:
- **At the start of a run from a USB stick**, USB goes first, ahead of SD and NVMe, so a reboot once the NVMe is written (a power cut, or a failed run to be re-run) comes back to the stick.
  A board that already boots from its NVMe and has never had a run put USB first (the v4 prototype, or a board imaged some other way) does not reach a USB stick at all, and goes through an SD card.
- **As the run's last step**, it is set to the Iti's whole bootloader configuration, not just the boot order:

  | setting | value | for |
  | --- | --- | --- |
  | `BOOT_ORDER` | `0xf61` | SD, then NVMe; no USB, and no network install |
  | `POWER_OFF_ON_HALT` | `1` | the Pi cuts its own power on halt, which the backup supply relies on |
  | `PSU_MAX_CURRENT` | `5000` | |
  | `PCIE_PROBE` | `1` | |
  | `BOOT_UART` | `1` | |
  | `HDMI_DELAY` | `0` | |

  The step also updates the bootloader to the one the provisioning image carries, wherever the board's is older, and never downgrades one.
  The image takes the newest bootloader on Raspberry Pi's "default" release channel when it is built, so updates are picked up each time `linux-images` rebuilds it; provisioning needs no network, and every board from one image gets the same bootloader. That bootloader is always well past the tryboot floor the `pi` image needs (firmware dated 2025-02-11 or later).

Provisioning is the one place the Iti's bootloader is configured.
These settings are carried over from the `linux-images` `pi eeprom` SD (`image/pi-eeprom-config.txt`), which is retired: `linux-images` stops building it, and its spec and tests go with it.

A run that fails before its last step leaves USB first, so the board can be re-run from the stick.
A run from an SD card (a repair) needs neither change for itself, since SD is first in both orders, but still ends with the Iti's configuration.

#### The QR label

The label goes on the bottom of the case, out of sight in normal use, in a space of 50 × 30 mm clear of the vents.
It carries the QR code on the left, filling the height with its quiet zone (version 5 at level H is 45 modules with the quiet zone, so modules of about 0.6 mm), and on the right the four-character suffix, large, over "Tamanu Iti v4.0" in small type so support can tell the hardware version from outside.
The recovery passphrase never goes on it.

The production label printer and stock are not chosen yet.
Until then the client produces an A4 PDF of 50 × 30 mm labels in a three-by-nine grid with cut lines, for sticker sheets on an ordinary laser printer, cut out by hand.
The sheet holds a label for each board this browser has provisioned since its last sheet, so a bench of boards fills one sheet; the client remembers them across its tabs.
A lost or damaged label is reprinted by running only the label step on that board, or by reading the old code with the operator app's QR export (WEB).

Mockup: the QR label sheet (`.workhorse/design/mockups/f4/label-sheet.html`), at true scale.

#### Board records

At the end, each board's record is shown to the assembler in the client and appended to a file on the provisioning medium's FAT partition, so it can be read on any computer.
One line per board, tab-separated, in the manner of the `linux-images` installer's `recovery-keys.txt`: the QR suffix, the version, the UUID of the NVMe's LUKS partition (to match a disk to its passphrase), the recovery passphrase, and when it was provisioned.
The recovery passphrase is made the way the `linux-images` installer makes one, so it is easy to type at a keyboard: six lowercase words of at least five letters, drawn from the miniLock diceware word list and joined by hyphens.
Sending it to a central register is a separate card.

#### Where it's built

Proposed, to confirm: bliti builds the provisioning image, for writing to a USB stick or an SD card, by assembling it from the `pi` image `linux-images` publishes rather than building an OS of its own.
The `pi` image is its OS (the same system, already booting on the Pi with its overlay and boot-slot plumbing), with bliti in provisioning mode and the provisioning boot configuration added, and a partition carrying the `pi` image again as what is written to the NVMe.
Everything Iti-specific then lives in bliti (EEPROM contents, HAT overlay, bootloader settings, provisioning, the image, and the assembly manual of G4), changing together in one repo; `linux-images` stays a generic Pi OS that knows nothing of Itis, and retires its `pi eeprom` SD.
The cost is an image-assembly build in bliti, far smaller than `linux-images`' own since it composes a published image.

### Reaching a provisioning device

A device in provisioning mode uses a well-known, published presence token instead of one derived from its board ID, so any client can open a channel to it without a QR code.
Trust comes from being physically at the assembly bench. The client cannot authenticate the device, since there is no key fingerprint to check against.

Its advertised name carries a provisioning marker plus the last four characters of the QR payload the board will have once provisioned.
That suffix comes from the TPM board ID through the key fingerprint, is safe to broadcast, and is the same four characters STAT shows as a device's fallback name and that appear on the printed label.
A provisioning device is silent until USER is pressed, then advertises for five minutes.
It stops advertising when a client connects (the controller already does this on connection; outside provisioning bliti re-registers to resume, and in provisioning it doesn't), and stays silent after the session ends until USER is pressed again.
So the boards on the air are the ones just pressed and not yet connected, normally exactly one.
The client opens the chooser filtered on the provisioning marker; where several boards were pressed at once, the suffix in each name tells them apart.

Where the TPM doesn't answer, there is no board ID to take a suffix from (falling back to the Pi serial would print a label that stops matching once the TPM works), so the device advertises a fault marker in place of the suffix, the hardware check fails on the TPM, and no label is offered.

The client works on one board at a time; several boards at once means several tabs.

Pressing USER is how the assembler picks the physical board, and also what puts it on the air.
A board whose USER button doesn't work can't be reached by the client; the console command provisions it, and its hardware check fails on USER.

### The provisioning client

The provisioning client is a second page in the existing web build, at `/provision`, not linked from the operator app (the web app's address is likely iti.tamanu.app, to confirm).
It links to the assembly manual (G4) at `/assemble`, and the manual links back, taking the builder from parts to provisioning to closing up and testing.
It shares the channel, protocol and wasm code with the operator app.

Its flow, one board per tab:

Each board picture carries only an orange circle; the client captions it with where the part is and what to do, so the words stay legible at phone width.

1. **Find the board.** Links to the assembly manual ("How to build an Iti"). Asks the assembler to press USER on the board, with the board picture showing where (`iti-hat-rev-b-user-button.jpg`), and offers finding it as the primary action: the chooser, filtered on the provisioning marker.
2. **Choose the steps.** Shows the board's suffix and the steps in order, each with a checkbox, ticked unless the device found it already done (with why beside it), and starting as the primary action.
3. **Run.** The step list stays on screen with each step's state (waiting, running, passed, failed, skipped); the current step's prompt sits beneath it:
   - Hardware check: USER already ticked from the press that found the board, then press and release CASE (`iti-hat-rev-b-case-switch.jpg`), then the sensor and TPM results
   - EEPROM: what the EEPROM holds now and what will be written; where it holds something else, confirming the overwrite; then "Bridge these pads" with the pads picture (`iti-hat-rev-b-wp-pads.jpg`), switching to "You can let go" once written, then the verify result
   - TPM firmware: updated or already current; the board drops off for its reboot and the assembler checks back with USER
   - Tailscale details, under the Tailscale row once the EEPROM is done, while the disk write runs above it: the auth key (kept in the browser for the session, so entered once for a bench of boards) and the name (a suggestion to accept or change), then "Save"; the last thing asked, after which the row reads as ready under its name until its turn
   - NVMe, keys, Tailscale sign-in and bootloader: progress, with the NVMe write and read-back as a progress bar
   - A failed step says why and offers to try that step again, or to stop
Every running screen says the page can be closed and that pressing USER checks back.
Finding a board that is mid-run goes straight to its step list.

4. **Done.** The board's record, with the recovery passphrase and a copy action; the label added to the browser's pending sheet, and downloading the sheet as the primary action; then "Power off the board and remove the USB stick".

Mockup: provisioning an Iti, every screen (`.workhorse/design/mockups/f4/provisioning.html`). It reuses the operator app's own components: the step dots of reaching a device, the Status screen's rows with their marks, and its bar-state panels; the only new pieces are a board picture inside a step, and a step's prompt beneath its row.

The console command follows the same steps with text prompts: it names the pads and buttons by their silkscreen labels (ID WP, USER, CASE) in place of the pictures, and prints the record at the end.

### Reporting the version

A HAT-identified board reports as "Tamanu Iti v" followed by its major.minor version: "Tamanu Iti v4.0" for rev B.
The third generation carries no EEPROM to tell revisions apart, and stays "Tamanu Iti v3".
This changes NFO's Product section ("Tamanu Iti v" followed by the generation), which this card amends.

### The HAT, rev B (silkscreened "ITI 4.0")

The HAT's 2×10 header sits on the Pi's physical pins 9 to 28.

| part | device | connection | notes |
| --- | --- | --- | --- |
| U1 | onsemi CAT24C256 EEPROM, 32 KiB | ID bus, GPIO0/1, address `0x50`, 3.9 kΩ pull-ups | write-protected unless the ID WP pads are bridged (grounding WP) |
| U2 | ST ST33KTPM2X32DKG9, TPM 2.0 | SPI0 CE1 (GPIO7); reset GPIO24; interrupt GPIO25 | the BID board-ID source; GPIO8 (CE0) is reserved, not the TPM's chip select |
| U3 | Bosch BME680, air | sensor bus, address `0x77` (the handoff says `0x76`; the board answers at `0x77`, chip ID `0x61`) | no interrupt; rev C is expected to carry a cheaper air sensor (possibly a BMP580) |
| U4 | Lite-On LTR-308ALS-01, light | sensor bus, address `0x53` | interrupt on GPIO18, active low |
| SW1 | CASE lever switch | GPIO17 | active low while the case is closed |
| SW2 | USER button | GPIO27 | active low while pressed; the button the assembler presses to select a board |

The sensor bus is GPIO22 (SDA) / GPIO23 (SCL), which the Pi 5 drives as I²C3 (`i2c3-pi5` with `pins_22_23`), with 4.7 kΩ pull-ups.
Both buttons have external pull-ups and series resistors.
GPIO15 is spare.

### Inferring the version, and where it stops

Provisioning tells rev B from later revisions by which air sensor answers on the sensor bus: a BME680 (chip ID `0x61` at `0x77`) means v4.0.
This kind of detection by positive markers is what C4 avoided, because a dead or missing part misreads.
Here it is used only once, at provisioning, to decide what to write; from then on the EEPROM is authoritative and the running system never infers from parts.
Where provisioning can't place the board as a known version (the sensor that tells revisions apart doesn't answer, or a combination it doesn't know), the EEPROM step stops and says the board needs looking at, without guessing.

### Checking the hardware

Before the label is printed, provisioning exercises the HAT so a bad board is caught on the bench and not in the field.
The press of USER that put the board on the air counts as USER's check; the assembler then presses and releases the CASE lever; the device reads the air and light sensors once, and confirms the TPM answers.
Each check passes or fails in the client.
A sensor passes when it answers with its expected chip ID and returns a reading; the TPM passes when it answers and yields the board ID.
Doing anything ongoing with these devices (case-open alerts, environment readings) belongs to other cards.

The hardware check runs before the EEPROM is programmed, so on a fresh board the firmware has configured none of the HAT's devices.
The provisioning image's own `config.txt` therefore carries the HAT's configuration explicitly; it is a bench tool for known hardware, so it doesn't need the EEPROM to tell it.

### The HAT's overlay

HAT+ puts an overlay *name* in the EEPROM, not `config.txt` lines; the firmware loads that `.dtbo` from the boot partition.
The sensor bus needs `i2c3-pi5` with a parameter (`pins_22_23`), which a bare name can't carry, so the HAT needs an overlay of its own rather than a list of upstream ones.

One Iti overlay per hardware version that needs a different one (`tamanu-iti-v4` to start), installed by provisioning, configuring:
- SPI0 with the TPM on CE1 (what `tpm-slb9670` does today)
- I²C3 on GPIO22/23

It enables the buses and does not bind the sensors' kernel drivers: a bound driver holds the address, which would stop provisioning (and anything else reading the chip directly) from reaching it.
Whether the sensors get kernel drivers, and whether the buttons become `gpio-keys` input devices, is for the cards that use them; the overlay grows then.

Provisioning installs it into the new system right after writing the NVMe, in two places: the boot slot's `current/overlays/`, and `/etc/flash-kernel/dtbs/overlays/`, which `flash-kernel` searches (independent of kernel version) when it stages each new boot slot. The provisioning image carries it in its own boot slot too, for the hardware check. (If the image is built in `linux-images` after all, `linux-images` ships it in those places instead.)

With the overlay named in the EEPROM, `linux-images` drops the lines only the HAT needs (`dtoverlay=tpm-slb9670`, and `dtparam=spi=on` if nothing else uses SPI).
`dtparam=i2c_arm=on` stays: the backup supply's gauge on bus 1 is not on this HAT, and v3 has no EEPROM to name it.

### Prototype experiment: devices from the EEPROM alone

What depends on the TPM on the prototype (checked):
- LUKS root unlocks from an empty keyfile, not the TPM
- Tailscale's state is plain JSON, not TPM-sealed
- bliti's board ID: without the TPM it falls back to the Pi serial, so the QR label stops matching and BLE is unreachable until the TPM is back (C4 would also report it as "Tamanu Iti v3")

Removing the `config.txt` line with the EEPROM still blank has a known outcome, since nothing else names the overlay: the TPM stays down.
The informative run is the other order:
1. Install the Iti overlay on the boot partition; program the EEPROM naming it, with the `config.txt` line still in place; reboot; read `/proc/device-tree/hat/` back (also settles how the firmware renders `uuid`, `product_id`, `product_ver`) and check I²C3 is up with both sensors answering
2. Remove the `config.txt` line; reboot; check `/dev/tpm0` comes up
3. Put the line back if it doesn't

Prepared on the prototype (13:25, 28 Oct):
- `tamanu-iti-v4.dtbo` in `/boot/firmware/current/overlays/`: SPI0 with the TPM on CE1 (`tcg,tpm_tis-spi`, the generic TIS-over-SPI binding, which the kernel's driver accepts and which describes the ST33K accurately), and I²C3 on GPIO22/23 at 100 kHz
- The v4.0 image, built with Raspberry Pi's `eepmake`: HAT+ format, 99 bytes, a vendor-info atom and the `tamanu-iti-v4` overlay name atom
- The ID bus as a bit-banged I²C bus on GPIO0/1, the EEPROM registered with the kernel's at24 driver: it answers at `0x50`, 32 KiB, and reads blank (all `0xff`)

A write counts as done when the EEPROM reads back identical to the image, not when the write call returns, so it holds however the chip reacts to a write while protected.

Run (28 Oct):
- Written first try once the pads were bridged, and read back identical after release
- Unbridged, the chip refuses the write and the kernel reports an I/O timeout, so "not bridged yet" shows as a failed write, not a silent no-op
- The firmware publishes the fields as text: `uuid` `9c2bcc02-123a-4d9d-80e0-72166bec3218`, `product_id` `0x0004`, `product_ver` `0x0000`, and the strings, each NUL-terminated
- With both the EEPROM overlay and the `config.txt` line, the two TPM nodes collide on CE1 ("chipselect 1 already in use") and the first to register wins; harmless, but it is why the line has to go
- With the `config.txt` line removed, the TPM comes up from the Iti overlay alone (`tcg,tpm_tis-spi`, manufacturer STM), and I²C3 comes up with the light sensor at `0x53` (part ID `0xb1`) and the BME680 at `0x77`
- The prototype keeps the line commented out (backup at `/boot/firmware/config.txt.f4-backup`). The overlay is also in `/etc/flash-kernel/dtbs/overlays/`, one of the paths `flash-kernel` copies overlays from when it stages a new boot slot on a kernel upgrade, so it survives slot switches
- Wi-Fi was down after the first reboot and the device was off Tailscale until cabled; most likely the reboot reverting the prototype's dev deploy of bliti, unrelated to the EEPROM

## Implementation options

- The Pi 5 has no firmware measured boot, so PCRs read from the provisioning boot won't match the NVMe boot, and the NVMe boot may have little measured at all. What a PCR policy can usefully bind to on the Pi is a tech-design question; until it's answered, the first-boot rebind may have nothing to add.
- Volume key rotation: the `pi` image rekeys itself on first boot. If provisioning rotates instead (it can, while the volume is offline and before enrolment), the first-boot rekey must recognise that and not run again; or provisioning leaves rotation to first boot and enrols keyslots that survive it (LUKS2 reencryption keeps keyslots). Tech design picks.
- `linux-images` already holds the pieces this reuses: the installer's dm-verity images partition, the recovery passphrase and TPM enrolment, and the save-recovery-keys option. Its `pi eeprom` SD image is retired, its bootloader settings now applied by provisioning.

## Trade-offs

- **Bootloader from the image build, over fetched live or pinned.** Fetching the latest at provisioning needs network on the bench and lets boards done a week apart differ. Pinning a tested version adds a test process without a known problem behind it. Taking the default channel's newest at image build is current as of each rebuild, works offline, and keeps a batch consistent.

- **Second page over a separate app or a section of the operator app.** Above the channel, provisioning shares almost nothing with the operator flow: no QR code, its own discovery, a multi-step pipeline, label rendering, and a recovery passphrase on screen. A field operator should never meet the passphrase screen, and an assembler has no use for Status or Network, so they are kept apart. A second entry point in one Vite build costs one config line; a separate app would mean a second build and deploy to keep in step with the protocol, which is the complexity worth avoiding.
- **Silent until pressed, over always advertising.** Web Bluetooth reaches a device only through the browser's chooser, filtered by name or prefix, so a page cannot listen to several boards and notice which saw a press. Advertising only after a press makes the press itself the selection: the chooser lists just the pressed boards, with nothing to match against and no name that changes (so no host name caching to worry about). Rejected: a "pressed" variant of an always-on name, which needed the name to change and risked hosts caching the old one; and confirming after connecting, which costs a chooser pass and a handshake per wrong guess.
- **Version as major.minor.** Revisions within a generation differ in hardware bliti may need to know about, and the user's naming (rev B as v4.0, rev C as v4.1) makes the revision visible where it matters.

## Open questions

- [x] How the firmware renders the fields: text, `product_id` as `0x0004`, `uuid` hyphenated
- [x] The Iti overlay lives in `linux-images` only. It will grow as other cards bind sensor drivers and buttons, and upstreaming is a slower loop; revisit once it settles
- [x] Scope: the whole provisioning medium is planned here; the work is split into cards later
- [x] Reaching a device without a QR code: well-known token, beacon suffix, button press to select
- [x] Sensor bus and button GPIOs: from the hardware handoff
- [x] U1: CAT24C256, 32 KiB
- [x] The "bridge here" picture: the user's rev B photo, labelled
- [x] The hardware check step: agreed
- [x] What tells rev B apart: the BME680 air sensor
- [x] Mapping from EEPROM to version: Iti product UUID, then `product_id`.`product_ver`
- [x] Iti product UUID: `9c2bcc02-123a-4d9d-80e0-72166bec3218`
- [x] Per-board records: shown in the client and saved to the provisioning medium; central register is a separate card
- [x] Strings and values: "BES International", "Tamanu Iti v4.0" (rev C "Tamanu Iti v4.1"), `product_id` 4, `product_ver` 0 for rev B, 1 for rev C
- [x] The label: 50 × 30 mm, QR plus suffix and version; A4 sticker-sheet PDF until a production printer is chosen
- [ ] Obtain ST33KTPM firmware images from ST (they are not public)
- [ ] Tailscale at provisioning: confirm signing in against the NVMe system's state from the provisioning system carries over to its first boot; the naming scheme for Itis; how the client holds the session's credential, and whether a browser can mint auth keys from a Tailscale OAuth client (CORS)
- [ ] How the device tells the NVMe already carries the medium's system, from the plaintext boot partition (a version the `pi` image records there, or another marker)
- [ ] `POWER_OFF_ON_HALT=1` with the X1208: the Pi's own button is not reachable in the case, and the X1208's button reaches Linux as the Pi's power key (a short press shut the prototype down cleanly), but after that shutdown presses did not start it again, only the X1208's hard cut (three presses) did. Test a halt and a single X1208 press with the setting at 1 and at 0, since provisioning writes it
- [x] The prototype's TPM: ST33KTPM firmware `0x00090101` (TPM spec revision 1.59); ST has published at least `0x00090200`, which needs the LMS-signed manifest path
- [x] Both LEDs (ACT green, PWR red) are controllable from Linux, on/off only
- [ ] Check bliti can drive the Pi 5's ACT and PWR LEDs from the provisioning system, and what both on together looks like; the table above assumes a distinct amber, and needs redoing with blink patterns if it isn't
- [ ] Whether a bootloader configuration written from Linux on the Pi 5 takes effect at the next boot without a further reboot, which the USB-first change at the start of a run relies on
- [ ] Physically blocking the SD slot in the case: a hardware design change, outside bliti
- [ ] Confirm the provisioning image is assembled in bliti from the published `pi` image (proposed), rather than built in `linux-images`
- [ ] The contract between bliti and the `linux-images` provisioning image: how bliti knows it is in provisioning mode, where it finds the NVMe image, where it writes records
