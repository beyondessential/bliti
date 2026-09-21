#!/bin/sh
# Rasterise the application's icons from icon.svg, the one master beside them. The PNG files are
# committed, because a build must not need a rasteriser; run this only when the master changes.
#
# Chromium will not offer to install the application unless the manifest carries a 192px and a 512px
# icon, so those two are what makes it installable at all rather than merely manifested.
set -eu
cd "$(dirname "$0")/app/public"

BACKGROUND='#cc3467'

# Android crops a maskable icon to a circle 80% of the tile across. The master already sits inside
# that circle, so the mark is rendered larger than the tile and the overflow cropped away, which
# fills the circle rather than leaving the flower marooned in the middle of it. Past about 112% the
# outermost petals cross the circle and lose their tips, so this leaves a margin under that.
maskable() {
	rsvg-convert -w "$((${1} * 108 / 100))" icon.svg |
		magick - -background "$BACKGROUND" -gravity center -extent "${1}x${1}" \
			-alpha remove -alpha off "PNG24:$2"
}

rsvg-convert -w 192 -h 192 icon.svg -o icon-192.png
rsvg-convert -w 512 -h 512 icon.svg -o icon-512.png
maskable 192 icon-maskable-192.png
maskable 512 icon-maskable-512.png
# iOS takes the home-screen icon from the markup rather than the manifest, and composites it onto
# black wherever it is transparent, so this one is flattened.
rsvg-convert -w 180 -h 180 -b "$BACKGROUND" icon.svg -o apple-touch-icon.png
