# Hotspot tile shows its QR code

## Escaping in the Wi-Fi URI

The WPA3 Specification (section 7) percent-encodes octets outside its printable set, `;` among them.
The older ZXing form, which many phone camera parsers were written against, backslash-escapes `\ ; , " :` instead.
An SSID or passphrase with none of those characters encodes identically either way, so this only bites on unusual values.
Before settling the encoder, scan a code carrying a `;` in its passphrase with a current iOS and Android camera, and choose the form both join with.

The hotspot runs WPA2/WPA3 transitional (`render/hostapd.rs`), so the type is `T:WPA` with no `R:`.
