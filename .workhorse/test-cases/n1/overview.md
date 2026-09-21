# Making the web client installable

Covers the installability [WEB](../../specs/web-app.md) requires, and how the mark survives the
platforms that redraw it. What a browser demands before it offers the install is the browser's own
affair rather than something the spec fixes, so most of these cases stand on their own. The manifest cases are automated in `crates/bliti-web/app/tests/installable.spec.js`;
the device cases are not automatable here, because whether a browser offers the install is the
browser's own decision on its own hardware.

## The manifest

- [x] The manifest is served and declares a name, a start URL and an installable display mode
- [x] The manifest declares a square PNG of at least 192px and one of at least 512px
- [x] The manifest declares a maskable icon
- [x] Every icon the manifest declares is served, and each PNG's real dimensions match the size it claims
- [x] The manifest states no preference for a related native application
- [x] The markup links the manifest, and a home-screen icon for platforms that ignore it
- [x] A service worker registers and serves the application's own assets

## On an Android phone

The device the client is actually used from, and the only place the criteria are known to be met
rather than merely declared.

- [ ] Chrome offers to install the application, over https, without the browser being coaxed by devtools (verifies spec: WEB)
- [ ] The installed application launches from its home-screen icon into its own window, with no browser chrome (verifies spec: WEB)
- [ ] The home-screen icon shows the flower, cropped to the launcher's shape without losing the mark
- [ ] The installed application still opens a device's QR link, and still reaches the device over Bluetooth
- [ ] With the phone offline, launching from the icon still loads the application and its icon (verifies spec: WEB)

## On an iPhone

Not the target device, but the path exists and the icon comes from the markup rather than the manifest.

- [ ] Add to Home Screen gives the flower rather than a screenshot or a black tile

## The mark

- [x] The flower reads at 192px and 96px with the chain of petals still distinct
- [ ] The flower still reads at 48px and as a favicon, where the chain is busiest and the seams
      between petals are close to vanishing
- [x] The maskable icon fills the circle a launcher crops to, without the outermost petals crossing it
- [ ] The icon is legible on a dark home-screen wallpaper and a light one
