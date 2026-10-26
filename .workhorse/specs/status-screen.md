---
id: STAT
---

# Status screen

The Status screen is the screen an operator meets when a channel to a device opens: whether the device is on a network, whether Tailscale and Tamanu are running, and the way into everything else.

[VIEW](device-view.md) renders the whole catalogue of [NFO](device-info.md).
This screen renders the few entries that answer "is it working", and sends an operator to [VIEW](device-view.md) and the other screens under Advanced for the rest.

## The landing

The application MUST show the Status screen when a channel to a device opens, and MUST return the operator to it from every screen reached from it.

The application MUST title the Status screen with the device's `hostname`, falling back to the last four characters of the encoded payload of [QR](qr-code.md) where the device reports none.

The application MUST carry beneath the title the device's software identity, from `board` with `board-revision`, `os` and `kernel`, as [VIEW](device-view.md) draws its header.

The application MUST offer to disconnect beside the title.

The application MUST show beneath the title each message from the device it could not act on.

> [!NOTE]
> The question an operator opens the application to answer is "is this working, and if not, what is wrong". The Status screen answers it before anything is tapped.

## The rows

The application MUST show, beneath the title, a row for the network, a row for Tailscale and a row for Tamanu, in that order.

The application MUST read the rows from the batches of [NFO](device-info.md#batches), changing them only once a batch has ended, by everything in it.

Until the first batch has ended, the application MUST show the network row and the Tailscale row in progress, saying it is checking each.

Each row MUST carry a mark, a name saying how the thing stands, and beneath the name, where there is any, its detail.

A row's mark MUST be one of:

| mark | meaning |
| --- | --- |
| well | the thing is working |
| warning | the thing is working, with a problem |
| not well | the thing is not working |
| in progress | the thing is changing state, or its state is not yet known |

The application MUST give every row's detail as short labelled values, and MUST leave whole sentences to the device's `reason`.

The application MUST render a `reason` as the device wrote it.

The application MUST supply its own wording for every state it recognises.

The application MUST render a state it does not recognise by the mark its `status` gives, `passed` as well, `warning` as warning and any other as not well, with the state's value in its own wording and the device's `reason`.

## The network row

The application MUST count a way as in use as follows:

| way | in use where |
| --- | --- |
| cable | a wired interface's `wired-carrier` is `true` and that interface holds an address |
| wifi | an interface is joined to a network by `wireless-network` and that interface holds an address |
| hotspot | `hotspot` carries a value |

An interface holds an address where `network-address` reports for it an IPv4 or IPv6 address that is not link-local.
An interface that meets the cable's condition or wifi's is in use for that way.

The application MUST show the network row well, saying the device is connected, while any way is in use, and not well, saying it is not connected, otherwise.

The application MUST show as the network row's detail, while it is well, each way in use, by its icon and its short name, in this order:

- one interface in use for the cable or for wifi, and for wifi the name of the network it is joined to;
- then the hotspot, where it is in use, with how many clients are joined from `hotspot-clients`.

The application MUST choose that interface as the one whose `interface` trait carries `route` as `default`, and where none in use for the cable or for wifi does, the first in use for the cable by interface name, and otherwise the first in use for wifi by interface name.

While the network row is not well, the application MUST say as its detail that a cable is plugged in with no address, where `wired-carrier` is `true` on an interface that holds none, and that the device is joined to a network with no address, naming it, where an interface joined by `wireless-network` holds none.

The application MUST offer on the network row to change the network while the row is well, and to set it up while it is not.
Either MUST open the screen of [NSCR](network/screen.md).

> [!NOTE]
> A hotspot with nothing else is a way in use, so a device sharing its hotspot alone reads as connected: phones on the hotspot reach it.
> Requiring an address is what tells a cable into a dead switch, or a network whose DHCP never answered, from one that works.
> The default route is the way the device's own traffic leaves by, so it names the uplink where cable and wifi are both up. The fixed order covers a default route on the overlay or on no way at all.

## The Tailscale row

Once the first batch has ended, the application MUST show the Tailscale row only where the device reports `tailscale-state`.

The application MUST render the Tailscale row from `tailscale-state` by this table, showing the device's `reason` beneath the name where the table says so:

| `tailscale-state` | mark | says | `reason` |
| --- | --- | --- | --- |
| `running`, `passed` | well | connected | no |
| `running`, `warning` | warning | connected | yes |
| `starting` | in progress | connecting | no |
| `stopped` | not well | turned off | no |
| `needs-login` | not well | not signed in | no |
| `expired` | not well | sign-in expired | no |
| `needs-approval` | not well | waiting for approval | no |
| `broken` | not well | state unknown | yes |

The application MUST show as the row's detail the name the device is reached by, from `tailscale-name`, labelled as the device name, wherever `tailscale-name` carries a value.

Where `tailscale-key-expiry` is `warning`, the application MUST show the row's mark as warning where it would otherwise be well, and MUST show in the row's detail when the sign-in lapses, as a time from now.

> [!NOTE]
> A sign-in that lapses takes a device out of reach of the fleet, and the Status screen is where an operator who can renew it will see the warning.

## The Tamanu row

The application MUST show the Tamanu row in progress, saying it is checking Tamanu, while the device reports no `tamanu-state`.

The application MUST otherwise render the Tamanu row from `tamanu-state` by this table, showing the device's `reason` beneath the name where the table says so:

| `tamanu-state` | mark | says | `reason` |
| --- | --- | --- | --- |
| `running` | well | running | no |
| `degraded` | warning | partly running | yes |
| `stopped` | not well | stopped | yes |
| `absent` | not well | not installed | no |
| `starting` | in progress | starting | no |
| `restarting` | in progress | restarting | no |
| `broken` | not well | state unknown | yes |

The application MUST show as the row's detail the version of each of Tamanu's web application and facility server, from `tamanu-web-version` and `tamanu-facility-server-version`, each labelled and only where it carries a value.

> [!NOTE]
> A device reports none of the Tamanu facts until its first look at Tamanu has ended, as [NFO](device-info.md) has it, so the row says it is checking rather than leaving a gap.

## Advanced

The application MUST gather, beneath the rows, under a disclosure headed Advanced, the ways to the full readings of [VIEW](device-view.md), the network settings of [NSCR](network/screen.md), and the power and battery controls of [CSCR](control/screen.md).

The application MUST show the disclosure closed when a channel opens, and MUST keep it open or closed as the operator left it while the channel stays open.

> [!NOTE]
> A facility operator never needs the disclosure and never meets it by accident; a field technician finds the full depth under Advanced, where they look for it.

## While an act is under way

The application MUST show on the Status screen a device carrying out an act, as [WEB](web-app.md) specifies, in place of the rows and Advanced, for as long as the act is under way.
