---
status: draft
---

# Iti v4 HAT EEPROM

What a v4 board's HAT EEPROM holds, how bliti programs and verifies it during provisioning, and how bliti reads it back to identify a Tamanu Iti and its generation.

## Behaviour

### What the EEPROM holds

The EEPROM uses the Raspberry Pi HAT+ format: a header, a vendor-info atom, and optional further atoms.
The vendor-info atom's fields are all standard:

| field | holds | interpreted by bliti |
| --- | --- | --- |
| `uuid` | a fresh RFC 4122 UUID generated per board at programming time: the board's serial | no (see open questions on accounting) |
| `vendor` | a readable vendor string | no |
| `product` | a readable product string, e.g. "Tamanu Iti v4" | no |
| `product_id` | one value per generation | no; readable only |
| `product_ver` | the board revision within a generation | no; readable only |

What bliti matches on to recognise a Tamanu Iti, and to read its generation, is a well-known UUID per generation.
The HAT spec reserves `uuid` for a per-board unique value, so the well-known one can't live there; the candidate is a `custom_data` atom.

The readable fields are for anyone curious reading the board, and carry no meaning bliti depends on.

### How C4 consumes it

C4's `product` fact takes "Tamanu Iti v" plus the generation where the HAT EEPROM identifies a Tamanu Iti.
This card fills in C4's placeholder (`facts::product::generation`) with the real match.

### Provisioning

A provisioning mode guides the assembler through programming a board's EEPROM.
The EEPROM is write-protected unless two test pads are bridged (the HAT design guide's WP-to-test-point pattern).
The flow:

1. Detect the EEPROM, and whether it is already programmed
2. Tell the assembler to bridge the pads, with a picture of the board showing where
3. Repeatedly try to write until a write succeeds
4. Tell the assembler they can let go
5. Read back and verify

The v4 prototype goes through the same flow.

Which board version to write is inferred from the hardware, not configured or asked.
Each board version is expected to be detectable from the hardware present via GPIO and peripherals: v4 rev B (this HAT) carries specific devices checkable over I²C, and v4 rev C will differ.
Board versions are reported as major.minor: rev B is v4.0, rev C v4.1.

Where the EEPROM already identifies a Tamanu Iti of the version being provisioned, the step compares the rest of its contents against what it would write, since there may be updates for it.
It skips if they match and rewrites if not, keeping the board's serial UUID.

Where the EEPROM holds something else (another vendor's data, or another Iti version), the step shows what it holds and asks the assembler to confirm before overwriting.

The write retries for up to five minutes waiting for the pads to be bridged, then gives up, tells the assembler to check the bridge, and offers to try again.

Front ends: both a client screen (over the channel) and a device-local command, driving one device-side act.

### The provisioning SD card

Provisioning happens from an SD card carrying an OS, bliti in provisioning mode, and a copy of the image for the Iti's NVMe.
Booted on a fresh Iti, it programs the EEPROM, flashes the NVMe, binds the LUKS partition to the TPM, and produces the QR code as a printable.
It needs no QR code to start.

Each step is offered separately and can be skipped, so a board that is already imaged (the v4 prototype, say) can have only its EEPROM programmed.

#### Flashing the NVMe

The image is the `linux-images` `pi` variant.
Its LUKS2 root ships with an empty placeholder passphrase and the image's own volume key; the volume key is rotated per board (`linux-images` already supports this: the `pi` image rekeys itself on first boot, `image.identity.luks-rekey`).
After writing, provisioning reads the NVMe back and checks it against the image.
The image copy on the SD card is ideally carried in a dm-verity partition, as the `linux-images` installer medium does; a plain checksum otherwise.

#### Binding to the TPM

Hybrid: provisioning enrols a TPM-only keyslot (no PCR policy), which works from the SD boot and ties the disk to this board, and replaces the empty placeholder passphrase.
A first-boot enrolment on the NVMe system then rebinds with PCRs, if the Pi can measure anything worth binding to.
A recovery passphrase is generated at provisioning. It is never printed on the outside of the device.

#### Configuring the bootloader

A step that sets the Pi 5 bootloader up for the Iti (boot order to NVMe, firmware version), reusing what the `linux-images` `pi eeprom` SD does.

#### The QR label

The client renders a label file (PDF/PNG) sized for a label printer, carrying the QR code and its four-character suffix, for the assembler to print.

#### Board records

At the end, each board's record (EEPROM serial UUID, QR suffix, recovery passphrase) is shown to the assembler in the client and appended to a file on the provisioning SD card.
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

To tell which physical board is which, the assembler presses one of the HAT's buttons (two are wired to GPIOs) on the board they want, and the client picks out the device that saw the press.

## Implementation options

- The Pi 5 has no firmware measured boot, so PCRs read from the SD boot won't match the NVMe boot, and the NVMe boot may have little measured at all. What a PCR policy can usefully bind to on the Pi is a tech-design question; until it's answered, the first-boot rebind may have nothing to add.
- `linux-images` already holds the pieces this reuses: the installer's dm-verity images partition, the recovery passphrase and TPM enrolment, the save-recovery-keys option, and the `pi eeprom` SD image that configures the Pi 5 bootloader.

## Open questions

- [ ] Does the firmware publish `custom_data` atoms under `/proc/device-tree/hat/`? If not, matching on a custom-atom UUID means reading the EEPROM over I²C, which C4 avoids in the facts poll
- [x] Scope: the whole provisioning SD card is planned here; the work is split into cards later
- [x] Reaching a device without a QR code: well-known token, beacon suffix, button press to select
- [ ] The two buttons: which GPIOs, and what else they are for
- [x] Per-board records: shown in the client and saved to the SD; central register is a separate card
- [ ] Vendor and product strings, and the `product_id` values
- [ ] Investigate the HAT's other hardware: its I²C bus and devices
