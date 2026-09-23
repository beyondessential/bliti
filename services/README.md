# Device-side requirements

What a bliti device needs beyond the `bliti` binary: the systemd unit, the
bluetoothd configuration BLI-CHN's peripheral-only requirement calls for, and
for the network configuration module the hostapd unit bliti drives and the iwd
drop-in keeping iwd off the hotspot's interface. Each file in this directory
says where it installs and why it exists.

This file records the packages a device needs at runtime. Nothing installs them
yet, and deployment is by hand. It is written down so that a Debian package,
when there is one, can declare them as `Depends` instead of rediscovering them
one failure at a time.

## Packages bliti needs

### Today

- **`bluez`** for bluetoothd, which the GATT server runs on. It has to be
  peripheral-only, which is what `bliti-bluetoothd.conf` and
  `bliti-bluetooth-dropin.conf` beside this file are for.

### For the network configuration module

- **`hostapd`** for the hotspot. Nothing else here puts a radio into AP mode.
- **`wireless-regdb`** for the kernel's regulatory database, which is what makes
  the regulatory domain setting mean anything. Without it a device is held to
  the world domain and never gets the local channel set, which is the restricted
  behaviour an unset domain is specified to fall back to rather than something
  to rely on. `crda` is obsolete on any kernel this targets and is not wanted:
  the kernel reads `/lib/firmware/regulatory.db` itself.
- **`iw`** for the settings bliti makes on the radio at runtime: the regulatory
  domain (`iw reg set`), since the modprobe file only takes effect when cfg80211
  loads, and creating and deleting the hotspot's access point interface. bliti
  runs it rather than speaking nl80211 itself, until a netlink crate is chosen.
  It is a dependency for as long as that holds.
- **`iwd`** for the wireless client, with `bliti-iwd-dropin.conf` beside this
  file keeping it off the hotspot's interface.
- **`systemd-resolved`**, at systemd 258 or later, for per-link resolvers. A
  candidate's own resolvers are written as a DNS delegate
  (`/etc/systemd/dns-delegate.d`), which is what keeps the site's own names
  going to the site's resolvers, and delegates first appear in 258.

The wireless client, wired addressing and the resolvers are all supplied by
whichever backend gets chosen, so their packages are listed under that choice
below rather than here.

A hotspot needs no separate DHCP server on the systemd-networkd route, because
networkd carries its own (`DHCPServer=`). On another backend it may, and
`dnsmasq-base` is the usual answer.

## The backend choice decides the rest

The working doc leaves open what configures the network underneath. The
dependency list forks on that answer, and this is one input to it:

| choice | package | in the prototype image |
| --- | --- | --- |
| systemd-networkd with wpa_supplicant | `wpasupplicant`, plus `systemd` for networkd | already there |
| NetworkManager | `network-manager` | not installed, and never has been |
| iwd with systemd-networkd | `iwd` | not installed, and never has been |

The image already carries everything the first option wants, and neither of the
other two has ever been on this board. That is not on its own a reason to pick
the first, since the backends differ in what they make easy, but it is a real
cost against the other two and it was not obvious before someone looked.

## What the image already ships

Observed on the `tamanu-iti-v4-prototype` board in September 2026, a Raspberry
Pi 5 on Ubuntu 26.04 with the Cypress CYW43455 radio. Evidence about one image
at one moment, not a guarantee: a package a deb needs is declared whether or not
an image is believed to carry it.

| package | version | note |
| --- | --- | --- |
| `bluez` | 5.85 | |
| `wpasupplicant` | 2:2.11 | running, idle |
| `iw` | 6.17 | |
| `wireless-regdb` | 2026.05.30 | domain was `country 00`, the world default |
| `netplan.io` | 1.2 | configures ethernet only |
| `systemd` | 259.5 | networkd and resolved both active |
| `hostapd` | 2:2.11 | installed by hand for the AP mode spike |

`hostapd` was the only one missing. It is installed on that board now, and
masked so it does not start on its own, since the network module is meant to
drive it rather than have it come up from a unit file of its own.

The daemon runs `iw` for now, which is why it is listed among the packages
above; it stops being a dependency if bliti comes to speak nl80211 itself.
