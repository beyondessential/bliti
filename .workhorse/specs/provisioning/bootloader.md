---
id: PBL
---

# Configuring the bootloader

The step `bootloader` of a [run](../overview.md#run) writes to a Tamanu Iti the bootloader configuration the Iti runs with, and brings the board's bootloader up to the one the [provisioning medium](../overview.md#provisioning-medium) carries.
It is the last step of a run, so that a run that fails before it can be run again from the medium.
[PRV](overview.md#the-boot-order) specifies the boot order a device sets when provisioning mode starts.

## Borrowed terms

| term | meaning |
| --- | --- |
| bootloader | The firmware, held in the Pi 5's own flash, that starts the board and chooses what it boots. |
| bootloader configuration | The settings the bootloader reads from the same flash, each of the form `NAME=value`. |
| boot order | The bootloader's list of the media it tries to boot from, in the order it tries them. |
| manifest | As [PMED](medium.md#the-manifest) has it. |

## The configuration

The step MUST leave the board's bootloader configuration holding the settings below, and no others.

| setting | value |
| --- | --- |
| `BOOT_ORDER` | `0xf61` |
| `POWER_OFF_ON_HALT` | `1` |
| `PSU_MAX_CURRENT` | `5000` |
| `PCIE_PROBE` | `1` |
| `BOOT_UART` | `1` |
| `HDMI_DELAY` | `0` |

> [!NOTE]
> `BOOT_ORDER` of `0xf61` is the SD card, then the NVMe disk, then the list again.
> It lists no USB stick and no network install, so a board that has passed the step boots its own disk with the provisioning medium still in, and an SD card is how it is provisioned again.
> `POWER_OFF_ON_HALT` has the board cut its own power when it halts, which the backup supply relies on.

## The bootloader

The date of a bootloader MUST be the date Raspberry Pi gives it, in the form of the manifest's `BOOTLOADER_VERSION`.

The bootloader the medium carries is the file `pieeprom.bin` of [PMED](medium.md#what-the-medium-carries), and its date is the manifest's `BOOTLOADER_VERSION`.

The step MUST bring the board's bootloader to the one the medium carries where the board's is dated earlier.

The step MUST NOT replace a bootloader with one dated earlier, and MUST NOT replace one with the same date.

## Done-state

The step MUST determine its done-state from the bootloader and the configuration as the board holds them in its flash, and MUST NOT determine it from what the firmware reported when the board booted.

The step MUST report:

| done-state | where |
| --- | --- |
| `done` | the board's bootloader is not dated earlier than the medium's, and its configuration holds exactly the settings of [The configuration](#the-configuration) |
| `not-done` | the board's bootloader is dated earlier than the medium's, or its configuration differs |
| `unknown` | the medium carries no bootloader or no date for it, or the board's bootloader or configuration cannot be read |

The step SHOULD give as its reason what differs, or why the done-state is `unknown`.

> [!NOTE]
> A board that a run has put USB first on reads `not-done`, as its boot order differs.
> The firmware reads the flash as the board boots, so what it reports describes the flash as it was then.

## Carrying out the step

The step MUST fail, naming what is missing, where the medium carries no bootloader or no date for it.

The step MUST fail where it cannot read the board's bootloader or configuration.

Where the board's bootloader is not dated earlier than the medium's and its configuration holds exactly the settings of [The configuration](#the-configuration), the step MUST pass without writing.

Otherwise the step MUST write the bootloader only where [The bootloader](#the-bootloader) requires it, and MUST write the configuration.

The step MUST NOT ask an operator for anything.

The step MUST pass only once the bootloader and the configuration the board holds in its flash are those [Done-state](#done-state) reports as `done`, whatever its writes reported, and MUST otherwise fail, saying that the board's bootloader could not be set.

> [!NOTE]
> The bootloader reads its flash as the board boots, so what the step wrote is in effect from the next boot, including one after the board is powered off straight after the run finishes.
