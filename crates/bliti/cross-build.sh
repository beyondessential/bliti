#!/bin/sh
# Cross-build the daemon for a Pi and, optionally, install it on one.
#
# The daemon links against dbus and the TPM stack, so cross-compiling needs those libraries for the
# target. Rather than asking a developer to find aarch64 packages for whatever distribution they run,
# this borrows them from the device itself: it downloads the distribution's own -dev packages there
# (without installing anything), copies what it needs, and builds against that as a sysroot. The
# sysroot lives under target/ and is disposable.
#
#   ./crates/bliti/cross-build.sh ubuntu@some-device          # build only
#   ./crates/bliti/cross-build.sh ubuntu@some-device --install # build, then install and restart
set -eu

HOST="${1:?usage: cross-build.sh user@host [--install]}"
INSTALL="${2:-}"
TARGET=aarch64-unknown-linux-gnu
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SYSROOT="$ROOT/target/aarch64-sysroot"
LIBDIR="$SYSROOT/usr/lib/aarch64-linux-gnu"

if [ ! -d "$LIBDIR/pkgconfig" ]; then
	echo "building a sysroot from $HOST"
	mkdir -p "$SYSROOT"
	ssh "$HOST" 'rm -rf /tmp/blitisys && mkdir -p /tmp/blitisys && cd /tmp/blitisys \
		&& sudo -n apt-get update -qq && apt-get download libdbus-1-dev libdbus-1-3 libtss2-dev >/dev/null'
	scp -q "$HOST:/tmp/blitisys/*.deb" "$SYSROOT/"
	ssh "$HOST" 'rm -rf /tmp/blitisys'

	(cd "$SYSROOT" && for deb in *.deb; do
		ar x "$deb" && tar xf data.tar.* && rm -f data.tar.* control.tar.* debian-binary
	done && rm -f *.deb)

	# The runtime sonames the -dev packages expect are already installed on the device, and its
	# symlinks point at versioned files, so read the resolved bytes rather than copying the links.
	for lib in libz.so.1 libsystemd.so.0 libcrypto.so.3 libssl.so.3 libdbus-1.so.3 \
		libtss2-sys.so.1 libtss2-esys.so.0 libtss2-mu.so.0 libtss2-tctildr.so.0 libtss2-rc.so.0; do
		ssh "$HOST" "cat /usr/lib/aarch64-linux-gnu/$lib" > "$LIBDIR/$lib" 2>/dev/null || continue
		ln -sf "$lib" "$LIBDIR/${lib%%.so.*}.so"
	done

	# Requires.private only matters when linking statically, and each entry would drag in another
	# package's .pc file, and that one's, and so on.
	sed -i '/^Requires.private:/d' "$LIBDIR"/pkgconfig/*.pc
fi

# --allow-shlib-undefined is what makes a partial sysroot workable: the transitive symbols of
# libsystemd and friends resolve on the device, which has the full set, rather than here.
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
PKG_CONFIG_ALLOW_CROSS=1 \
PKG_CONFIG_SYSROOT_DIR="$SYSROOT" \
PKG_CONFIG_LIBDIR="$LIBDIR/pkgconfig" \
RUSTFLAGS="-L $LIBDIR -C link-arg=-Wl,--allow-shlib-undefined" \
	cargo build -p bliti --release --target "$TARGET"

BINARY="$ROOT/target/$TARGET/release/bliti"
echo "built $BINARY"

[ "$INSTALL" = "--install" ] || exit 0

scp -q "$BINARY" "$HOST:/tmp/bliti-new"
ssh "$HOST" 'set -e
	sudo -n systemctl stop bliti
	sudo -n install -m755 /tmp/bliti-new /usr/local/bin/bliti
	rm -f /tmp/bliti-new
	sudo -n systemctl start bliti'
echo "installed on $HOST"
