# Build the provisioning image

The image is assembled from the published `pi` image, as the provisioning medium spec (PMED) describes. Everything here is how; the spec is what.

## Build

A `just provision-image` recipe does the whole assembly with `sudo`, so it runs the same on an `ubuntu-26.04-arm` runner and on a Linux laptop. No container: the root is LUKS, so the build needs loop devices and dm-crypt, which a container would only get privileged.

1. Find the newest `linux-images` GitHub release with a `pi` image, read its `manifest.json`, download the arm64 `pi` `.img.zst`, and check it against the manifest's SHA-256. Decompress it to raw (8 GiB), keeping one pristine copy for the payload.
2. Grow the working copy and append two GPT partitions after `root`: the payload (the raw image's size plus its verity tree) and the data partition (FAT32 `ITIPROV`, a few hundred MiB).
3. `losetup -P`, open `root` with the empty keyfile, and mount the `@` subvolume. Install:
   - `bliti`, from `just build-device` (to `/usr/local/bin/bliti`, where the unit expects it)
   - `services/bliti-provision.service`, enabled, and `/etc/bliti/provision.env` with `--medium <data mount> --board-id-source tpm-endorsement-key`
   - the bluetoothd configuration from `services/` that provisioning mode needs (peripheral-only)
   - getty autologin drop-ins for `tty1` and `serial-getty@ttyAMA10` (the Pi 5's `serial0`) that run `bliti provision`
   - a mount for the data partition found on the disk root came from, by partition number, and not by label or PARTUUID, which two sticks from the same image would share; the provisioning unit requires that mount, and the payload's partition, so it does not start without them
   - `/usr/share/bliti/provision/`: `pieeprom.bin`, `tamanu-iti-v4.dtbo`, `tamanu-iti-v4.0.eep` (`just hat-overlay`, `just hat-eeprom`), and `manifest`
   - the overlay also in `/etc/flash-kernel/dtbs/overlays/`
4. Firmware partition: copy the overlay into `current/overlays/` (which carries the `README` marker the firmware needs). In `config.txt`, set `force_eeprom_read=0`, add `dtoverlay=tamanu-iti-v4`, remove `dtoverlay=tpm-slb9670`, and add the ID bus on GPIO0/1 the way the prototype has it (`i2c-gpio` on pins 0 and 1).
5. Write the pristine `pi` image into the payload partition, then run `veritysetup format` with the hash tree after the data in the same partition (with its superblock), and record the root hash and offset.
6. `mkfs.vfat` the data partition, and write the manifest to `/usr/share/bliti/provision/manifest` and `manifest.txt`.
7. Unmount, close LUKS, detach the loop device, and compress with `zstd -T0` to `tamanu-iti-provision.img.zst`, with `.sha256` and `.txt` beside it.

The bootloader comes from `raspberrypi/rpi-eeprom`'s `firmware-2712/default/`, as the newest `pieeprom-YYYY-MM-DD.bin`. Its date is `BOOTLOADER_VERSION`.

`eepmake` comes from `raspi-utils-eeprom`. If Ubuntu doesn't package it for the runner, build it from `raspberrypi/utils`.

The medium needs a 32 GB stick or card: the stick's own 8 GiB system, the 8 GiB payload with its tree, and the data partition come to just over 16 GiB.

## CI

- A workflow, triggered by `push` to `main`, a weekly `schedule`, and `pull_request`, with its concurrency grouped by ref like `cd.yml`. Production uploads go through the `production` environment and its existing role. A scheduled run executes on `main`, so its OIDC subject is the one the role already trusts. Previews use the preview role.
- PR opt-in: a "Build the provisioning image" tickbox in `.github/pull_request_template.md`, read from the PR body. `pull_request` needs the `edited` type for ticking the box to take effect. Keep `edited` from re-running the web build and deploy in `cd.yml`.
- The image job uploads `downloads/` itself and deletes it from a preview prefix when the box is unticked. `deploy.js --prune` must leave `downloads/` alone, or every preview deploy would delete the image. PR close already removes the whole prefix.
- Uploads are `application/zstd` (and `text/plain` for `.sha256`/`.txt`), revalidated rather than immutable, since the name is stable.
- Check that the web app's service worker neither precaches nor intercepts `/downloads/`.

## To check on hardware

- The `pi` image's first-boot root growth with partitions after `root`: it must give up cleanly and not hold boot.
- The first boot of a fresh stick re-encrypts its root (`image.identity.luks-rekey`). Measure how long it takes on a USB stick. It happens once per stick.
- Identity rotation leaves the appended partitions alone (it only touches the identifiers it recorded).
- A programmed board and a blank one both come up on the medium with the TPM at `/dev/tpm0`, I²C3 up, and the EEPROM readable on the ID bus.

## Steps

- [ ] `just provision-image`, producing the image, digest and manifest
- [ ] Structural test of the image (see test cases), run in CI after the build
- [ ] CI workflow: main, weekly, PR tickbox; uploads; `deploy.js` prune exclusion
- [ ] PR template tickbox
- [ ] Boot it on the v4 prototype and on a blank board
