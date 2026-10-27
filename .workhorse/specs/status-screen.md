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

The application MUST carry beneath the title the device's identity: the value of `product` where the device reports one with a value, and `board` otherwise, followed by `os`.

The application MUST show `board` and `os` there each as its `short` trait of [NFO](device-info.md#identity) has it, and as its value where it carries none.

The application MUST offer to disconnect beside the title.

Where the device sends something this version is too old to act on, as [MSG](messages.md#a-critical-member-the-receiver-does-not-know) has it, the application MUST say so beneath the title, once for each message type however often it arrives, for as long as the channel stays open.

> [!NOTE]
> The question an operator opens the application to answer is "is this working, and if not, what is wrong". The Status screen answers it before anything is tapped.

## The rows

The application MUST show, beneath the title, a row for the network, a row for Tailscale and a row for Tamanu, in that order.

The application MUST read the rows from the batches of [NFO](device-info.md#batches), changing them only once a batch has ended, by everything in it.

Until the first batch has ended, the application MUST show the network row and the Tailscale row in progress, saying it is checking each.

Each row MUST carry a mark, the name of what it is about, what it says of how that stands, beside them its act, where it has one, and beneath them, where there is any, its detail.

A row's mark MUST be one of:

| mark | meaning |
| --- | --- |
| well | the thing is working |
| warning | the thing is working, with a problem |
| not well | the thing is not working |
| in progress | the thing is changing state, or its state is not yet known |

The application MUST keep a row's detail to short values and phrases, and MUST leave whole sentences to the device's `reason`.

The application MUST render a `reason` as the device wrote it.

The application MUST supply its own wording for every state it recognises.

The application MUST render a state it does not recognise by the mark its `status` gives, `passed` as well, `warning` as warning and any other as not well, saying the state's value as the device sent it, with the device's `reason`.

## Acts on the rows

The application MUST open a Tailscale stream of [TSC](control/tailscale.md) and a Tamanu stream of [TMU](control/tamanu.md) when it shows the Status screen, and MUST close both when the operator leaves it.

The application MUST offer on the Tailscale row and the Tamanu row only an act the device lists on that row's stream, as the sections below have it, and MUST leave the row without an act until the device has listed its acts.

Where an act is to be confirmed, the application MUST ask the operator to confirm it, each time, before asking the device for it, showing the confirmation beneath the row in place of its act and leaving the other rows in view.

A confirmation MUST name the act in the words of the act's button.

The application MUST NOT offer a row's act again from asking the device for it until the device refuses it, sends its result or its failure, or lists its acts anew.

The application MUST show in the row's detail the reason of a `cannot-act`, or of a `tailscale-failed` or `tamanu-failed`, as the device wrote it, until the operator asks for an act on that row again or leaves the Status screen.

> [!NOTE]
> A device older than the application skips `tailscale` and `tamanu` as it skips anything else it does not recognise, and never answers, so its rows read as they would with nothing to offer.
> An act that changes Tailscale or Tamanu shows as the row's state changing on the `default` feed, which is why the rows need nothing more than the feed to show it under way or done.

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
- then the hotspot, where it is in use, with how many clients are joined from `hotspot-clients` where it carries a value.

The application MUST choose that interface as the one whose `interface` trait carries `route` as `default`, and where none in use for the cable or for wifi does, the first in use for the cable by interface name, and otherwise the first in use for wifi by interface name.

While the network row is not well, the application MUST say as its detail that a cable is plugged in with no address, where `wired-carrier` is `true` on an interface that holds none, and that the device is joined to a network with no address, naming it, where an interface joined by `wireless-network` holds none.

The application MUST offer on the network row to change the network while the row is well, and to set it up while it is not.
Either MUST open the Network screen of [NSET](network/setup.md), through the configuration session [NSET](network/setup.md#the-session) opens.

> [!NOTE]
> A hotspot with nothing else is a way in use, so a device sharing its hotspot alone reads as connected: phones on the hotspot reach it.
> Requiring an address is what tells a cable into a dead switch, or a network whose DHCP never answered, from one that works.
> The default route is the way the device's own traffic leaves by, so it names the uplink where cable and wifi are both up. The fixed order covers a default route on the overlay or on no way at all.

## The Tailscale row

Once the first batch has ended, the application MUST show the Tailscale row only where the device reports `tailscale-state`.

The application MUST render the Tailscale row from `tailscale-state` by this table, showing the device's `reason` beneath what the row says where the table calls for it:

| `tailscale-state` | mark | says | `reason` |
| --- | --- | --- | --- |
| `running`, `passed` | well | connected | no |
| `running`, `warning` | warning | connected | yes |
| `starting` | in progress | connecting | no |
| `stopped` | not well | turned off | no |
| `needs-login` | not well | not signed in | no |
| `expired` | not well | sign-in expired | no |
| `needs-approval` | not well | waiting for approval | no |
| no value, `broken` | not well | state unknown | yes |

The application MUST show as the row's detail the name the device is reached by, from `tailscale-name`, labelled as the device name, wherever `tailscale-name` carries a value.

Where `tailscale-key-expiry` is `warning`, the application MUST show the row's mark as warning where it would otherwise be well, and MUST show in the row's detail when the sign-in lapses, as a time from now.

The application MUST offer on the Tailscale row, where the device lists the act, the act this table gives for `tailscale-state`, and no act in any other state:

| `tailscale-state` | act | button | confirmed |
| --- | --- | --- | --- |
| `needs-login`, `expired` | `sign-in` | Connect | no |
| `stopped` | `turn-on` | Turn on | no |
| `running`, `broken` | `netcheck` | Network check | no |
| `needs-approval` | `log-out` | Cancel | yes |

Connect MUST open the sign-in screen of [TSCR](control/tailscale-screen.md#signing-in).

The confirmation of Cancel MUST ask whether to cancel the sign-in, and MUST offer to keep waiting as the preferred choice.

While a network check is under way, the application MUST show the row's act as under way, saying it is checking.

Once the device sends `tailscale-result` for a network check, the application MUST show in the row's detail one of:

- that the device can connect, where any relay answered;
- that the device is blocked, where none did, with why: that the network asks for a sign-in where `captive-portal` is true, that there is no internet where `ipv4` and `ipv6` are both false, and otherwise that Tailscale cannot be reached from this network.

The application MUST offer beside it the way to the full result on the Tailscale screen of [TSCR](control/tailscale-screen.md#network-check), and MUST keep it in the row's detail until the operator runs the check again or leaves the Status screen.

> [!NOTE]
> A relay is how most devices reach their tailnet, and Tailscale works through one, so a device that reaches only relays can connect.
> A network check tests the network beneath Tailscale, which is what an operator with a device that is on but not reachable needs to know first.

> [!NOTE]
> A sign-in that lapses takes a device out of reach of the fleet, and the Status screen is where an operator who can renew it will see the warning.

## The Tamanu row

The application MUST show the Tamanu row in progress, saying it is checking Tamanu, while the device reports no `tamanu-state`.

The application MUST otherwise render the Tamanu row from `tamanu-state` by this table, showing the device's `reason` beneath what the row says where the table calls for it:

| `tamanu-state` | mark | says | `reason` |
| --- | --- | --- | --- |
| `running` | well | running | no |
| `degraded` | warning | partly running | yes |
| `stopped` | not well | stopped | yes |
| `absent` | not well | not installed | no |
| `starting` | in progress | starting | no |
| `restarting` | in progress | restarting | no |
| no value, `broken` | not well | state unknown | yes |

The application MUST show as the row's detail the version of each of Tamanu's web application and facility server, from `tamanu-web-version` and `tamanu-facility-server-version`, each labelled and only where it carries a value.

The application MUST offer on the Tamanu row, where the device lists the act, the act this table gives for `tamanu-state`, and no act in any other state:

| `tamanu-state` | act | button | confirmed |
| --- | --- | --- | --- |
| `stopped` | `start` | Start | yes |
| `running`, `degraded` | `restart` | Restart | yes |

The confirmation of Start MUST ask whether to start Tamanu.

The confirmation of Restart MUST ask whether to restart Tamanu, and MUST say that Tamanu may be unavailable while it restarts.

> [!NOTE]
> A device reports none of the Tamanu facts until its first look at Tamanu has ended, as [NFO](device-info.md) has it, so the row says it is checking rather than leaving a gap.

## Advanced

The application MUST gather, beneath the rows, under a disclosure headed Advanced, in this order, the ways to the full readings of [VIEW](device-view.md), the network settings of [NSCR](network/screen.md), the Tailscale screen of [TSCR](control/tailscale-screen.md) where the device reports `tailscale-state`, and the power and battery controls of [CSCR](control/screen.md).

The application MUST show the disclosure closed when the operator opens a channel to a device, and MUST keep it open or closed as the operator left it until they do again.

## Wording

The application MUST supply its own wording for every act and every result, and MUST NOT put the vocabulary of [TSC](control/tailscale.md) or [TMU](control/tamanu.md) in front of an operator.

## While a power act is under way

The application MUST show on the Status screen a device carrying out a power act of [CTL](control/power.md), as [WEB](web-app.md) specifies, in place of the rows and Advanced, for as long as the act is under way.
