---
id: PMED
---

# Provisioning medium

The [provisioning medium](../overview.md#provisioning-medium) is a disk image, written whole to a USB stick or an SD card, that boots a Tamanu Iti into [provisioning mode](overview.md) and carries everything a [run](../overview.md#run) writes to the board.
The same image serves as a USB stick at the factory and as an SD card for a repair.

## Borrowed terms

| term | meaning |
| --- | --- |
| `pi` image | The disk image for the Raspberry Pi 5 that `linux-images` publishes with each of its releases. Its root partition is LUKS2 with an empty passphrase. |
| boot slot | One of the copies of the kernel, initramfs, device tree and overlays on the `pi` image's firmware partition. The firmware boots the current one. |
| dm-verity | The Linux device-mapper target that checks each block read from a device against a hash tree, and fails the read of any block that does not match. |
| HAT overlay | The device tree overlay `tamanu-iti-v4` of [HAT](../hat.md). |
| default channel | The release channel Raspberry Pi names "default" for the Pi 5's bootloader. |

## The medium's system

The medium's system MUST be the `pi` image it is built from, with the additions this spec makes.

The medium's system MUST run bliti in provisioning mode from boot, with the data partition as the medium and the TPM Endorsement Key of [BID](../board-id.md) as the kind of board ID source.

The medium's system MUST NOT install or run bliti's ordinary daemon.

> [!NOTE]
> The ordinary daemon would hold the lines of the board's switches, advertise as an ordinary device, and keep its own state on the medium.

The medium's system MUST open the console command of [PCON](console.md) at boot on its text console and on its serial console, without asking for a login.

> [!NOTE]
> Whoever is at the console is at the bench, which is the same trust the provisioning token rests on.
> A client over Bluetooth is the usual front end, and the console serves a bench without one.

### Firmware configuration

The medium's firmware configuration MUST load the HAT overlay, whatever the board's HAT EEPROM holds.

The medium's firmware configuration MUST NOT load an overlay a HAT EEPROM names.

The medium's firmware configuration MUST NOT load any overlay, other than the HAT overlay, that declares a device on the first SPI bus's second chip select.

> [!NOTE]
> A fresh board's EEPROM names no overlay, and a programmed one names the HAT overlay.
> Loading the overlay from the configuration alone gives both the same devices, and a second overlay declaring the TPM would collide with it.

The medium's system MUST carry the HAT overlay among the overlays of its current boot slot, and among the overlays its kernel updates copy into a new boot slot.

The medium's system MUST make the HAT EEPROM's bus, on GPIO0 and GPIO1, available to software.

## Layout

The medium MUST hold, in order, the partitions of the `pi` image as that image lays them out, then the payload partition, then the data partition.

The medium's system MUST find the payload and data partitions on the disk it booted from, and MUST NOT use a partition of any other disk.

Where the disk it booted from lacks the payload or data partition, or the data partition cannot be mounted, the medium's system MUST report it as [DEV](../device.md) requires, and MUST NOT start bliti in provisioning mode.

> [!NOTE]
> A board can hold a second medium, or an NVMe written from one, and neither is the medium the board booted from.

## The payload

The payload partition MUST hold the `pi` image the medium's system is built from, byte for byte as published, from the partition's first byte.

The payload partition MUST hold a dm-verity hash tree of the payload after it, beginning with its superblock.

A step that reads the payload MUST read it through dm-verity, against the root hash in the manifest of the medium's system, and MUST fail where any read fails.

> [!NOTE]
> A stick that has worn, or was written badly, fails the step that reads it, rather than writing a corrupt system to the board's disk.

## What the medium carries

The medium's system MUST carry, in `/usr/share/bliti/provision/`:

| file | is |
| --- | --- |
| `pieeprom.bin` | the newest bootloader on the default channel for the Pi 5 when the medium was built |
| `tamanu-iti-v4.dtbo` | the HAT overlay |
| `tamanu-iti-v<version>.eep` | the HAT EEPROM image for each version, as `tamanu-iti-v4.0.eep` is, whose contents [HAT](../hat.md) defines and bliti holds |
| `manifest` | the manifest |

## The manifest

The manifest records what a medium carries.
It is text, one `NAME=value` line per entry, in this order.

| name | value |
| --- | --- |
| `BLITI_COMMIT` | the full hash of the bliti commit the medium was built from |
| `BUILT` | when the medium was built, in UTC, in the form `2026-10-29T01:49:00Z` |
| `PI_IMAGE_VERSION` | the version of the `linux-images` release the `pi` image comes from |
| `PI_IMAGE_SIZE` | the payload's size in bytes |
| `PI_IMAGE_SHA256` | the SHA-256 digest of the payload, in lowercase hexadecimal |
| `PAYLOAD_HASH_OFFSET` | the offset in bytes, from the start of the payload partition, of the hash tree's superblock |
| `PAYLOAD_ROOT_HASH` | the root hash of the payload's hash tree, in lowercase hexadecimal |
| `BOOTLOADER_VERSION` | the date Raspberry Pi gives the bootloader the medium carries, in the form `2026-10-29` |

## The data partition

The data partition MUST be FAT32, labelled `ITIPROV`.

The medium's system MUST mount the data partition read-write, and MUST give its mount point to bliti as the medium.

The data partition MUST hold a copy of the manifest at `manifest.txt`, at its root.

> [!NOTE]
> The data partition holds what runs keep, and can be read on any computer.
> The copy of the manifest there tells someone holding a stick what is on it, and the medium's system reads its own.

## Building

The medium MUST be built from the newest release of `linux-images` that publishes a `pi` image.

The build MUST check the `pi` image against the checksum its release publishes, and MUST fail where it does not match.

The build MUST take bliti, the HAT overlay and the HAT EEPROM images from the one commit it builds.

The build MUST take the newest bootloader on the default channel at the time it runs.

The build MUST produce the medium as one raw disk image compressed with zstd, the SHA-256 digest of that file, and the manifest.

## Publishing

The medium MUST be built and published on each merge to bliti's main branch, and once a week.

The production origin of [WEB](../web-app.md) MUST serve the newest medium at these paths, each build replacing the last.

| path | is |
| --- | --- |
| `/downloads/tamanu-iti-provision.img.zst` | the medium |
| `/downloads/tamanu-iti-provision.img.zst.sha256` | its SHA-256 digest, in the form `sha256sum` reads |
| `/downloads/tamanu-iti-provision.txt` | its manifest |

A pull request's preview origin MUST serve a medium built from the pull request at the same paths where the pull request's description has its "Build the provisioning image" box ticked, and MUST NOT serve one otherwise.

> [!NOTE]
> The weekly build picks up a new `pi` image or bootloader with no change to bliti.
> A medium is several gigabytes, so a pull request builds one only when asked to.
