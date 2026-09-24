---
id: WLAN
---

# Wireless networks

A `wireless` candidate of [LINK](attachment.md) names a network to join and carries what is needed to join it.

## What a candidate carries

| member | type | required | meaning |
| --- | --- | --- | --- |
| `ssid` | string | yes | the network to join |
| `security` | object | yes | how the device authenticates to it, as below |
| `hidden` | boolean | no | whether the network is joined without it appearing in a scan |
| `interface` | string | no | the wireless interface it is joined on; unset, the device chooses, as [LINK](attachment.md) specifies |
| `bands` | array | no | the bands it may be joined on, each named as [HOT](hotspot.md) names bands; unset, any |

`security` MUST carry a `kind` of `psk`, `sae`, `psk-sae` or `enterprise`.

`psk`, `sae` and `psk-sae` MUST carry a `passphrase`.

`enterprise` MUST carry an `eap` of `peap`, `ttls`, `tls` or `pwd`, and the members that method uses:

| member | used by | meaning |
| --- | --- | --- |
| `identity` | every method | who authenticates; the inner identity under `peap` and `ttls` |
| `anonymous-identity` | `peap`, `ttls` | the outer identity, where it differs |
| `password` | `peap`, `ttls`, `pwd` | the password |
| `phase2` | `peap`, `ttls` | the inner method |
| `ca-certificate` | `peap`, `ttls`, `tls` | the certificate authority the server's certificate must chain to, as PEM |
| `domain` | `peap`, `ttls`, `tls` | the name the server's certificate must carry |
| `client-certificate` | `tls` | the device's certificate, as PEM |
| `client-key` | `tls` | the device's private key, as PEM |
| `client-key-passphrase` | `tls` | what decrypts the key, where it is encrypted |

Every member a method uses is required, except `anonymous-identity` and `client-key-passphrase`.

A device MUST treat an enterprise candidate carrying a member its method does not use as invalid.

## Choosing bands

`bands` MUST name at least one band, and MUST NOT name one twice.

A device SHOULD offer `bands` on every radio able to hold a wireless connection to the bands chosen for it.

A device MUST join a candidate carrying `bands` only on a band it names, and MUST treat its network as out of range where no access point of it is heard on one.

> [!NOTE]
> A network on several access points can be heard on more than one band, and which to use is the operator's call as much as the device's: 2.4 GHz reaches further, 5 GHz carries more, and a hotspot sharing the radio has to share its band.
> The usual choice is a band to stay off, such as a crowded 2.4 GHz, more than a single band to hold to, which is why a candidate names a set.
> A device that cannot hold a connection to chosen bands offers no `bands`, and a client offers only what it is offered, as [NET](overview.md) requires. That is the case on iwd, which bliti's own device runs as its wireless client: iwd has no per-network band, and restricts bands only for every network on every radio at once, which stops it scanning the others too. A device on iwd therefore does not meet the SHOULD. A backend that can, or a per-network band in iwd, brings `bands` in without a change here.

## Which networks a device joins

A device MUST join only a network that authenticates the access point to it.

A device MUST join a `sae` candidate only by SAE, and MUST fail one that joined by anything else at `association`.

A device MUST offer `sae` only on a radio able to hold a connection to SAE.

> [!NOTE]
> WPA2-PSK and WPA3-SAE authenticate by proving possession of the key, and 802.1X by certificate.
> The requirement is not that traffic be encrypted. A link encrypted without authentication leaves an adversary free to impersonate the access point and to inspect, modify and forge everything crossing it, and the traffic at risk is the application's: the overlay of [NFO](../device-info.md) carries management rather than what a device is deployed to do.

## Joining by WPS

A device MUST join by WPS on request, by push-button and by PIN.

A device MUST report which WPS methods each of its radios offers among its capabilities.

A device asked to join by WPS naming no `interface` MUST join on a radio offering the method asked for.

A device asked to join by WPS for a named network MUST add only that network, and MUST NOT keep credentials the exchange yields for any other, nor stay joined to it.

> [!NOTE]
> A device with no keyboard is one an operator cannot type a passphrase into, and WPS is what a site's existing access point already offers for that.
> Which mechanisms a site runs is the site's to decide. A device that refused one because the mechanism is weak would be imposing a judgement on a network it is a guest of, and would leave the operator holding a device that will not join.

## Where the device is the access point

A device MUST NOT offer WPS to clients of its own hotspot.

> [!NOTE]
> Joining by what a site offers is working with what exists. Offering an onboarding mechanism of our own is a choice about what to put into the world, and that one is ours to decline.
