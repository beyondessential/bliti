# Hotspot tile shows its QR code

## The Wi-Fi URI

The code carries the ZXing form of the Wi-Fi URI, `WIFI:T:WPA;S:<ssid>;P:<passphrase>;;`, with `\ ; , : "` backslash-escaped.
Android's own "share Wi-Fi" generator (`WifiNetworkConfig.getQrCode` in the Settings app) escapes the same way, and phone cameras parse against it; the WPA3 Specification's percent-encoding is the form they were not written against.
Values are not quoted, even where ZXing suggests quoting one that looks like hexadecimal: Android's parser (`WifiUriParser.addQuotation`, under its newer escape-handling flag) wraps every value in quotes regardless, so a quoted SSID would join a network whose name includes the quotes.
The hotspot runs WPA2/WPA3 transitional (`render/hostapd.rs`), so the type is `T:WPA` with no `R:`.

The encoder lives in `bliti-core` beside the device's own QR code, so it is unit-tested natively with the same scanner, and reaches the browser through `bliti-web`.
The SVG draws its dark modules in `currentColor` over nothing, so the theme colours both the code and its quiet zone.

## Build

- [x] VIEW names the ZXing Wi-Fi URI and its escaping; the overview's external documents cite ZXing's barcode contents page in place of the WPA3 Specification
- [x] Device: the hotspot report holds the passphrase, and the `hotspot` fact carries it as the `passphrase` trait (NFO)
- [x] Core: the Wi-Fi URI encoder and its SVG, with tests that scan the SVG back and cover escaping
- [x] Web: wasm export for the hotspot code
- [x] Web: the hotspot tile's reveal shows the passphrase and the code, once the protocol module has loaded (VIEW)
  - [x] `passphrase` is descriptive in the tile keying, so a new passphrase replaces the tile
- [x] Playwright coverage for the reveal
- [x] `cargo fmt`, clippy, `cargo test`, `just build`, Playwright
