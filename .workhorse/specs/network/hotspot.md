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
| `interface` | string | no | the wireless interface whose radio runs the hotspot |
| `passphrase` | string | yes | what a client joins with |
| `share-upstream` | boolean | no | whether clients reach the device's own network; enabled where unset |
| `isolate-clients` | boolean | no | whether clients are kept from reaching each other; enabled where unset |
| `dhcp-range` | string | no | the subnet clients are addressed from |
| `band` | string | no | the band the hotspot operates on: `2ghz` for 2.4 GHz, `5ghz` or `6ghz` |
| `channel` | number | no | the channel it operates on |
| `channel-width` | number | no | the width of that channel, in megahertz |

A device MUST derive neither `ssid` nor `passphrase` from anything it holds, and MUST NOT supply a default for either.

`dhcp-range` MUST be one IPv4 subnet in CIDR notation.
A device MUST hold the subnet's first host address itself, and MUST hand out the rest to clients.
A device whose `dhcp-range` is unset MUST use `10.41.0.0/24`, the same range as every other bliti device.

> [!NOTE]
> The hotspot's passphrase is the one credential here meant to be read aloud and handed to a stranger, so it is kept clear of the chain every other value on the device descends from.
> A fixed default range is predictable and documentable. It is overridable because it collides with an upstream that happens to use it.

## Choosing a radio

A device MUST run a hotspot naming an `interface` on that interface's radio.

A device MUST run a hotspot naming no `interface` on a radio able to run it, and MUST prefer one carrying no wireless candidate, then one running an access point and a wireless client independently.

A device MUST NOT move a running hotspot to another radio unless a radio it prefers becomes available or its own is lost.

A device MUST run a hotspot whose `band`, `channel` or `channel-width` is set only on a radio offering them.

## Running alongside a wireless client

A device MUST report, for each radio able to run an access point, whether it runs one beside a wireless client at once, only one at a time, or at once only on one channel.

A wireless candidate could be carried by the radio whose interface it names, or by any radio where it names none.

A radio that runs only one at a time MUST NOT carry the hotspot and a wireless candidate together, and a device MUST treat as invalid a document whose hotspot and a wireless candidate could be carried only by such a radio.

For a radio that runs an access point and a wireless client only on one channel, a device MUST offer in that radio's capabilities the `band`, `channel` and `channel-width` values it can run a hotspot on with no wireless client beside it.

Such a radio MUST carry a hotspot setting `band`, `channel` or `channel-width` only where no wireless candidate in the document could be carried by that radio, and MUST then run the hotspot on the channel the document chooses.

A device MUST treat as invalid a document whose hotspot sets any of the three where every radio the hotspot could run on with them is such a radio and could carry a wireless candidate in the document, and MUST name in `at` the first of `band`, `channel` and `channel-width`, in that order, that the hotspot sets.

A device MUST operate a hotspot that sets none of the three on such a radio on the channel of the wireless client that radio carries, whenever one is associated.

A device MUST NOT start such a hotspot until the wireless client that radio carries has associated or has failed to.

A device MUST NOT run such a hotspot while that client is associated on a channel the regulatory domain lets no access point start on, and MUST treat a proposal bringing that about as failing at `hotspot`, with a reason naming the channel.

> [!NOTE]
> Holding the choice to what the document says rather than to whether a client happens to be associated is what keeps a setting that appears from being one that silently stops holding.
> The channel a hotspot is on is reported under [NFO](../device-info.md), so an operator who cannot choose it can still see it.
