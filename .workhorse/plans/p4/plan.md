# Build the provisioning image

The image is assembled from the published `pi` image, as the provisioning medium spec (PMED) describes. Everything here is how; the spec is what.

## Build

A `just provision-image` recipe does the whole assembly with `sudo`, so it runs the same on an `ubuntu-26.04-arm` runner and on a Linux laptop. No container: the root is LUKS, so the build needs loop devices and dm-crypt, which a container would only get privileged.

1. Find the newest `linux-images` GitHub release, skipping pre-releases, with a `resolute` `pi` image, read its `manifest.json`, download the arm64 `resolute` `pi` `.img.zst`, and check it against the release's `SHA256SUMS`. Both are served from `tools.ops.tamanu.io/linux-images/<version>/`, not attached to the GitHub release. Decompress it to raw (8 GiB), keeping one pristine copy for the payload.
2. Grow the working copy and append two GPT partitions after `root`: the payload (the raw image's size plus its verity tree) and the data partition (FAT32 `ITIPROV`, 512 MiB: at 256 MiB FAT32 sits at its minimum cluster count).
3. `losetup -P`, open `root` with the empty keyfile, and mount the `@` subvolume. Install:
   - `bliti`, from `just build-device` (to `/usr/local/bin/bliti`, where the unit expects it)
   - `services/bliti-provision.service`, enabled, and `/etc/bliti/provision.env` with `--medium <data mount> --board-id-source tpm-endorsement-key`
   - the bluetoothd configuration from `services/` that provisioning mode needs (peripheral-only)
   - getty autologin drop-ins for `tty1` and `serial-getty@ttyAMA10` (the Pi 5's `serial0`) that log in as root and run `bliti provision`, leaving the root shell once it exits
   - a mount for the data partition, checked and repaired by `fsck.vfat` before mounting, found on the disk root came from, by partition number, and not by label or PARTUUID, which two sticks from the same image would share; the provisioning unit requires that mount, and the payload's partition, so it does not start without them
   - `/usr/share/bliti/provision/`: `pieeprom.bin`, `tamanu-iti-v4.dtbo`, `tamanu-iti-v4.0.eep` (`just hat-overlay`, `just hat-eeprom`), and `manifest`
   - unattended upgrades off: `unattended-upgrades` and the `apt-daily` timers masked
4. Firmware partition: copy the overlay into `current/overlays/` (which carries the `README` marker the firmware needs). In `config.txt`, keep the firmware from loading the EEPROM's overlay with a bare `dtoverlay=` at the top (Raspberry Pi documents this as blocking any HAT overlay when it comes before every other `dtoverlay`/`dtparam`), add `dtoverlay=tamanu-iti-v4`, remove `dtoverlay=tpm-slb9670`, and add the ID bus on GPIO0/1 the way the prototype had it, `i2c-gpio` on pins 0 and 1. It is pinned to `bus=9`: left dynamic, the overlay's own docs say it probably lands on 3, which is I²C3's number and the one bliti reads the sensors at.
5. Write the pristine `pi` image into the payload partition, then run `veritysetup format` with the hash tree after the data in the same partition (with its superblock), and record the root hash and offset.
6. `mkfs.vfat` the data partition, and write the manifest to `/usr/share/bliti/provision/manifest` and `manifest.txt`.
7. Unmount, close LUKS, detach the loop device, and compress with `zstd -T0` to `tamanu-iti-provision.img.zst`, with `.sha256` and `.txt` beside it.

The bootloader comes from `raspberrypi/rpi-eeprom`'s `firmware-2712/default/`, as the newest `pieeprom-YYYY-MM-DD.bin`. Its date is `BOOTLOADER_VERSION`.

`eepmake` comes from `raspi-utils-eeprom`, which resolute carries in main for arm64. A host without it builds it from `raspberrypi/utils` (`eeptools/`).

The `pi` image lacks `dosfstools`, which the medium unit's check needs; the build installs it through a chroot (qemu-user on a non-arm host), then removes the package lists again, as `linux-images` ships none. `unattended-upgrades` is not installed, but the `apt-daily` timers are enabled, so masking is still needed.

The medium needs a 32 GB stick or card: the stick's own 8 GiB system, the 8 GiB payload with its tree, and the data partition come to about 16.6 GiB. Compressed it is about 7 GB, twice the `pi` image's 3.5 GB, since the root and the payload are the same LUKS data and too far apart for zstd to share.

The `pi` image's first-boot root growth runs `growpart` on partition 3, which finds no room with the payload after it and exits cleanly ("No change needed"), so it does not hold boot.

## CI

- A workflow, triggered by `push` to `main`, a weekly `schedule`, and `pull_request`, with its concurrency grouped by ref like `cd.yml`. Production uploads go through the `production` environment and its existing role. A scheduled run executes on `main`, so its OIDC subject is the one the role already trusts. Previews use the preview role.
- PR opt-in, since an image is several gigabytes: a "Build the provisioning image" tickbox in `.github/pull_request_template.md`, read from the PR body. While it is ticked, the PR's preview serves an image built from its head at the same `/downloads/` paths; unticked, it serves none. `pull_request` needs the `edited` type for ticking the box to take effect, which is one reason the image gets a workflow of its own: `cd.yml` keeps its triggers, so editing a PR body never rebuilds the web app.
- The image job uploads `downloads/` itself and deletes it from a preview prefix when the box is unticked. `deploy.js --prune` must leave `downloads/` alone, or every preview deploy would delete the image. PR close already removes the whole prefix.
- Upload the image first and the digest and manifest after it, so the digest and manifest describe an image not yet served only for the moment the small uploads take.
- Uploads are `application/zstd` (and `text/plain` for `.sha256`/`.txt`), revalidated rather than immutable, since the name is stable.
- The web app's service worker answers every same-origin GET that is not a page navigation through its runtime cache (`respond` in `src/sw.js`), so a `fetch` of the manifest would be cached stale, and of the image would put gigabytes in the cache. It has to leave `/downloads/` to the network.
- Like `cd.yml`, the build job holds no credentials, since it runs the PR's code; upload jobs only download what it made. If an artefact of several GB is too much for `actions/upload-artifact`, main and weekly builds (which run main's code) can build and upload in one job, and PR builds stay split.
- The decide job reads the box from the body, and on an `edited` event acts only when the box itself changed (ticked: build; unticked: remove), so editing a ticked PR's title or text does not rebuild.
- Disk on the arm runner: the raw image, the decompressed `pi` image and the compressed output are each GBs. Keep image files sparse throughout (`truncate`, `cp --sparse=always`, `dd conv=sparse`), and free preinstalled toolchains first if that is still short.

## To check on hardware

- The `pi` image's first-boot root growth with partitions after `root`: by its script it exits cleanly, but confirm it on a stick.
- The first boot of a fresh stick re-encrypts its root (`image.identity.luks-rekey`). Measure how long it takes on a USB stick. It happens once per stick.
- Identity rotation leaves the appended partitions alone (it only touches the identifiers it recorded).
- That the bare `dtoverlay=` the build puts at the top of `config.txt` blocks the EEPROM's overlay on a programmed board, and whether `/proc/device-tree/hat/` is still published. If it does not block it, `force_eeprom_read=0` instead, which stops the firmware reading the EEPROM at all.
- That the ID bus comes up as `/dev/i2c-9` with the EEPROM answering at `0x50`.
- A programmed board and a blank one both come up on the medium with the TPM at `/dev/tpm0`, I²C3 up, and the EEPROM readable on the ID bus.

## Open: two disks with one image's labels

The `pi` image finds its own partitions by GPT label: `/etc/crypttab` (also in the initramfs) and its first-boot identity rotation use `/dev/disk/by-partlabel/root`, and `/etc/fstab` uses `by-partlabel/firmware` and `by-partlabel/xboot`. Once the disk-write step has written the payload to the NVMe, the board holds two disks with partitions labelled `root`, `firmware` and `xboot`, and which one a `by-partlabel` link points at is whichever udev saw last. A boot from the medium during the rest of the run could unlock and mount the NVMe's root as its own, or rotate the NVMe's identity. The medium's own payload and data partitions are found by number on the boot disk and are not affected. This is for the card that builds the disk-write step to settle, likely in `linux-images` (finding its partitions on the disk the firmware booted from) rather than by relabelling the medium here.

## Steps

The assembly lives in `provision/` at the repository root, beside `hat/` and `services/`: the scripts, and under `provision/root/` the files installed into the medium's root as they lie there.

### Assembly

- [x] `provision/fetch.sh <dir>`: the newest non-draft, non-pre-release `linux-images` release with a `resolute` arm64 `pi` `.img.zst` in its `manifest.json`; download and check it against the release's `SHA256SUMS`, failing on a mismatch; then the newest `pieeprom-YYYY-MM-DD.bin` in `rpi-eeprom`'s `firmware-2712/default/`
- [x] `provision/build.sh <dir>`, run as root: build steps 2 to 7 above, taking `bliti` from `target/device/` and the overlay and EEPROM image from `target/hat/`, and failing on any step's failure with the loop device, LUKS mapping and mounts cleaned up on exit
- [x] Runtime packages: check the `pi` image carries what `bliti provision-mode` needs (`bluez`, `libdbus-1-3`, the `libtss2` ESYS and device TCTI libraries) and what the medium's units use (`dosfstools`); install any it lacks into the root through a chroot, native on the arm runner
- [x] `provision/root/usr/local/libexec/bliti-provision-console`: runs `bliti provision`, then `exec`s a root login shell
- [x] `provision/root/etc/systemd/system/getty@tty1.service.d/` and `serial-getty@ttyAMA10.service.d/` drop-ins: autologin as root with that script as the login program
- [x] `provision/root/etc/systemd/system/bliti-provision-medium.service`: finds the disk `/` came from through its LUKS mapping, requires partitions N+1 (payload) and N+2 (data) after the `pi` image's last, runs `fsck.vfat -a` on the data partition and mounts it read-write at `/mnt/provision`, and links the payload partition at `/run/bliti-provision/payload` for the disk-write step; any failure is written to standard error and fails the unit
- [x] `services/bliti-provision.service`: `Requires=` and `After=` the medium unit, so provisioning mode does not start without it
- [x] `/etc/bliti/provision.env`, written by the build: `--medium /mnt/provision --board-id-source tpm-endorsement-key`
- [x] Install `services/bliti-bluetoothd.conf` and `services/bliti-bluetooth-dropin.conf` where they say
- [x] Mask `unattended-upgrades`, `apt-daily.timer` and `apt-daily-upgrade.timer`
- [x] Firmware partition and `config.txt` edits (build step 4), with the `dtoverlay=` form pending the hardware check
- [x] The manifest, in PMED's order, written to the root and to `manifest.txt` on the data partition
- [x] `just provision-image`: depends on `build-device`, `hat-overlay` and `hat-eeprom`, then runs `fetch.sh` and `sudo provision/build.sh` into `target/provision/`
- [x] `provision/README.md`: what the recipe needs on the host (`dtc`, `eepmake`, `cryptsetup`, `dosfstools`, `zstd`, podman or docker), and how to write the image to a stick

### Structural test

- [x] `provision/test.sh <image>`, run as root: every case under "Image structure" in the test cases, including a flipped payload byte failing a read through verity, and the image fitting a stick sold as 32 GB
- [x] `just provision-image-test`, running it on `target/provision/`

### Web app

- [x] `src/sw.js`: leave every request under `/downloads/` to the network, uncached, with a case in `tests/service-worker.spec.js`
- [x] `deploy.js --prune`: never delete under `downloads/`, with a case in `tests/deploy.spec.js`

### CI

- [x] `.github/pull_request_template.md`: a "Build the provisioning image" tickbox with a marker comment, as Review Hero's has
- [x] `.github/workflows/provision-image.yml`: `push` to main, a weekly `schedule`, and `pull_request` (`opened`, `synchronize`, `reopened`, `edited`); concurrency per ref, never cancelled
- [x] Build job on `ubuntu-26.04-arm`, without credentials: install the host tools, `just provision-image`, `just provision-image-test`, upload the three files. On a PR, only when the box is ticked and the PR is from this repository
- [x] Production job, in the `production` environment: upload to `prod/downloads/`, the image first, then the digest and manifest
- [x] Preview job: the same to `previews/pr-<n>/downloads/` while ticked; a job that deletes `previews/pr-<n>/downloads/` when it is not
- [x] Content types and cache headers as noted under CI. The CloudFront distribution serves `/downloads/` straight from the bucket: the viewer-request function in `infra` (`pulumi/bliti`) maps a path with an extension to its key under the origin's prefix and only rewrites compressible types, a missing path is S3's own 404, and both deploy roles may write and delete anything under their prefix

### On hardware

- [ ] Work through "To check on hardware", settling the `config.txt` form
- [ ] Boot the image on the v4 prototype and on a blank board, and tick the hardware test cases
