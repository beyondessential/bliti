---
id: HOT
---

# Hotspot

A device runs a wireless access point that clients join directly, described by the `hotspot` member of the document of [NET](overview.md).

## When it runs

A device MUST run a hotspot only where its configuration carries one.

> [!NOTE]
> A device out of the box is reached over the channel of [CHN](../channel.md), which is what the QR code is for.

## What it carries

| member | type | required | meaning |
| --- | --- | --- | --- |
| `ssid` | string | yes | the network the hotspot advertises |
| `passphrase` | string | yes | what a client joins with |
| `share-upstream` | boolean | no | whether clients reach the device's own network; enabled where unset |
| `isolate-clients` | boolean | no | whether clients are kept from reaching each other; enabled where unset |
| `dhcp-range` | string | no | the addresses handed to clients |
| `band` | string | no | the band the hotspot operates on |
| `channel` | number | no | the channel it operates on |
| `channel-width` | number | no | the width of that channel |

A device MUST derive neither `ssid` nor `passphrase` from anything it holds, and MUST NOT supply a default for either.

A device whose `dhcp-range` is unset MUST use the same range as every other bliti device.

> [!NOTE]
> The hotspot's passphrase is the one credential here meant to be read aloud and handed to a stranger, so it is kept clear of the chain every other value on the device descends from.
> A fixed default range is predictable and documentable. It is overridable because it collides with an upstream that happens to use it.

## Running alongside a wireless client

A device whose radio can run an access point and a wireless client at once MUST report that among its capabilities.

A device whose radio can run only one at a time MUST report that instead, and MUST treat a document carrying both a hotspot and a wireless candidate as invalid.

A device whose radio runs an access point and a wireless client only on one channel MUST report that among its capabilities, MUST omit `band`, `channel` and `channel-width` from its capabilities, and MUST operate its hotspot on the channel its wireless client is using whenever one is associated.

> [!NOTE]
> Reporting the constraint rather than accepting a channel and overriding it is what keeps a setting that appears from being one that silently stops holding, in this case depending on whether a client happened to associate.
> The channel a hotspot is on is reported under [NFO](../device-info.md), so an operator who cannot choose it can still see it.
