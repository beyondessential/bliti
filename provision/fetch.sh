#!/usr/bin/env bash
# Fetch what the provisioning image is assembled from into <dir> (PMED, "Building"):
#
#   pi.img.zst          the newest released `pi` image for resolute, checked against its SHA256SUMS
#   pi-version          the linux-images release it came from
#   pieeprom.bin        the newest Pi 5 bootloader on Raspberry Pi's default channel
#   bootloader-version  that bootloader's date
#
# A pi.img.zst already in <dir> that matches the release's checksum is kept rather than downloaded
# again. LINUX_IMAGES_VERSION names a release rather than finding the newest, and LINUX_IMAGES_URL
# where releases are served from; both are for testing this script, and a published image sets
# neither.
set -euo pipefail

dir=${1:?usage: fetch.sh <dir>}
mkdir -p "$dir"

releases=https://api.github.com/repos/beyondessential/linux-images/releases
served=${LINUX_IMAGES_URL:-https://tools.ops.tamanu.io/linux-images}
eeprom=https://api.github.com/repos/raspberrypi/rpi-eeprom/contents/firmware-2712/default

# The GitHub API allows few unauthenticated requests from one address, which a shared CI runner
# runs out of.
github=(-H 'Accept: application/vnd.github+json')
if [[ -n ${GITHUB_TOKEN:-} ]]; then
	github+=(-H "Authorization: Bearer $GITHUB_TOKEN")
fi

get() {
	curl --proto '=https,file' --tlsv1.2 -fsSL --retry 3 "$@"
}

# The entry for the resolute arm64 `pi` image in a release's manifest.json, empty where it has none.
pi_entry() {
	get "$served/$1/manifest.json" 2>/dev/null |
		jq -c '.files[]? | select(.variant == "pi" and .suite == "resolute" and .arch == "arm64"
			and .format == "img.zst")' | head -n1
}

if [[ -n ${LINUX_IMAGES_VERSION:-} ]]; then
	version=$LINUX_IMAGES_VERSION
	entry=$(pi_entry "$version")
else
	# Newest first, as the API lists them. Drafts and pre-releases are skipped (PMED).
	version='' entry=''
	for tag in $(get "${github[@]}" "$releases?per_page=30" |
		jq -r '.[] | select(.draft | not) | select(.prerelease | not) | .tag_name'); do
		entry=$(pi_entry "${tag#v}")
		if [[ -n $entry ]]; then
			version=${tag#v}
			break
		fi
	done
fi
if [[ -z $entry ]]; then
	echo "no linux-images release${version:+ $version} publishes a resolute pi image" >&2
	exit 1
fi

name=$(jq -r .name <<<"$entry")
# The release's own SHA256SUMS, rather than the manifest that pointed at the file.
sum=$(get "$served/$version/SHA256SUMS" | awk -v name="$name" '$2 == name { print $1 }')
if [[ ! $sum =~ ^[0-9a-f]{64}$ ]]; then
	echo "SHA256SUMS of linux-images $version has no checksum for $name" >&2
	exit 1
fi

check() {
	echo "$sum  $dir/pi.img.zst" | sha256sum -c --status
}

if [[ -f $dir/pi.img.zst ]] && check; then
	echo "pi image $name from linux-images $version already fetched"
else
	echo "fetching $name from linux-images $version"
	get -o "$dir/pi.img.zst.part" "$served/$version/$name"
	mv "$dir/pi.img.zst.part" "$dir/pi.img.zst"
	if ! check; then
		rm -f "$dir/pi.img.zst"
		echo "$name does not match the checksum linux-images $version publishes" >&2
		exit 1
	fi
fi
echo "$version" >"$dir/pi-version"

# Raspberry Pi names each bootloader by its date, so the newest is the greatest name.
bootloader=$(get "${github[@]}" "$eeprom" |
	jq -r '[.[] | select(.name | test("^pieeprom-[0-9]{4}-[0-9]{2}-[0-9]{2}\\.bin$"))]
		| sort_by(.name) | last | "\(.name) \(.download_url)"')
read -r file url <<<"$bootloader"
if [[ $file == null || -z $url ]]; then
	echo "no bootloader on rpi-eeprom's default channel" >&2
	exit 1
fi
echo "fetching bootloader $file"
get -o "$dir/pieeprom.bin" "$url"
date=${file#pieeprom-}
echo "${date%.bin}" >"$dir/bootloader-version"
