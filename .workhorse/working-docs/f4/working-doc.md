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
| `uuid` | 16 bytes, "product UUID (unique)", required; unique per product and preserved across flashes (the tools say to keep the one generated) | one well-known UUID for the Tamanu Iti product line | yes: recognises a Tamanu Iti |
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

C4's `product` fact takes "Tamanu Iti v" plus the generation where the HAT EEPROM identifies a Tamanu Iti.
This card fills in C4's placeholder (`facts::product::generation`) with the real match.
The placeholder takes `vendor` and `product`; the real match reads `uuid`, `product_id` and `product_ver` instead, so its inputs change too.

### Provisioning

A provisioning mode guides the assembler through programming a board's EEPROM.
The EEPROM is write-protected unless two test pads are bridged (the HAT design guide's WP-to-test-point pattern).
The flow:

1. Detect the EEPROM, and whether it is already programmed
2. Tell the assembler to bridge the pads, with a picture of the board showing where: the rev B photo with the ID WP pads circled and labelled (`.workhorse/design/assets/iti-hat-rev-b-wp-pads.jpg`), shipped with the provisioning client
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

Front ends: both a client screen (over the channel) and a device-local command, driving one device-side act.

### The provisioning SD card

Provisioning happens from an SD card carrying an OS, bliti in provisioning mode, and a copy of the image for the Iti's NVMe.
Booted on a fresh Iti, it programs the EEPROM, flashes the NVMe, binds the LUKS partition to the TPM, and produces the QR code as a printable.
It needs no QR code to start.

Each step is offered separately and can be skipped, so a board that is already imaged (the v4 prototype, say) can have only its EEPROM programmed.

#### Order of steps

1. Check the hardware (first, so a bad board fails fast, and because the air sensor read places the board's version for the EEPROM step)
2. Program the EEPROM
3. Configure the bootloader
4. Flash the NVMe and read it back
5. Rotate the volume key, enrol the TPM keyslot, and replace the placeholder passphrase with a recovery passphrase
6. Render the label and save the board's record

The assembler chooses which steps to run; the rest are skipped.
A step that fails stops the run and says why, and the assembler can run it again without repeating the steps before it.
The label and record are offered at the end of any run whose chosen steps all passed, since the QR code comes from the TPM board ID whichever steps ran.
At the end the client tells the assembler to power the board off and remove the SD card.

#### Flashing the NVMe

The image is the `linux-images` `pi` variant.
Its LUKS2 root ships with an empty placeholder passphrase and the image's own volume key; the volume key is rotated per board (`linux-images` already supports this: the `pi` image rekeys itself on first boot, `image.identity.luks-rekey`).
After writing, provisioning reads the NVMe back and checks it against the image.
The image copy on the SD card is ideally carried in a dm-verity partition, as the `linux-images` installer medium does; a plain checksum otherwise.

#### Binding to the TPM

Hybrid: provisioning enrols a TPM-only keyslot (no PCR policy), which works from the SD boot and ties the disk to this board, and replaces the empty placeholder passphrase.
A first-boot enrolment on the NVMe system then rebinds with PCRs, if the Pi can measure anything worth binding to.
A recovery passphrase is generated at provisioning. It is never included alongside the QR code, which would display on the outside of the device.

What the TPM-only keyslot protects: the disk away from its board.
It does not protect a whole stolen device. The bootloader tries SD before NVMe (`BOOT_ORDER=0xf61` in `linux-images`), so anyone can boot their own SD on the board and the TPM will unseal for them; with no measured boot on the Pi, a PCR policy cannot tell their OS from ours.
NVMe-first would not close this (boot with the NVMe out, then attach it over USB) and would stop the provisioning SD taking over an imaged board, so the order stays SD first.
Closing it takes Raspberry Pi secure boot (signed boot images, a customer key burnt into OTP, irreversible per board). That is a separate card, though its OTP programming would naturally be another provisioning step.

#### Configuring the bootloader

A step that sets the Pi 5 bootloader up for the Iti (boot order to NVMe, firmware version), reusing what the `linux-images` `pi eeprom` SD does.

#### The QR label

The client renders a label file (PDF/PNG) sized for a label printer, carrying the QR code and its four-character suffix, for the assembler to print.

#### Board records

At the end, each board's record (QR suffix, version, recovery passphrase) is shown to the assembler in the client and appended to a file on the provisioning SD card.
Sending it to a central register is a separate card.

#### Where it's built

`linux-images` builds the provisioning SD image, beside the `pi eeprom` SD, with bliti packaged in provisioning mode.
This card specs bliti's side and the contract between the two repos; the image build is `linux-images` work.

### Reaching a provisioning device

A device in provisioning mode uses a well-known, published presence token instead of one derived from its board ID, so any client can open a channel to it without a QR code.
Trust comes from being physically at the assembly bench. The client cannot authenticate the device, since there is no key fingerprint to check against.

Its advertised name carries a provisioning marker plus the last four characters of the QR payload the board will have once provisioned.
That suffix comes from the TPM board ID through the key fingerprint, is safe to broadcast, and is the same four characters STAT shows as a device's fallback name and that appear on the printed label.
Several boards can be in provisioning at once. The client finds them by filtering on the well-known marker and lists them by suffix.

To tell which physical board is which, the assembler presses one of the HAT's buttons (two are wired to GPIOs) on the board they want.
For a short while after a press, the device advertises a "pressed" variant of its provisioning name, and the client opens the chooser filtered to that variant, so only the pressed board is listed.

### The provisioning client

The provisioning client is a second page in the existing web build, at its own path, not linked from the operator app.
It shares the channel, protocol and wasm code with the operator app.

### Reporting the version

A HAT-identified board reports as "Tamanu Iti v" followed by its major.minor version: "Tamanu Iti v4.0" for rev B.
The third generation carries no EEPROM to tell revisions apart, and stays "Tamanu Iti v3".
This changes C4's Product section in NFO ("Tamanu Iti v" followed by the generation), which this card amends once C4 merges.

### The HAT, rev B (silkscreened "ITI 4.0")

| part | what | notes |
| --- | --- | --- |
| U1 | the HAT EEPROM (SOIC-8, presumably) | beside R1/R2, marked 392 (3.9 kΩ, the HAT spec's ID-pin pull-ups) and the ID WP pads |
| ID WP | two round pads, centre right | bridged to make the EEPROM writable |
| U2 | TPM, ST33K part on SPI | the BID board-ID source |
| U3 | AIR: Bosch BME680 | rev C is expected to carry a cheaper air sensor instead (possibly a BMP580) |
| U4 | LIGHT: ambient light sensor ("ALS 308") | |
| USER | tactile button | the button the assembler presses to select a board in provisioning |
| SW1 | CASE: lever switch, held pressed while the case is closed | case-open detection |
| J3 | 2×10 header | |

The sensors are not on I²C bus 1 (where the backup supply's gauge sits).
Which bus, and which GPIOs the buttons use, are to come from the user.

### Inferring the version, and where it stops

Provisioning tells rev B from later revisions by which air sensor answers: a BME680 means v4.0.
This kind of detection by positive markers is what C4 avoided, because a dead or missing part misreads.
Here it is used only once, at provisioning, to decide what to write; from then on the EEPROM is authoritative and the running system never infers from parts.
Where provisioning can't place the board as a known version (the sensor that tells revisions apart doesn't answer, or a combination it doesn't know), the EEPROM step stops and says the board needs looking at, without guessing.

### Checking the hardware

Before the label is printed, provisioning exercises the HAT so a bad board is caught on the bench and not in the field.
The assembler presses USER, then presses and releases the CASE lever; the device reads the air and light sensors once, and confirms the TPM answers.
Each check passes or fails in the client.
Doing anything ongoing with these devices (case-open alerts, environment readings) belongs to other cards.

## Implementation options

- The Pi 5 has no firmware measured boot, so PCRs read from the SD boot won't match the NVMe boot, and the NVMe boot may have little measured at all. What a PCR policy can usefully bind to on the Pi is a tech-design question; until it's answered, the first-boot rebind may have nothing to add.
- Volume key rotation: the `pi` image rekeys itself on first boot. If provisioning rotates instead (it can, while the volume is offline and before enrolment), the first-boot rekey must recognise that and not run again; or provisioning leaves rotation to first boot and enrols keyslots that survive it (LUKS2 reencryption keeps keyslots). Tech design picks.
- `linux-images` already holds the pieces this reuses: the installer's dm-verity images partition, the recovery passphrase and TPM enrolment, the save-recovery-keys option, and the `pi eeprom` SD image that configures the Pi 5 bootloader.

## Trade-offs

- **Second page over a separate app or a section of the operator app.** Above the channel, provisioning shares almost nothing with the operator flow: no QR code, its own discovery, a multi-step pipeline, label rendering, and a recovery passphrase on screen. A field operator should never meet the passphrase screen, and an assembler has no use for Status or Network, so they are kept apart. A second entry point in one Vite build costs one config line; a separate app would mean a second build and deploy to keep in step with the protocol, which is the complexity worth avoiding.
- **"Pressed" name variant over confirming after connecting.** Web Bluetooth reaches a device only through the browser's chooser, filtered by name or prefix, so a page cannot listen to several boards and notice which saw a press. Confirming after connecting means a chooser pass and a handshake per guess, which scales badly with several boards on a bench. The variant gets the right board in one pass. Its risk is a host caching a device's name (ADV's note on the GAP Device Name), so selection should happen before a board's first connection; tech design checks how browsers behave.
- **Version as major.minor.** Revisions within a generation differ in hardware bliti may need to know about, and the user's naming (rev B as v4.0, rev C as v4.1) makes the revision visible where it matters.

## Open questions

- [ ] Exactly how the firmware renders `uuid`, `product_id` and `product_ver` under `/proc/device-tree/hat/` (expected as text, e.g. `0x0004`); confirm by reading back the prototype once programmed
- [ ] The Iti overlay: its name, what it configures, and whether it lives in `linux-images` or upstream (HAT+ asks vendors to upstream overlays)
- [x] Scope: the whole provisioning SD card is planned here; the work is split into cards later
- [x] Reaching a device without a QR code: well-known token, beacon suffix, button press to select
- [ ] Which bus the sensors are on, and which GPIOs USER and CASE use (user is gathering)
- [ ] U1's exact part, for its size and write page size
- [x] The "bridge here" picture: the user's rev B photo, labelled
- [x] The hardware check step: agreed
- [x] What tells rev B apart: the BME680 air sensor
- [x] Mapping from EEPROM to version: Iti product UUID, then `product_id`.`product_ver`
- [ ] Generate the Iti product UUID (once, recorded in the spec)
- [x] Per-board records: shown in the client and saved to the SD; central register is a separate card
- [ ] Vendor and product strings, and the `product_id` values
