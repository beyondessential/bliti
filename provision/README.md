# The provisioning image

The image a provisioning medium is written from: a USB stick at the factory, or an SD card for a repair.
It boots a Tamanu Iti v4 into provisioning mode, as the [provisioning medium spec](../.workhorse/specs/provisioning/medium.md) describes.

It is assembled from the `pi` image `linux-images` publishes, not built from scratch:

| partition | holds |
| --- | --- |
| 1 to 3 | the `pi` image's own, with bliti in provisioning mode installed into its root and the HAT's configuration in its firmware partition |
| 4 | the `pi` image again, untouched, followed by its dm-verity hash tree: what the disk-write step writes to the NVMe |
| 5 | `ITIPROV`, FAT32: what runs keep, and `manifest.txt` |

| file | does |
| --- | --- |
| `fetch.sh` | fetches the newest `pi` image, checked against its release's checksum, and the newest Pi 5 bootloader |
| `build.sh` | assembles the image, as root |
| `test.sh` | checks a built image against the spec, as root |
| `root/` | files installed into the medium's root as they lie here |

## Building

`just provision-image` builds it into `target/provision/`, and `just provision-image-test` checks it.
It runs on Linux, with `sudo`, and needs about 40 GB free under `target/`.

The host needs `curl`, `jq`, `zstd`, `sgdisk`, `cryptsetup` and `veritysetup`, `dosfstools`, `dtc`, `eepmake` (from Raspberry Pi's `raspi-utils-eeprom`, or built from `raspberrypi/utils`), and podman or docker for `just build-device`.
The build installs any package the medium needs that the `pi` image lacks through a chroot, so on a host that is not arm64 it also needs qemu-user registered with binfmt_misc, with the `F` flag.

## On the medium

- `bliti-provision-medium.service` finds partitions 4 and 5 on the disk the root came from, checks and mounts the data partition at `/mnt/provision`, and links the payload at `/run/bliti-provision/payload`.
  `bliti-provision.service` requires it.
- The text console and the serial console open `bliti provision` with no login, and leave a root shell when it exits.
- The HAT's ID bus is `/dev/i2c-9`, a bit-banged bus on GPIO0/1, for the EEPROM step.
- The disk, overlay and encryption steps open the payload through `veritysetup`, write the NVMe, and run `cryptsetup`, `systemd-cryptenroll` and `dracut` (inside a chroot of the new system) against it.
  A run keeps its records in `records.tsv` on the data partition.
- The bootloader step reads and writes the Pi 5's bootloader flash on `/dev/spidev10.0` with `flashrom`, and makes its images with `rpi-eeprom-config`. Provisioning mode puts USB first in the boot order when it starts from a USB stick. `rpi-eeprom-update.service` is masked.
- The Tailscale step runs a `tailscaled` of its own, with no tunnel and its state on the NVMe's root, so it needs the bench's network: the board on a wired connection that gives it an address. It fails, saying so, where it cannot reach Tailscale.
- `/usr/share/bliti/provision/` holds the bootloader, the HAT overlay, the HAT EEPROM images and the manifest.

## Writing it

Raspberry Pi Imager writes it as it is, or with `dd`:

```sh
zstd -dc tamanu-iti-provision.img.zst | sudo dd of=/dev/sdX bs=4M conv=fsync status=progress
```

It needs a stick or card of 32 GB.
