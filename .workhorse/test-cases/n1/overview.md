# Making the web client installable

Covers the install criteria of [WEB](../../specs/web-app.md) and how the mark survives the platforms
that redraw it. The manifest cases are automated in `crates/bliti-web/app/tests/installable.spec.js`;
the device cases are not automatable here, because whether a browser offers the install is the
browser's own decision on its own hardware.

## The manifest

- [x] The manifest is served and declares a name, a start URL and an installable display mode (verifies spec: WEB)
- [x] The manifest declares a square PNG of at least 192px and one of at least 512px (verifies spec: WEB)
- [x] The manifest declares a maskable icon (verifies spec: WEB)
- [x] Every icon the manifest declares is served, and each PNG's real dimensions match the size it claims (verifies spec: WEB)
- [x] The manifest states no preference for a related native application (verifies spec: WEB)
- [x] The markup links the manifest, and a home-screen icon for platforms that ignore it (verifies spec: WEB)
- [x] A service worker registers and serves the application's own assets (verifies spec: WEB)

## On an Android phone

The device the client is actually used from, and the only place the criteria are known to be met
rather than merely declared.

- [ ] Chrome offers to install the application, over https, without the browser being coaxed by devtools (verifies spec: WEB)
- [ ] The installed application launches from its home-screen icon into its own window, with no browser chrome (verifies spec: WEB)
- [ ] The home-screen icon shows the flower, cropped to the launcher's shape without losing the mark (verifies spec: WEB)
- [ ] The installed application still opens a device's QR link, and still reaches the device over Bluetooth
- [ ] With the phone offline, launching from the icon still loads the application and its icon (verifies spec: WEB)

## On an iPhone

Not the target device, but the path exists and the icon comes from the markup rather than the manifest.

- [ ] Add to Home Screen gives the flower rather than a screenshot or a black tile (verifies spec: WEB)

## The mark

- [x] The flower reads at 192px, 96px and 48px without the petals merging
- [x] The maskable icon keeps the flower inside the circle a launcher crops to (verifies spec: WEB)
- [ ] The icon is legible on a dark home-screen wallpaper and a light one
