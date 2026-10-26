---
id: TSC
---

# Tailscale control

A client asks a device to sign in to Tailscale, turn it on or off, log out of it, test the network beneath it, and test the path to another machine on its tailnet, through a Tailscale stream.

The device's state in Tailscale is reported by [NFO](../device-info.md) as `tailscale-state`.

The Tailscale stream is a stream role beyond those of [MSG](../messages.md), established as [MSG](../messages.md) requires by which end opened the stream and by its first message.

## Borrowed terms

[NFO](../device-info.md) defines tailnet.

| term | meaning |
| --- | --- |
| auth key | A credential issued by the tailnet's administrator that signs a machine in without anyone approving the sign-in. |
| login link | A web address Tailscale issues for a sign-in, which a person opens and approves in their own account. |
| relay | A server Tailscale passes a machine's traffic through where the machines cannot reach each other directly. |
| region | One of the places Tailscale runs relays, each with a number and a name. |
| NAT | Network address translation, where a router presents many machines as one address. |

## The acts

| act | effect |
| --- | --- |
| `sign-in` | signs the device in, so it becomes a member of a tailnet |
| `turn-on` | connects the device to the tailnet it is signed in to |
| `turn-off` | disconnects the device from the tailnet, leaving it signed in |
| `log-out` | removes the device's sign-in |
| `netcheck` | tests the network beneath Tailscale |
| `ping` | tests the path from the device to another machine on its tailnet |

A device MUST list an act only where it can carry it out now, by the table below.

| act | listed while `tailscale-state` is |
| --- | --- |
| `sign-in` | `needs-login` or `expired` |
| `turn-on` | `stopped` |
| `turn-off` | `running` or `starting` |
| `log-out` | `running`, `starting`, `stopped` or `needs-approval` |
| `netcheck` | any state, and where `tailscale-state` is not reported at all, wherever the device can run the test |
| `ping` | `running` |

A device MUST list `netcheck` only, where it can run the test, while `tailscale-state` is `broken` or `skipped`.

> [!NOTE]
> Which acts are listed follows from the state, so a client holds no table of its own saying what each state allows.
> `turn-off` and `log-out` take the device out of reach of the fleet until it is turned on or signed in again, and a client confirms them in words saying so.
> The channel is carried over [CHN](../channel.md) and not over Tailscale, so neither act can strand the operator who asks for it.

## The exchange

A client MUST open a Tailscale stream by opening a stream whose first message is `tailscale`.

A device MUST answer `tailscale` with `tailscale-acts`, carrying as `acts` an array of the acts it can carry out now.

A device MUST send `tailscale-acts` on every open Tailscale stream each time the acts it can carry out change.

A device that does not run Tailscale MUST NOT list any act but `netcheck`.

A client MUST ask for an act by sending `tailscale-act` on the Tailscale stream, carrying as `ACT` a critical member naming the act, and the act's own parameters.

A device MUST answer every `tailscale-act` exactly once, with `cannot-act` of [MSG](../messages.md) where it will not carry the act out, and otherwise with `tailscale-accepted`.

A device MUST refuse an act it did not list, at `$['act']`.

A device MUST refuse an act asked for while another is under way, on any Tailscale stream, at `$`, with a reason saying an act is under way.

An act is under way from the moment the device accepts it until it has been carried out, has failed, or has been ended.

A device MUST send `tailscale-failed`, carrying `reason`, where it has accepted an act and does not carry it out, and MUST NOT send it otherwise.

A device MUST NOT wait indefinitely for an act to be carried out.

A device MUST serve any number of Tailscale streams at once, on one channel or across several.

A device MUST end an act it accepted that is waiting on something outside the device when the stream that asked for it ends, however it ends: a `netcheck`, a `ping`, or a `sign-in` by login link.
A device MUST leave Tailscale as it stands when such an act is ended.

A device MUST carry out every other act it accepted once the stream that asked for it has ended.

An act that changes Tailscale is carried out when Tailscale reaches the state the act aims at, as the sections below give.

> [!NOTE]
> The selector is critical because each act changes what the device is, as the `act` of [CTL](power.md) does.
> Every parameter is critical as well, because an act carried out without one it did not read would be a different act: a sign-in under the wrong name, or a ping of the wrong machine.
> Unlike a power act, none of these acts ends the channel, so there is no `going-away` to send.
> A client learns that an act which changes Tailscale was carried out from `tailscale-state` on the `default` feed, where it learns everything else about the device's standing, and learns that one failed from `tailscale-failed`, since a failure may leave the state as it was.

## Signing in

`sign-in` MUST carry `METHOD`, which is `auth-key` or `login-link`.

A device MUST refuse a `METHOD` it does not recognise, at `$['method']`.

`sign-in` by `auth-key` MUST carry `KEY`, the auth key, and a device MUST refuse one that does not, at `$['key']`.

`sign-in` MAY carry `NAME`, the name the device registers under.

A device MUST register under its own hostname where `NAME` is absent.

`sign-in` is carried out when Tailscale reaches `running` or `needs-approval` in a tailnet the device accepts.

A device MAY be configured to accept a single tailnet, by means outside this protocol.

A device so configured MUST NOT start its bliti software where it is signed in to any other tailnet.

A device so configured MUST sign out of any other tailnet a `sign-in` joins, MUST send `tailscale-failed` with a `reason` saying it accepts only its configured tailnet, and MUST NOT leave Tailscale signed in to the other.

A device signing in by `auth-key` MUST send `tailscale-failed` where Tailscale rejects the key or does not reach either state.

