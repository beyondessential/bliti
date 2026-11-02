#!/usr/bin/env bash
# Assemble the provisioning image (PMED) in <dir>, from what fetch.sh put there, the daemon from
# `just build-device`, and the overlay and EEPROM images from `just hat-overlay` and `just hat-eeprom`.
# Run as root: it uses loop devices, dm-crypt and mounts. Writes, in <dir>:
#
#   tamanu-iti-provision.img.zst         the image
#   tamanu-iti-provision.img.zst.sha256  its digest, as sha256sum writes it
#   tamanu-iti-provision.txt             its manifest
#
# BLITI_COMMIT is the commit being built, which root cannot always ask git for in someone else's
# checkout.
set -euo pipefail

dir=$(realpath "${1:?usage: build.sh <dir>}")
repo=$(realpath "$(dirname "$0")/..")
commit=${BLITI_COMMIT:?BLITI_COMMIT must name the commit being built}
built=$(date -u +%Y-%m-%dT%H:%M:%SZ)

if ((EUID != 0)); then
	echo "build.sh runs as root" >&2
	exit 1
fi

bliti=$repo/target/device/aarch64-unknown-linux-gnu/release/bliti
hat=$repo/target/hat
for input in "$dir/pi.img.zst" "$dir/pi-version" "$dir/pieeprom.bin" "$dir/bootloader-version" \
	"$bliti" "$hat/tamanu-iti-v4.dtbo"; do
	[[ -f $input ]] || {
		echo "missing $input" >&2
		exit 1
	}
