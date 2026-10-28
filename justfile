# Recipes for working on bliti. `just setup` installs what the browser client's build needs; the
# rest are what CI runs, so the artefact a laptop produces is the artefact CI uploads.

web-app := justfile_directory() / "crates/bliti-web/app"

# The wasm-bindgen CLI and the wasm-bindgen crate are one pair: the bindings the CLI writes call
# into the runtime compiled into the module, so a mismatched pair fails when the module loads. Read
# it from the lockfile rather than stating the version anywhere it could drift from.
wasm_bindgen_version := `grep -A1 '^name = "wasm-bindgen"$' Cargo.lock | grep '^version' | cut -d'"' -f2`

[private]
default:
	@just --list

# Install the toolchain and dependencies the browser client's build needs.
setup:
	rustup target add wasm32-unknown-unknown
	cargo install wasm-bindgen-cli --version {{wasm_bindgen_version}} --locked
	cd {{web-app}} && npm ci

# The wasm-bindgen CLI version this tree builds against, for a CI step that installs a prebuilt one.
wasm-bindgen-version:
	@echo {{wasm_bindgen_version}}

# Build the wasm module and its bindings into the application's source tree.
build-wasm:
	#!/usr/bin/env bash
	set -euo pipefail
	# Vite picks the output up and bundles it like any other module. Everything protocol-shaped is
	# in there; the application around it only drives Web Bluetooth, the camera, and the interface.
	# Without these, rustc bakes the building account's home directory into the panic locations it
	# records, and the module carries them to every browser that loads it. Remapping also makes the
	# artefact the same on any machine. `trim-paths` would do this in the profile, but it is not
	# stabilised.
	remap="--remap-path-prefix=$(rustc --print sysroot)=/rust"
	remap="$remap --remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}/registry/src=/cargo"
	remap="$remap --remap-path-prefix=$PWD=/bliti"
	RUSTFLAGS="$remap" cargo build -p bliti-web --target wasm32-unknown-unknown --profile wasm
	wasm-bindgen --target web --no-typescript --out-dir {{web-app}}/src/wasm \
		target/wasm32-unknown-unknown/wasm/bliti_web.wasm

# Build the static bundle an origin serves.
build: build-wasm
	cd {{web-app}} && npm run build
	# Only the bundle an origin serves is precompressed. The harness builds its own, in test mode
	# and to its own directory, and nothing ever serves that one.
	cd {{web-app}} && node precompress.js

# Credentials are the AWS CLI's, found as it always finds them. The URL names the bucket and the
# prefix to write under, as s3://bes-bliti-web/previews/pr-7/, and --prune deletes what the new
# bundle no longer has; --dry-run lists what would be written and deleted, and writes nothing.
#
# Build the static bundle and upload it to an S3 prefix, the way CI does.
deploy url *flags: build
	cd {{web-app}} && node deploy.js {{url}} {{flags}}

# Builds in a container with the libraries the daemon links (crates/bliti/Containerfile), under
# podman or docker, whichever is present; CONTAINER_ENGINE picks one. The binary is
# target/device/aarch64-unknown-linux-gnu/release/bliti.
#
# Build the daemon for a 64-bit Arm device.
build-device:
	#!/usr/bin/env bash
	set -euo pipefail
	engine=${CONTAINER_ENGINE:-$(command -v podman || command -v docker || true)}
	if [[ -z $engine ]]; then
		echo "build-device needs podman or docker" >&2
		exit 1
	fi
	image=bliti-build-device
	"$engine" build -q -t "$image" -f crates/bliti/Containerfile crates/bliti >/dev/null
	# Run as the invoking user, so the build is theirs. Rootless podman maps the container's root to
	# them already, and keep-id keeps that mapping for their own uid.
	case $(basename "$engine") in
		podman) user=(--userns=keep-id) ;;
		*) user=(--user "$(id -u):$(id -g)") ;;
	esac
	# Its own target directory: the build scripts it compiles link the container's glibc, not the
	# host's, so they cannot share one with a host build.
	"$engine" run --rm "${user[@]}" \
		-v "$PWD:/bliti:z" -w /bliti \
		-v bliti-build-device-cargo:/cargo \
		-e BLITI_BUILD -e CARGO_TARGET_DIR=target/device \
		"$image" cargo build --release --locked --target aarch64-unknown-linux-gnu -p bliti