A device signing in by `login-link` MUST send `tailscale-login-link`, carrying as `url` the login link, as soon as Tailscale issues one, and MAY send it again where Tailscale issues another.

A client MUST treat the latest `tailscale-login-link` as the link to open.

A device signing in by `login-link` MUST send `tailscale-failed` where the link lapses or Tailscale does not reach `running` or `needs-approval` once the person who opened the link has approved it.

A device MUST NOT persist an auth key or a login link, and MUST NOT put one in a log or in the `reason` of a message.

> [!NOTE]
> An auth key suits a fleet, since a tagged key signs a device in without a person present and without a login that lapses.
> A login link suits a technician with an account who has no key.
> A device already signed in to another tailnet cannot honour the restriction, and refusing to start says so at the point the configuration is made, not when an operator next signs in.
> A key or a link names no tailnet until it is used, so a device learns which tailnet a sign-in reached only once it has reached it, and a device that accepts one tailnet signs out of the others afterwards and not before.
> `needs-approval` is a sign-in that succeeded and is waiting on an administrator, which is nothing more the operator can do at the device.

## Turning on and off, and logging out

`turn-on`, `turn-off` and `log-out` carry nothing beyond `ACT`.

Each is carried out when Tailscale reaches `stopped` after `turn-off`, `running` or `starting` after `turn-on`, and `needs-login` after `log-out`.

A device MUST send `tailscale-failed` where Tailscale does not reach it.

## Relays in a result

A result names a relay by an object with these members:

| member | type | meaning |
| --- | --- | --- |
| `id` | number | the number Tailscale gives the relay's region |
| `name` | string or null | the region's name; null where the device does not know it |

## Network check

`netcheck` carries nothing beyond `ACT`.

A device MUST run `netcheck` without regard to whether it is signed in, and MUST send `tailscale-result` carrying as `result` an object with these members:

| member | type | meaning |
| --- | --- | --- |
| `udp` | boolean | whether the device can exchange UDP datagrams with a relay |
| `ipv4` | boolean | whether the device reaches the internet over IPv4 |
| `ipv6` | boolean | whether the device reaches the internet over IPv6 |
| `nat-varies` | boolean or null | whether the NAT in front of the device presents it at a different address to each destination; null where the test could not tell |
| `captive-portal` | boolean or null | whether the network the device is on intercepts web traffic until a person signs in to it; null where the test could not tell |
| `relays` | array | each relay that answered, as a relay object with its `latency` in seconds added, ordered by rising latency |

A device MUST send `tailscale-failed` where it could not run the test, and MUST send `tailscale-result` where it ran the test, whatever the network it found.

A relay has answered where it replied to the device over any transport, and `udp` says whether it did so over UDP.

A device sending `tailscale-result` with no relay in `relays` MUST carry `udp` as false.

> [!NOTE]
> The test measures the network the device is on and not Tailscale, which is why it runs for a device that is not signed in: a device that cannot sign in is usually on a network that blocks Tailscale, and the result says which part where the test can run at all.
> A `nat-varies` of true is the case in which two machines behind such NATs cannot connect directly and Tailscale falls back to a relay.
> An empty `relays` is a network that reaches no relay at all, and the first of `relays` is the nearest.
> A device that has never been signed in may not know region names, which is why `name` can be null and `id` cannot.

## Ping

`ping` MAY carry `PEER`, the machine to test.

`PEER` MUST be the machine's name on the tailnet, bare or fully qualified, and MUST NOT be an address.

A device MUST test a machine of its own choosing where `PEER` is absent, and MUST send `tailscale-failed` where it has none.

A device MUST send `tailscale-result` carrying as `result` an object with these members:

| member | type | meaning |
| --- | --- | --- |
| `peer` | string | the machine tested, as Tailscale names it |
| `reachable` | boolean | whether the machine answered |
| `path` | string or null | `direct` where it answered over a direct connection, `relayed` where through a relay; null where it did not answer |
| `relay` | object or null | the relay that carried it, as a relay object, where `path` is `relayed`; null otherwise |
| `latency` | number or null | the round trip in seconds; null where it did not answer |

A device MUST report `path` as `direct` where the machine answered over a direct connection at any point while the device tests it, and as `relayed` where it answered only through a relay.

A device MUST send `tailscale-failed` where `PEER` names no machine on its tailnet, or where it could not run the test.

> [!NOTE]
> A machine that exists and does not answer is a result and not a failure, because that is what the operator asked the device to find out.
> The default machine is a matter of how a device is deployed, which is why it is the device's to choose and `peer` says which was tested.

## Logging

A device MUST log every act asked for, with whether it was accepted, and the `name` and `version` the asking client gave in its `hello`.

A device MUST log an act it accepted and did not carry out, with the reason it failed.

## Message types

| type | sent by | on | carries |
| --- | --- | --- | --- |
| `tailscale` | client | a Tailscale stream, as the first message | nothing beyond `type` |
| `tailscale-acts` | device | the Tailscale stream | `acts` |
| `tailscale-act` | client | the Tailscale stream | `ACT` and the act's parameters |
| `tailscale-accepted` | device | the Tailscale stream | nothing beyond `type` |
| `tailscale-login-link` | device | the Tailscale stream | `url` |
| `tailscale-result` | device | the Tailscale stream | `result` |
| `tailscale-failed` | device | the Tailscale stream | `reason` |
| `cannot-act` | device | the Tailscale stream | as [MSG](../messages.md) has it |
