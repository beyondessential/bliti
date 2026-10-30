# Write the disk and set up encryption

Scenarios for the steps `disk`, `overlay` and `encryption`. The automated ones run against fakes of the disk, the volume and the initramfs; the board ones are checked on a real board against epic.

## Writing the system (verifies spec: PDSK)

- [x] A blank disk is written, read back and marked without a prompt
- [x] The bytes after the system on the disk are left alone
- [x] A disk holding another system asks to overwrite, and is written only on confirmation
- [x] Declining leaves the disk as it was and fails the step
- [x] The medium's own system and an earlier one are written again without asking
- [x] A disk written in part is written again without asking
- [x] A failed read of the payload fails the step and leaves no marker
- [x] A read-back that differs fails the step and leaves no marker
- [x] A drive smaller than the system fails the step
- [x] No NVMe drive, or two, fail the step
- [x] Progress runs from nothing to all over the write and the read-back
- [x] Done-state: the medium's marker is done; another marker, blank, partition labels only and another system are not done; no disk or two is unknown
- [x] Working out the done-state changes nothing
- [ ] On a board: the payload is read through dm-verity, and a stick with a corrupted block fails the step
- [ ] On a board: the written disk boots to the first-boot rotation with no complaint

## Installing the overlay (verifies spec: PDSK)

- [x] The overlay is installed in the current boot slot and in the directory kernel updates take overlays from
- [x] An older or differing overlay is replaced; one already in both places is left unwritten
- [x] A disk with no current boot slot, no root filesystem or no firmware partition fails the step
- [x] The root volume is opened by the empty passphrase, or by the TPM where that does not open it; neither fails the step
- [x] Done-state: both places holding the overlay is done; either missing or differing is not done; a system that cannot be looked into is unknown
- [ ] On a board: after a kernel update the new boot slot still carries the overlay

## Setting up encryption (verifies spec: PENC)

- [x] A disk as the image ships it ends with a new volume key, a new UUID, a recovery keyslot and a TPM keyslot, and the empty passphrase unlocks nothing
- [x] The record is appended to `records.tsv` with five tab-separated fields, and reported as `record`
- [x] A board that cannot be placed has a record with no version
- [x] A volume whose key and UUID are not the image's keeps them
- [x] Keyslots an earlier try left are removed
- [x] An interrupted re-encryption is resumed
- [x] Every interruption leaves a disk that opens, and the empty passphrase never goes before the record is saved
- [x] A restart after the record was saved leaves a second line, and the last is the board's record
- [x] A record that cannot be saved fails the step and leaves the empty passphrase
- [x] A system that cannot unlock from the TPM fails the step before anything changes
- [x] A TPM keyslot bound to PCRs fails the step
- [x] The TPM not answering, no suffix, no encrypted volume, or a volume nothing opens fails the step
- [x] A volume already set up passes without changing anything
- [x] Done-state: follows the disk; another board's TPM or the image's key is not done; no TPM or no disk is unknown
- [x] The recovery passphrase is six lowercase words of five characters or more from the word list, joined by hyphens
- [x] The system is rewired: crypttab, dracut configuration, the current slot's initramfs, and no staged slot
- [ ] On a board: the TPM keyslot has no PCR policy, and the system boots to the root volume with nobody present
- [ ] On a board: the first boot leaves the volume key and UUID alone, and still rotates the other identifiers
- [ ] On a board: a power cut at each stage of the step leaves a board the next run finishes
