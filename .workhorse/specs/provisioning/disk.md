---
id: PDSK
---

# Writing the system to the disk

The step `disk` of a [run](../overview.md#run) writes the system on the [provisioning medium](../overview.md#provisioning-medium) to a Tamanu Iti's NVMe disk, and the step `overlay` installs the HAT overlay into it.
[PENC](encryption.md) specifies the step that then encrypts the disk.

## Borrowed terms

| term | meaning |
| --- | --- |
| payload | The `pi` image on the provisioning medium, as [PMED](medium.md#the-payload) has it. |
| manifest | As [PMED](medium.md#the-manifest) has it. |
| boot slot | As [PMED](medium.md) has it. |
| HAT overlay | As [PMED](medium.md) has it. |
| firmware partition | The first partition of the `pi` image, a FAT32 partition labelled `firmware` that holds the boot slots. |
| root partition | The third partition of the `pi` image, which holds the encrypted root volume. |
| root filesystem | The filesystem a system mounts as `/`, inside the root volume. |

## The disk

The disk MUST be the board's NVMe drive.

Both steps MUST fail, saying so, where the board has no NVMe drive or has more than one.

The step `disk` MUST fail, saying so, where the drive is smaller than the payload's size in the manifest.

## What the disk holds

A system the step `disk` has written MUST carry, at the root of its firmware partition, a file `iti-image.txt` that holds the manifest's `PI_IMAGE_VERSION` and `PI_IMAGE_SHA256` lines, in that order, as the manifest has them.

A device MUST describe what the disk holds as the first of these that applies.

| holds | where |
| --- | --- |
| the medium's system | its firmware partition carries `iti-image.txt`, whose `PI_IMAGE_SHA256` is the manifest's |
| an earlier system | its firmware partition carries `iti-image.txt`, described by its `PI_IMAGE_VERSION`, or its partitions are labelled `firmware`, `xboot` and `root` as the `pi` image labels them |
| blank | it holds no partition table and no filesystem signature |
| another system | anything else |

> [!NOTE]
> The file marks a system the step wrote and verified.
> It lets a medium recognise a disk by what was written to it, which later changes to the system, such as kernel updates and the first boot's rotation of identifiers, do not alter.
> A disk the step wrote in part, before a power cut, carries the image's partition labels and no file, and is an earlier system, so the step starts again without asking.

## Done-state of the step `disk`

The step MUST determine its done-state by reading the disk, and MUST NOT change the disk to do so.

The step MUST report:

| done-state | where |
| --- | --- |
| `done` | the disk holds the medium's system |
| `not-done` | the disk holds an earlier system, another system, or is blank |
| `unknown` | the disk cannot be found, or cannot be read |

The step SHOULD give as its reason what the disk holds, or why the done-state is `unknown`.

## Writing the system

Where the disk holds another system, the step MUST open a `confirm` prompt with the id `overwrite`, whose text says that the disk holds a system other than the Iti's and that writing erases it, and MUST write only where the operator confirms.

Where the operator declines, the step MUST fail, saying that the disk was left as it was.

Where the disk holds anything else, including the medium's system, the step MUST write without asking.

The step MUST remove `iti-image.txt` from the disk, where the disk carries one, before it writes anything else.

The step MUST read the payload through dm-verity against the manifest's `PAYLOAD_ROOT_HASH`, as [PMED](medium.md#the-payload) requires, and MUST write its first `PI_IMAGE_SIZE` bytes, byte for byte, from the disk's first byte.

The step MUST fail where a read of the payload fails.

The step MUST flush the written bytes to the disk, and then read them back from the disk itself, not from memory.

The step MUST fail, saying that what was read back differs, where the SHA-256 digest of the disk's first `PI_IMAGE_SIZE` bytes is not the manifest's `PI_IMAGE_SHA256`.

Only after the read-back matches, the step MUST write `iti-image.txt` to the disk's firmware partition, and then pass.

The step MUST report its progress as a fraction over the write and the read-back together.

> [!NOTE]
> A run interrupted anywhere in the step starts the step again from its beginning, which writes the whole payload again.
> A disk written in part carries no `iti-image.txt`, so no medium takes it for the medium's system.

## Done-state of the step `overlay`

The step MUST determine its done-state by reading the disk, and MUST NOT change the disk to do so.

The step MUST open the disk's root volume to read its root filesystem, as [Opening the root volume](#opening-the-root-volume) has it, and MUST close it again.

The step MUST report:

| done-state | where |
| --- | --- |
| `done` | the HAT overlay is in both places [Installing the overlay](#installing-the-overlay) names, byte for byte as the medium carries it |
| `not-done` | the disk holds a system, and the HAT overlay is missing from either place or differs |
| `unknown` | the disk cannot be found or read, it holds no system to install into, or its root volume cannot be opened |

The step SHOULD give as its reason what is missing, or why the done-state is `unknown`.

## Installing the overlay

The step MUST install the HAT overlay that the medium carries as `tamanu-iti-v4.dtbo`, as [PMED](medium.md#what-the-medium-carries) has it, as a file of that name in both of:

| place | on |
| --- | --- |
| the `overlays` directory of the current boot slot | the disk's firmware partition |
| the directory `/etc/flash-kernel/dtbs/overlays/` | the disk's root filesystem |

> [!NOTE]
> The firmware reads the overlay from the current boot slot.
> The system's kernel updates take the overlays of each boot slot they stage from the directory on the root filesystem, so an overlay only in the slot would be lost at the first update.

The step MUST fail, saying so, where the disk holds no firmware partition, no current boot slot, or no root filesystem to install into.

A file at the overlay's name MUST NOT hold anything other than the whole overlay at any moment, including through a power cut.

The step MUST flush each file to the disk, and MUST pass only once both read back as the overlay the medium carries.

Where both places already hold the overlay, the step MUST pass without writing.

## Opening the root volume

The step MUST open the disk's root volume by whichever of the empty passphrase of [PENC](encryption.md#borrowed-terms) and the board's TPM unlocks it.

The step MUST fail, saying that the root volume could not be opened, where neither does.

The step MUST mount the root filesystem read-only to determine its done-state.
