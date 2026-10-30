# Write the disk and set up encryption

Builds the steps `disk`, `overlay` and `encryption` of PDSK and PENC against K4's `Step` trait, in `crates/bliti/src/provision/steps/`.

## Approach

- Everything a step does to the machine sits behind a trait, so the steps are tested on fakes backed by temporary files and directories: `Storage` (the NVMe, its partitions, mounts, the verity-opened payload), `Luks` (header, unlock tests, re-encryption, keyslots) and `Initramfs` (rebuilding the initramfs inside the new root). The real implementations run the tools the medium carries, as `bootloader::Spi` does for `flashrom`.
- The step logic (state machines, ordering, crash safety) is the part under test. The real implementations are checked on a board against epic, not by this card.
- The shared pieces are `provision/manifest.rs`, `storage.rs`, `luks.rs`, `system.rs` (opening the root volume, rewiring the new system), and the passphrase and records under `steps/encryption/`.

## Decisions

- The volume key rotation compares the disk's LUKS header with the payload's root volume, read through a read-only loop device on the verity-opened payload, so the manifest does not change.
- The initramfs is rebuilt in a chroot of the new root and installed into `current/` by rename. `new/` is removed. Nothing else stages a slot.
- Before any keyslot changes, the step checks that the new root can build a TPM-unlocking initramfs (dracut's `tpm2-tss` module present), so a system that cannot is left openable by the empty passphrase.
- The TPM keyslot is enrolled with no PCRs, and the step checks the token carries none.
- A restart after step 6 of PENC saves a second line for the board; the last is the record.

## Steps

- [x] Manifest reader
- [x] `Storage` trait, partition naming, real implementation
- [x] `Luks` trait, header parsing, real implementation
- [x] Opening the root volume (`system.rs`)
- [x] `disk` step with tests
- [x] `overlay` step with tests
- [x] Recovery passphrase and records file
- [x] Initramfs rewiring (`Initramfs`) with tests
- [x] `encryption` step with tests, including every interruption point
- [x] Registry and daemon wiring (medium path, board suffix)
- [x] Medium build: tools the steps run, checked by `test.sh`
- [x] Test cases ticked (the board ones stay unticked: they are checked on epic)

## To check on a board

- Confirm `systemd-cryptenroll` is in the package `build.sh` names for it, and that `--tpm2-pcrs=` with no value binds no PCR; the build and the step check these.
- The `pi` image's root carries dracut's `tpm2-tss` module; `test.sh` checks it, and without it `encryption` fails before changing anything.
- Confirm dropping the page cache with `fadvise` is enough for the read-back to come from the NVMe.
