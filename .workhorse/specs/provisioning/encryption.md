---
id: PENC
---

# Encrypting the disk

The step `encryption` of a [run](../overview.md#run) takes the root volume of the system that [PDSK](disk.md) wrote to a Tamanu Iti's disk from the build of the image to the board's own.
It gives the volume a key of its own, sets the volume to unlock from the board's TPM, and replaces the passphrase the image ships with by a recovery passphrase, which it saves in the board's record.

## Borrowed terms

| term | meaning |
| --- | --- |
| LUKS2 | The Linux disk encryption format of the root volume. A volume holds one volume key, which encrypts its data, and keyslots, each of which holds that key wrapped under one way of unlocking it. |
| volume key | The key that encrypts the volume's data. |
| keyslot | As LUKS2 has it. |
| root volume | The LUKS2 volume on the root partition of the disk, as [PDSK](disk.md) has it. |
| empty passphrase | The empty string, which unlocks the root volume of the `pi` image as the image ships. |
| TPM keyslot | A keyslot whose key only the board's TPM releases, so the volume unlocks on the board without anyone present. |
| recovery passphrase | A passphrase an operator types to unlock the volume where the TPM does not. |
| TPM | A Trusted Platform Module, as [BID](../board-id.md) has it. |
| PCR | A register of a TPM that holds a digest of what was measured into it. |

## The volume

The step takes the root volume as the `pi` image ships it: a LUKS2 volume with one keyslot, which the empty passphrase unlocks, whose volume key and UUID every copy of the image shares.

> [!NOTE]
> The volume key is wrapped, not secret: anyone holding the image can unwrap it with the empty passphrase.
> A disk keeps its data from whoever holds it only once its volume key is one that no copy of the image shares.

## What the step leaves

When the step has passed, the root volume MUST:

- hold a volume key other than the payload's root volume holds,
- have a UUID other than the payload's root volume has,
- hold exactly two keyslots, a TPM keyslot and a keyslot the recovery passphrase unlocks,
- not be unlocked by the empty passphrase.

The TPM keyslot MUST NOT carry a policy over PCRs.

> [!NOTE]
> The Pi's firmware measures nothing into the TPM, so a policy over PCRs binds the key to values that a system of anyone's would produce as well as the Iti's.
> The keyslot ties the disk to its board.
> It does not stop someone who can boot a system of their own on the board from having the TPM release the key.

The system on the disk MUST unlock its root volume at boot from the TPM keyslot, with nobody present.

The boot slot the step leaves current MUST carry every overlay that the boot slot current before the step carried.

The step MUST leave no boot slot that the firmware can boot, current or as a rollback, whose initramfs unlocks the root volume only by the empty passphrase.

> [!NOTE]
> A boot slot that unlocks only by the empty passphrase leaves a disk it cannot open once the passphrase is gone.
> The first boot's rotation of identifiers leaves a volume alone whose key and UUID already differ from the image's, so the step rotates both, and the UUID the record carries stays the volume's.

## The recovery passphrase

The step MUST generate a recovery passphrase of six words, each drawn at random from the miniLock diceware word list by a cryptographically secure generator, written in lowercase and joined by hyphens.

The step MUST draw only words of at least five characters, which have only ASCII letters and digits.

The step MUST NOT keep the passphrase anywhere but the keyslot it unlocks, the records file of [The record](#the-record) and the step's result, and MUST NOT put it in a log.

## The record

The step MUST save the board's record by appending a line to the file `records.tsv` at the root of the [data partition](medium.md#the-data-partition) of the provisioning medium, and MUST create the file where it does not exist.

The line MUST hold these fields, in this order, separated by tab characters and ended by a line feed.

| field | holds |
| --- | --- |
| suffix | the suffix of the board's name, as [PRV](overview.md#the-advertised-name) has it |
| version | the board's version as [PHW](hardware.md#placing-the-board) places it, or nothing where it cannot place it |
| disk | the UUID of the root volume, in lowercase |
| passphrase | the recovery passphrase |
| at | when the line was saved, as an RFC 3339 date and time in UTC |

The file MUST NOT carry a header line, and the step MUST NOT change any line already in it.

Where the file holds more than one line for a board, the last is the board's record.

The step MUST flush the line to the medium before it goes on.

The step MUST report the record it saved as the member `record` of its result, as [PRS](stream.md#the-record) has it, with every member it has a value for.

> [!NOTE]
> The file is on a FAT32 partition, so any computer can read it.
> Each medium keeps its own, so the records of the boards a medium provisioned are on that medium.

## Done-state

The step MUST determine its done-state by reading the disk and asking the TPM, and MUST NOT change the disk to do so.

The step MUST report:

| done-state | where |
| --- | --- |
| `done` | the root volume holds a volume key other than the payload's, the board's TPM unlocks it, and the empty passphrase does not |
| `not-done` | the disk holds no LUKS2 root volume, or holds one for which any of those does not hold |
| `unknown` | the disk cannot be found or read, or the TPM does not answer |

The step SHOULD give as its reason what is missing, or why the done-state is `unknown`.

## Carrying out the step

The step MUST fail, saying so, where the TPM does not answer, where the board's suffix is `FAULT`, or where the disk holds no LUKS2 root volume.

The step MUST fail, saying that the root volume could not be opened, where neither the empty passphrase nor the board's TPM unlocks it.

Where the root volume is done, as [Done-state](#done-state) has it, the step MUST pass without changing anything.

Where the empty passphrase unlocks the root volume, the step MUST do what follows, whatever else the volume holds.

1. Where the volume holds the payload's volume key, or a re-encryption under a new key is in progress, the step MUST re-encrypt the volume under a new volume key, resuming a re-encryption in progress, and MUST complete that before it changes any keyslot.
2. Where the volume's UUID is the payload's, the step MUST give it a new UUID, drawn at random.
3. The step MUST remove every keyslot the empty passphrase does not unlock, then add the keyslot for a recovery passphrase it generates and the TPM keyslot.
4. The step MUST make the system on the disk unlock from the TPM keyslot at boot, as [What the step leaves](#what-the-step-leaves) has it, with the empty passphrase's keyslot still in place.
5. The step MUST check that the recovery passphrase and the TPM each unlock the volume.
6. The step MUST save the record, as [The record](#the-record) has it.
7. The step MUST remove the empty passphrase's keyslot, and MUST pass only once the TPM unlocks the volume and the empty passphrase does not.

The step MUST fail, leaving the empty passphrase's keyslot in place, where it cannot complete steps 1 to 6.

> [!NOTE]
> The order keeps a disk that can be opened through every interruption: while the empty passphrase's keyslot is in place the volume opens with it, and once it is removed the TPM keyslot opens the volume and the passphrase is already in the record.
> An interrupted re-encryption resumes on the next carrying out, and an interruption after step 3 leaves keyslots that step 3 removes the next time.
> A restart after step 6 saves a second line for the board, and the last is its record.
