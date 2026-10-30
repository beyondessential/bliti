#!/usr/bin/env bash
# Check the provisioning image build.sh left in <dir> against PMED, from the outside: its layout, its
# payload and hash tree, what its root and firmware partition carry, and its manifest. Run as root,
# after build.sh, with what fetch.sh fetched still in <dir>. Prints each failure, and exits non-zero
# if there was one.
set -euo pipefail

dir=$(realpath "${1:?usage: test.sh <dir>}")
repo=$(realpath "$(dirname "$0")/..")
name=tamanu-iti-provision

if ((EUID != 0)); then
	echo "test.sh runs as root" >&2
	exit 1
fi

failures=0
pass() { echo "ok    $*"; }
fail() {
	echo "FAIL  $*"
	failures=$((failures + 1))
}
check() {
	local what=$1
	shift
	if "$@"; then pass "$what"; else fail "$what"; fi
}

image=$dir/test.img
pristine=$dir/test-pi.img
mnt=$dir/test-mnt
teardown=()
undo() {
	local i
	for ((i = ${#teardown[@]} - 1; i >= 0; i--)); do
		eval "${teardown[i]}" || true
	done
	teardown=()
}
trap 'undo; rm -f "$image" "$pristine" "$dir/test-key"; rmdir "$mnt"/* "$mnt" 2>/dev/null || true' EXIT

(cd "$dir" && sha256sum --check --status "$name.img.zst.sha256") ||
	fail "$name.img.zst matches its .sha256"
zstd -d -q -f "$dir/$name.img.zst" -o "$image"
zstd -d -q -f "$dir/pi.img.zst" -o "$pristine"

declare -A manifest
while IFS='=' read -r key value; do
	manifest[$key]=$value
done <"$dir/$name.txt"

# Smaller than any USB stick or SD card sold as 32 GB holds.
check "the image fits a 32 GB stick" test "$(stat -c %s "$image")" -le 29000000000

# The pi image's partitions, unchanged, then the payload and the data partition.
layout() {
	sfdisk --dump "$1" | grep '^/' | sed 's/^[^:]*: //' | head -n "$2"
}
check "the pi image's partitions come first, unchanged" \
	test "$(layout "$image" 3)" = "$(layout "$pristine" 3)"
check "the image holds five partitions" test "$(sfdisk --dump "$image" | grep -c '^/')" -eq 5

loop=$(losetup --partscan --read-only --show --find "$image")
teardown+=("losetup -d '$loop'")
udevadm settle
payload=${loop}p4
data=${loop}p5

check "the data partition is FAT32" test "$(blkid -p -o value -s VERSION "$data")" = FAT32
check "the data partition is labelled ITIPROV" test "$(blkid -p -o value -s LABEL "$data")" = ITIPROV
check "the data partition is clean" fsck.vfat -n "$data" >/dev/null

size=${manifest[PI_IMAGE_SIZE]}
check "the payload begins with the pi image" \
	test "$(head -c "$size" "$payload" | sha256sum | cut -d' ' -f1)" = "${manifest[PI_IMAGE_SHA256]}"
check "PI_IMAGE_SHA256 is the pi image's" \
	test "$(sha256sum "$pristine" | cut -d' ' -f1)" = "${manifest[PI_IMAGE_SHA256]}"
check "PI_IMAGE_SIZE is the pi image's" test "$(stat -c %s "$pristine")" -eq "$size"
sums=$(curl --proto '=https' -fsSL "https://tools.ops.tamanu.io/linux-images/${manifest[PI_IMAGE_VERSION]}/SHA256SUMS")
check "the pi image is one its release publishes" \
	grep -q "^$(sha256sum "$dir/pi.img.zst" | cut -d' ' -f1) " <<<"$sums"
check "the hash tree verifies against the manifest" \
	veritysetup verify --hash-offset="${manifest[PAYLOAD_HASH_OFFSET]}" \
	"$payload" "$payload" "${manifest[PAYLOAD_ROOT_HASH]}"

mkdir -p "$mnt/data" "$mnt/root" "$mnt/firmware"
mount -o ro "$data" "$mnt/data"
teardown+=("umount '$mnt/data'")
: >"$dir/test-key"
mapping=bliti-provision-test-$$
cryptsetup open --readonly --key-file="$dir/test-key" "${loop}p3" "$mapping"
teardown+=("cryptsetup close '$mapping'")
mount -o ro,subvol=@ "/dev/mapper/$mapping" "$mnt/root"
teardown+=("umount '$mnt/root'")
mount -o ro "${loop}p1" "$mnt/firmware"
teardown+=("umount '$mnt/firmware'")
root=$mnt/root
fw=$mnt/firmware
share=$root/usr/share/bliti/provision

order="BLITI_COMMIT BUILT PI_IMAGE_VERSION PI_IMAGE_SIZE PI_IMAGE_SHA256 PAYLOAD_HASH_OFFSET PAYLOAD_ROOT_HASH BOOTLOADER_VERSION"
check "the manifest has every entry, in order" \
	test "$(cut -d= -f1 "$dir/$name.txt" | tr '\n' ' ')" = "$order "
manifest_values() {
	[[ ${manifest[BLITI_COMMIT]} =~ ^[0-9a-f]{40}$ ]] &&
		[[ ${manifest[BUILT]} =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$ ]] &&
		[[ ${manifest[PI_IMAGE_SHA256]} =~ ^[0-9a-f]{64}$ ]] &&
		[[ ${manifest[PAYLOAD_ROOT_HASH]} =~ ^[0-9a-f]{64}$ ]] &&
		[[ ${manifest[BOOTLOADER_VERSION]} =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]]
}
check "the manifest's values are in their forms" manifest_values
check "the root carries the manifest" cmp -s "$dir/$name.txt" "$share/manifest"
check "the data partition carries the manifest" cmp -s "$dir/$name.txt" "$mnt/data/manifest.txt"

check "the root carries bliti" cmp -s "$repo/target/device/aarch64-unknown-linux-gnu/release/bliti" \
	"$root/usr/local/bin/bliti"
check "provisioning mode is enabled" \
	test -L "$root/etc/systemd/system/multi-user.target.wants/bliti-provision.service"
check "the medium unit is enabled" \
	test -L "$root/etc/systemd/system/multi-user.target.wants/bliti-provision-medium.service"
check "provisioning mode requires the medium unit" \
	grep -qx 'Requires=bliti-provision-medium.service' "$root/etc/systemd/system/bliti-provision.service"
check "provisioning mode takes when the image was built from the manifest" \
	grep -qx 'EnvironmentFile=/usr/share/bliti/provision/manifest' "$root/etc/systemd/system/bliti-provision.service"
check "provision.env names the data mount and the TPM Endorsement Key" \
	grep -qx 'BLITI_PROVISION_ARGS=--medium /mnt/provision --board-id-source tpm-endorsement-key' \
	"$root/etc/bliti/provision.env"
no_daemon() {
	! find "$root/etc/systemd" "$root/usr/lib/systemd" -name bliti.service | grep -q .
}
check "the ordinary daemon is not installed" no_daemon
check "bluetoothd is peripheral-only" test -f "$root/etc/bluetooth/bliti.conf" -a \
	-f "$root/etc/systemd/system/bluetooth.service.d/10-bliti.conf"
for tty in getty@tty1 serial-getty@ttyAMA10; do
	check "$tty opens the console command with no login" grep -qe \
		'--autologin root --login-program /usr/local/libexec/bliti-provision-console' \
		"$root/etc/systemd/system/$tty.service.d/10-bliti-provision.conf"
done
check "the root carries fsck.vfat" test -x "$root/usr/sbin/fsck.vfat"
check "the root carries flashrom and rpi-eeprom-config" test -x "$root/usr/sbin/flashrom" -a \
	-x "$root/usr/bin/rpi-eeprom-config"
check "the root carries what the disk and encryption steps run" test \
	-x "$root/usr/bin/systemd-cryptenroll" -a -x "$root/usr/bin/dracut" -a \
	-x "$root/usr/sbin/cryptsetup" -a -x "$root/usr/sbin/veritysetup" -a \
	-x "$root/usr/sbin/blkid" -a -x "$root/usr/sbin/losetup" -a -x "$root/usr/sbin/blockdev"
check "the root can make an initramfs that unlocks from the TPM" \
	compgen -G "$root/usr/lib/dracut/modules.d/*tpm2-tss" >/dev/null
check "the root carries tailscaled, which the Tailscale step runs" test -x "$root/usr/sbin/tailscaled"
check "the root holds no package lists" test -z "$(find "$root/var/lib/apt/lists" -maxdepth 1 -type f)"
masked() {
	local unit
	for unit in unattended-upgrades.service apt-daily.timer apt-daily.service \
		apt-daily-upgrade.timer apt-daily-upgrade.service \
		rpi-eeprom-update.service; do
		[[ $(readlink "$root/etc/systemd/system/$unit") == /dev/null ]] || return 1
	done
}
check "nothing upgrades packages unattended" masked

check "the root carries the bootloader" cmp -s "$dir/pieeprom.bin" "$share/pieeprom.bin"
check "the root carries the HAT overlay" cmp -s "$repo/target/hat/tamanu-iti-v4.dtbo" "$share/tamanu-iti-v4.dtbo"
for eep in "$repo"/target/hat/tamanu-iti-v*.eep; do
	check "the root carries $(basename "$eep")" cmp -s "$eep" "$share/$(basename "$eep")"
done

config=$fw/config.txt
check "the firmware loads no overlay an EEPROM names" \
	test "$(grep -v '^\s*\(#.*\)\?$' "$config" | head -n1)" = "dtoverlay="
check "the firmware loads the HAT overlay" grep -qx 'dtoverlay=tamanu-iti-v4' "$config"
check "the firmware does not load tpm-slb9670" bash -c "! grep -q '^\s*dtoverlay=tpm-slb9670' '$config'"
check "the firmware brings up the ID bus" \
	grep -qx 'dtoverlay=i2c-gpio,i2c_gpio_sda=0,i2c_gpio_scl=1,bus=9' "$config"
check "the boot slot carries the HAT overlay" \
	cmp -s "$repo/target/hat/tamanu-iti-v4.dtbo" "$fw/current/overlays/tamanu-iti-v4.dtbo"

undo

# A byte flipped in the payload fails a read of its block through the hash tree.
block=$((size / 4096 / 2))
offset=$(($(sfdisk --json "$image" | jq '.partitiontable.partitions[3].start') * 512 + block * 4096))
byte=$(od -An -tu1 -j "$offset" -N1 "$image" | tr -d ' ')
printf '%b' "\\x$(printf %02x $((byte ^ 1)))" | dd of="$image" bs=1 seek="$offset" conv=notrunc status=none
loop=$(losetup --partscan --read-only --show --find "$image")
teardown+=("losetup -d '$loop'")
udevadm settle
verity=bliti-provision-test-verity-$$
veritysetup open --hash-offset="${manifest[PAYLOAD_HASH_OFFSET]}" \
	"${loop}p4" "$verity" "${loop}p4" "${manifest[PAYLOAD_ROOT_HASH]}"
teardown+=("veritysetup close '$verity'")
check "a corrupt payload block fails its read" bash -c \
	"! dd if=/dev/mapper/$verity of=/dev/null bs=4096 skip=$block count=1 iflag=direct status=none 2>/dev/null"
check "an intact payload block reads" \
	dd if="/dev/mapper/$verity" of=/dev/null bs=4096 skip=$((block + 1)) count=1 iflag=direct status=none
undo

# A pi image that does not match its release's checksum fails the fetch.
fake=$(mktemp -d)
teardown+=("rm -rf '$fake'")
mkdir -p "$fake/served/0.0.0" "$fake/out"
echo 'not a pi image' >"$fake/served/0.0.0/fake.img.zst"
echo '{"files":[{"name":"fake.img.zst","variant":"pi","suite":"resolute","arch":"arm64","format":"img.zst"}]}' \
	>"$fake/served/0.0.0/manifest.json"
echo "$(printf '0%.0s' {1..64})  fake.img.zst" >"$fake/served/0.0.0/SHA256SUMS"
check "a pi image whose checksum does not match fails the fetch" bash -c \
	"! LINUX_IMAGES_URL=file://$fake/served LINUX_IMAGES_VERSION=0.0.0 '$repo/provision/fetch.sh' '$fake/out' 2>/dev/null"
check "a pi image whose checksum does not match is not kept" test ! -e "$fake/out/pi.img.zst"
undo

if ((failures)); then
	echo "$failures failed"
	exit 1
fi
echo "all passed"