done
shopt -s nullglob
eeproms=("$hat"/tamanu-iti-v*.eep)
shopt -u nullglob
((${#eeproms[@]})) || {
	echo "no HAT EEPROM images in $hat" >&2
	exit 1
}

name=tamanu-iti-provision
pristine=$dir/pi.img
work=$dir/$name.img
mnt=$dir/mnt
mkdir -p "$mnt"

key=$dir/empty-key

# Undone in reverse, on any exit, so a failed build leaves no loop device, mapping or mount behind.
teardown=()
undo() {
	local i
	for ((i = ${#teardown[@]} - 1; i >= 0; i--)); do
		eval "${teardown[i]}" || true
	done
	teardown=()
}
trap 'undo; rm -f "$work" "$pristine" "$key"' EXIT

echo "decompressing the pi image"
zstd -d -q -f "$dir/pi.img.zst" -o "$pristine"
pi_size=$(stat -c %s "$pristine")
pi_sha=$(sha256sum "$pristine" | cut -d' ' -f1)
((pi_size % 4096 == 0)) || {
	echo "the pi image is not a whole number of 4 KiB blocks" >&2
	exit 1
}

# The hash tree of 4 KiB blocks of SHA-256 digests: each level holds 128 digests to a block, and the
# superblock takes a block ahead of it.
blocks=$((pi_size / 4096)) tree=1
for ((n = blocks; n > 1; )); do
	n=$(((n + 127) / 128))
	tree=$((tree + n))
done
mib=$((1024 * 1024))
mib_up() {
	echo $((($1 + mib - 1) / mib * mib))
}
payload_size=$(mib_up $((pi_size + tree * 4096)))
data_size=$((512 * mib))

# Sectors of 512 bytes, each partition on a 1 MiB boundary. The payload begins where the pi image
# ended, over what was its backup partition table.
payload_start=$(($(mib_up "$pi_size") / 512))
payload_end=$((payload_start + payload_size / 512 - 1))
data_start=$((payload_end + 1))
data_end=$((data_start + data_size / 512 - 1))

echo "laying out $work"
cp --sparse=always "$pristine" "$work"
truncate -s $(((data_end + 1) * 512 + mib)) "$work"
# The pi image's own partitions are renamed, because a disk written from the payload carries their
# names, and both the medium's system and that disk's find their partitions by name (PMED).
sgdisk --move-second-header \
	--change-name=1:provision-firmware --change-name=2:provision-xboot --change-name=3:provision-root \
	--new="4:$payload_start:$payload_end" --typecode=4:8300 --change-name=4:provision-payload \
	--new="5:$data_start:$data_end" --typecode=5:0700 --change-name=5:provision-data \
	"$work" >/dev/null
# The pi image's backup partition table, left behind in the gap before the payload once the table has
# a new one at the end.
dd if=/dev/zero of="$work" bs=512 seek=$((pi_size / 512 - 33)) count=33 conv=notrunc status=none

loop=$(losetup --partscan --show --find "$work")
teardown+=("losetup -d '$loop'")
udevadm settle
for p in 1 3 4 5; do
	[[ -b ${loop}p$p ]] || {
		echo "${loop}p$p did not appear" >&2
		exit 1
	}
done

echo "writing the payload"
dd if="$pristine" of="${loop}p4" bs=4M conv=sparse,fsync status=none
root_hash=$(veritysetup format --hash-offset="$pi_size" --data-blocks="$blocks" \
	"${loop}p4" "${loop}p4" | awk '/^Root hash:/ { print $3 }')
[[ $root_hash =~ ^[0-9a-f]{64}$ ]] || {
	echo "veritysetup gave no root hash" >&2
	exit 1
}

manifest=$dir/$name.txt
cat >"$manifest" <<EOF
BLITI_COMMIT=$commit
BUILT=$built
PI_IMAGE_VERSION=$(<"$dir/pi-version")
PI_IMAGE_SIZE=$pi_size
PI_IMAGE_SHA256=$pi_sha
PAYLOAD_HASH_OFFSET=$pi_size
PAYLOAD_ROOT_HASH=$root_hash
BOOTLOADER_VERSION=$(<"$dir/bootloader-version")
EOF

echo "installing provisioning mode into the root"
: >"$key"
mapping=bliti-provision-build-$$
cryptsetup open --key-file="$key" "${loop}p3" "$mapping"
teardown+=("cryptsetup close '$mapping'")
root=$mnt/root
mkdir -p "$root"
mount -o subvol=@ "/dev/mapper/$mapping" "$root"
teardown+=("umount '$root'")

# What provisioning mode and the medium's units run, by the package that carries each.
declare -A needs=(
	[/usr/sbin/fsck.vfat]=dosfstools
	[/usr/sbin/cryptsetup]=cryptsetup-bin
	[/usr/sbin/veritysetup]=cryptsetup-bin
	[/usr/libexec/bluetooth/bluetoothd]=bluez
	[/usr/sbin/agetty]=util-linux
	[/usr/sbin/flashrom]=flashrom
	[/usr/bin/rpi-eeprom-config]=rpi-eeprom
	[/usr/bin/systemd-cryptenroll]=systemd-cryptsetup
	[/usr/bin/dracut]=dracut-core
)
for fs in dev dev/pts proc sys; do
	mount --bind "/$fs" "$root/$fs"
	teardown+=("umount '$root/$fs'")
done
# The image points resolv.conf into /run, which is empty until it boots.
mount -t tmpfs tmpfs "$root/run"
teardown+=("umount '$root/run'")
# What the pi image installs, read before anything is installed here.
mkdir -p "$root/run/bliti-pristine"
cp "$root/var/lib/dpkg/status" "$root/run/bliti-pristine/status"
mkdir -p "$root/run/systemd/resolve"
cp -L /etc/resolv.conf "$root/run/systemd/resolve/stub-resolv.conf"
chroot "$root" apt-get update -q

missing=()
for path in "${!needs[@]}"; do
	[[ -e $root$path ]] || missing+=("${needs[$path]}")
done
if ((${#missing[@]})); then
	mapfile -t missing < <(printf '%s\n' "${missing[@]}" | sort -u)
	echo "installing ${missing[*]}"
	DEBIAN_FRONTEND=noninteractive NEEDRESTART_SUSPEND=1 chroot "$root" \
		apt-get install -y -q --no-install-recommends "${missing[@]}"
fi
for path in "${!needs[@]}"; do
	[[ -e $root$path ]] || {
		echo "the root has no $path" >&2
		exit 1
	}
done

# The packages the step `software` installs in the disk's system for the Iti's network configuration
# (PDSK): what iwd and hostapd need, without recommends, that the pi image does not already hold, as
# apt would install them on the pi image as it ships.
share=$root/usr/share/bliti/provision
debs=$share/debs
install -d -m 755 "$debs"
plan=$(chroot "$root" apt-get install --simulate --no-install-recommends -q \
	-o Dir::State::status=/run/bliti-pristine/status iwd hostapd)
if grep -q '^Remv ' <<<"$plan"; then
	echo "installing iwd and hostapd on the pi image would remove packages:" >&2
	grep '^Remv ' <<<"$plan" >&2
	exit 1
fi
mapfile -t carried < <(awk '/^Inst / { print $2 }' <<<"$plan")
if ((${#carried[@]})); then
	echo "carrying ${carried[*]}"
	chroot "$root" sh -c 'cd "$1" && shift && exec apt-get download -q -o APT::Sandbox::User=root "$@"' \
		sh /usr/share/bliti/provision/debs "${carried[@]}"
fi
for package in "${carried[@]}"; do
	compgen -G "$debs/${package%%:*}_*.deb" >/dev/null || {
		echo "apt downloaded no package for $package" >&2
		exit 1
	}
done
find "$debs" -name '*.deb' -exec chmod 644 {} +

# The image ships with no package lists, as linux-images builds it.
chroot "$root" apt-get clean
rm -rf "$root/var/lib/apt/lists/"*

install -D -m 755 "$bliti" "$root/usr/local/bin/bliti"
if chroot "$root" ldd /usr/local/bin/bliti | grep 'not found'; then
	echo "the root lacks libraries bliti links" >&2
	exit 1
fi

install -D -m 644 "$repo/services/bliti-provision.service" "$root/etc/systemd/system/bliti-provision.service"
install -D -m 644 "$repo/services/bliti-bluetoothd.conf" "$root/etc/bluetooth/bliti.conf"
install -D -m 644 "$repo/services/bliti-bluetooth-dropin.conf" \
	"$root/etc/systemd/system/bluetooth.service.d/10-bliti.conf"
cp -r --preserve=mode "$repo/provision/root/." "$root/"
install -d -m 755 "$root/etc/bliti"
echo 'BLITI_PROVISION_ARGS=--medium /mnt/provision --board-id-source tpm-endorsement-key' \
	>"$root/etc/bliti/provision.env"

install -m 644 "$dir/pieeprom.bin" "$hat/tamanu-iti-v4.dtbo" "${eeproms[@]}" "$share/"
install -m 644 "$manifest" "$share/manifest"
# The Iti's software, at the paths the step `software` installs it to in the disk's system (PDSK).
software=$share/system
install -D -m 755 "$bliti" "$software/usr/local/bin/bliti"
install -D -m 644 "$repo/services/bliti.service" "$software/etc/systemd/system/bliti.service"
install -D -m 644 "$repo/services/bliti-bluetoothd.conf" "$software/etc/bluetooth/bliti.conf"
install -D -m 644 "$repo/services/bliti-bluetooth-dropin.conf" \
	"$software/etc/systemd/system/bluetooth.service.d/10-bliti.conf"
install -D -m 644 "$repo/services/bliti-journald.conf" "$software/etc/systemd/journald.conf.d/10-bliti.conf"
install -D -m 644 "$repo/services/bliti-daemon.env" "$software/etc/bliti/daemon.env"
install -D -m 644 "$repo/services/bliti-iwd-dropin.conf" \
	"$software/etc/systemd/system/iwd.service.d/10-bliti.conf"
install -D -m 644 "$repo/services/bliti-hostapd@.service" "$software/etc/systemd/system/bliti-hostapd@.service"

systemctl --root="$root" enable bliti-provision-medium.service bliti-provision.service
# The medium does not update its own software (PMED), and the bootloader is written by the step that
# configures it (PBL), not by rpi-eeprom's own updater.
systemctl --root="$root" mask unattended-upgrades.service \
	apt-daily.timer apt-daily.service apt-daily-upgrade.timer apt-daily-upgrade.service \
	rpi-eeprom-update.service
# The medium changes no disk as it boots, other than checking its data partition (PMED): the root grow
# finds its partition by the pi image's name, and the first boot's identity rotation is left out of
# the initramfs by provision/root's dracut configuration. Nor is it ever on a tailnet: the step that
# signs in to Tailscale runs a tailscaled of its own (PTS). The pi image installs the units that are
# in /etc there, where their masks have to go.
systemctl --root="$root" mask tailscaled.service
for unit in grow-root-filesystem.service bes-tailscale-firstboot-auth.service; do
	systemctl --root="$root" disable "$unit"
	rm "$root/etc/systemd/system/$unit"
	systemctl --root="$root" mask "$unit"
done
sed -i -E 's#/dev/disk/by-partlabel/(firmware|xboot|root)([[:space:]])#/dev/disk/by-partlabel/provision-\1\2#' \
	"$root/etc/crypttab" "$root/etc/fstab"

echo "configuring the firmware"
fw=$mnt/firmware
mkdir -p "$fw"
mount "${loop}p1" "$fw"
teardown+=("umount '$fw'")
overlays=$fw/current/overlays
[[ -f $overlays/README ]] || {
	echo "the pi image's boot slot has no overlays/README" >&2
	exit 1
}
install -m 644 "$hat/tamanu-iti-v4.dtbo" "$overlays/"

config=$fw/config.txt
{
	cat <<-'EOF'
		# The provisioning medium loads the HAT overlay itself, at the end of this file, so a blank
		# EEPROM and a programmed one give the same devices (PMED). A bare dtoverlay= ahead of every
		# other dtoverlay and dtparam keeps the firmware from loading the overlay an EEPROM names.
		dtoverlay=

		# A factory board's supply reports no 5 A, and without this the firmware stops a USB boot to
		# wait for the power button.
		usb_max_current_enable=1

	EOF
	# tpm-slb9670 declares the TPM on SPI0 CE1, which the HAT overlay declares too.
	grep -v '^dtoverlay=tpm-slb9670' "$config"
	cat <<-'EOF'

		# The HAT, and its ID bus on GPIO0/1 as /dev/i2c-9 for the EEPROM step.
		[all]
		dtoverlay=tamanu-iti-v4
		dtoverlay=i2c-gpio,i2c_gpio_sda=0,i2c_gpio_scl=1,bus=9
	EOF
} >"$config.new"
mv "$config.new" "$config"

echo "making the medium's initramfs"
kernels=("$root"/usr/lib/modules/*)
((${#kernels[@]} == 1)) || {
	echo "the root carries ${#kernels[@]} kernels, not one" >&2
	exit 1
}
chroot "$root" dracut --force --kver "$(basename "${kernels[0]}")" /var/tmp/bliti-initrd.img
mv "$root/var/tmp/bliti-initrd.img" "$fw/current/initrd.img"

echo "formatting the data partition"
mkfs.vfat -F 32 -n ITIPROV "${loop}p5" >/dev/null
data=$mnt/data
mkdir -p "$data"
mount "${loop}p5" "$data"
teardown+=("umount '$data'")
install -m 644 "$manifest" "$data/manifest.txt"

# Unmount and detach now, so everything is written before the image is read back to compress it.
sync
undo

echo "compressing"
zstd -T0 -q -f "$work" -o "$dir/$name.img.zst"
(cd "$dir" && sha256sum "$name.img.zst" >"$name.img.zst.sha256")
rmdir "$mnt"/* "$mnt" 2>/dev/null || true

if [[ -n ${SUDO_UID:-} ]]; then
	chown "$SUDO_UID:${SUDO_GID:-$SUDO_UID}" "$dir/$name.img.zst" "$dir/$name.img.zst.sha256" "$manifest"
fi
echo "built $dir/$name.img.zst"