# Build the daemon, install it on a device as the one its service runs, and restart it.
install-device host: build-device
	ssh {{host}} 'cat >/tmp/bliti-new' <target/device/aarch64-unknown-linux-gnu/release/bliti
	ssh {{host}} 'set -e; sudo -n systemctl stop bliti; \
		sudo -n install -m755 /tmp/bliti-new /usr/local/bin/bliti; rm -f /tmp/bliti-new; \
		sudo -n systemctl start bliti'

hat-out := justfile_directory() / "target/hat"

# Compile the HAT's device tree overlay from hat/tamanu-iti-v4.dts. Needs dtc.
hat-overlay:
	mkdir -p {{hat-out}}
	dtc -@ -I dts -O dtb -o {{hat-out}}/tamanu-iti-v4.dtbo hat/tamanu-iti-v4.dts

# Build the revision B EEPROM image from hat/tamanu-iti-v4.0.txt. Needs eepmake, which comes in
# Raspberry Pi's raspi-utils-eeprom package.
hat-eeprom:
	mkdir -p {{hat-out}}
	eepmake hat/tamanu-iti-v4.0.txt {{hat-out}}/tamanu-iti-v4.0.eep

# Rasterise the application's icons from icon.svg, the one master beside them.
icons:
	#!/usr/bin/env bash
	set -euo pipefail
	# The PNG files are committed, because a build must not need a rasteriser; run this only when
	# the master changes. Chromium will not offer to install the application unless the manifest
	# carries a 192px and a 512px icon, so those two are what makes it installable at all rather
	# than merely manifested.
	cd {{web-app}}/public
	background='#cc3467'
	# Android crops a maskable icon to a circle 80% of the tile across. The master already sits
	# inside that circle, so the mark is rendered larger than the tile and the overflow cropped
	# away, which fills the circle rather than leaving the flower marooned in the middle of it.
	# Past about 112% the outermost petals cross the circle and lose their tips.
	maskable() {
		rsvg-convert -w "$(( $1 * 108 / 100 ))" icon.svg |
			magick - -background "$background" -gravity center -extent "${1}x${1}" \
				-alpha remove -alpha off -strip "PNG24:$2"
	}
	rsvg-convert -w 192 -h 192 icon.svg -o icon-192.png
	rsvg-convert -w 512 -h 512 icon.svg -o icon-512.png
	maskable 192 icon-maskable-192.png
	maskable 512 icon-maskable-512.png
	# iOS takes the home-screen icon from the markup rather than the manifest, and composites it
	# onto black wherever it is transparent, so this one is flattened.
	rsvg-convert -w 180 -h 180 -b "$background" icon.svg -o apple-touch-icon.png

# Size of the artefact the browser downloads, raw and as an origin would serve it.
size: build-wasm
	#!/usr/bin/env bash
	set -euo pipefail
	wasm={{web-app}}/src/wasm/bliti_web_bg.wasm
	printf '%-10s %9s\n' raw "$(stat -c%s "$wasm")"
	printf '%-10s %9s\n' gzip "$(gzip -9 -c "$wasm" | wc -c)"
	command -v brotli >/dev/null && printf '%-10s %9s\n' brotli "$(brotli -q 11 -c "$wasm" | wc -c)"

test:
	cargo test

# The application's own tests, which fake at the message layer with no wasm and no Bluetooth.
test-web: build-wasm
	cd {{web-app}} && npx playwright test

clippy:
	cargo clippy --all-targets --all-features

fmt:
	cargo fmt
