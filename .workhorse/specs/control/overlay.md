---
id: OVL
---

# Overlay control

A client asks a device to sign in to its overlay, turn it on or off, log out of it, test the network beneath it, and test the path to another machine on it, through an overlay stream, without the channel or any other stream closing.

The overlay is the one [NFO](../device-info.md) reports, and the device's state in it is reported there as `overlay-state`.

The overlay stream is a stream role beyond those of [MSG](../messages.md), established as [MSG](../messages.md) requires by which end opened the stream and by its first message.

## Borrowed terms

| term | meaning |
| --- | --- |
| auth key | A credential issued by the overlay's administrator that signs a machine in without anyone approving the sign-in. |
| login link | A web address the overlay issues for a sign-in, which a person opens and approves in their own account. |
| control server | The service an overlay's machines register with and receive the tailnet's membership from. |
| relay | A server the overlay passes a machine's traffic through where the machines cannot reach each other directly. |
| NAT | Network address translation, where a router presents many machines as one address. |

## The acts

| act | effect |
| --- | --- |
| `sign-in` | signs the device in, so it becomes a member of a tailnet |
| `turn-on` | connects the device to the tailnet it is signed in to |
| `turn-off` | disconnects the device from the tailnet, leaving it signed in |
| `log-out` | removes the device's sign-in |
| `netcheck` | tests the network beneath the overlay |
| `ping` | tests the path from the device to another machine on its tailnet |

A device MUST list an act only where it can carry it out now, by the table below.

| act | listed while `overlay-state` is |
| --- | --- |
| `sign-in` | `needs-login` or `expired` |
| `turn-on` | `stopped` |
| `turn-off` | `running` or `starting` |
| `log-out` | `running`, `starting`, `stopped` or `needs-approval` |
| `netcheck` | any state, and where `overlay-state` is not reported at all, wherever the device can run the test |
| `ping` | `running` |

> [!NOTE]
> Which acts are listed follows from the state, so a client holds no table of its own saying what each state allows.
> `turn-off` and `log-out` take the device out of reach of the fleet until it is turned on or signed in again, and a client confirms them in words saying so.
> The channel is carried over [CHN](../channel.md) and not over the overlay, so neither act can strand the operator who asks for it.

## The exchange

A client MUST open an overlay stream by opening a stream whose first message is `overlay`.

A device MUST answer `overlay` with `overlay-acts`, carrying as `acts` an array of the acts it can carry out now.

A device MUST send `overlay-acts` on every open overlay stream each time the acts it can carry out change.

A client MUST ask for an act by sending `overlay-act` on the overlay stream, carrying as `ACT` a critical member naming the act, and the act's own parameters.

A device MUST answer every `overlay-act` exactly once, with `cannot-act` of [MSG](../messages.md) where it will not begin the act, and otherwise with `overlay-done` where the act was carried out and `overlay-failed` where it was begun and not carried out.

A device MUST refuse an act it did not list, at `$['act']`.

A device MUST refuse an act asked for while another is under way, on any overlay stream, at `$`, with a reason saying an act is under way.

A device MUST serve any number of overlay streams at once, on one channel or across several.

A client closing an overlay stream MUST end any act it asked for that is waiting on something outside the device: a `netcheck`, a `ping`, or a `sign-in` by login link.
A device MUST leave the overlay as it stands when such an act is ended.

A device MUST carry out every other act it began once the stream that asked for it has closed.

A device MUST report the overlay's new state in `overlay-state` no later than it sends `overlay-done` or `overlay-failed`.

> [!NOTE]
> The selector is critical because each act changes what the device is, as the `act` of [CTL](power.md) does.
> Every parameter is critical as well, because an act carried out without one it did not read would be a different act: a sign-in to the wrong control server, or a ping of the wrong machine.
> Unlike a power act, none of these acts ends the channel, so a device answers with the outcome rather than going away.
> A device with no overlay client lists only the acts it can carry out without one, which may be none.

## Signing in

`sign-in` MUST carry `METHOD`, which is `auth-key` or `login-link`.

A device MUST refuse a `METHOD` it does not recognise, at `$['method']`.

`sign-in` by `auth-key` MUST carry `KEY`, the auth key, and a device MUST refuse one that does not, at `$['key']`.

`sign-in` MAY carry `NAME`, the name the device registers under, and `SERVER`, the address of the control server it registers with.

A device MUST register under its own hostname where `NAME` is absent, and with the overlay's own control server where `SERVER` is absent.

A device signing in by `auth-key` MUST answer `overlay-done` once the overlay reaches `running` or `needs-approval`, and `overlay-failed` with a `reason` where the overlay rejects the key or does not reach either.

A device signing in by `login-link` MUST send `overlay-login-link`, carrying as `url` the login link, as soon as the overlay issues one.

A device signing in by `login-link` MUST answer `overlay-done` once the person who opened the link has approved it and the overlay reaches `running` or `needs-approval`, and `overlay-failed` with a `reason` where the link lapses or the overlay does not reach either.

A device MUST NOT wait indefinitely for a sign-in.

`overlay-done` MUST carry as `result` an object whose `state` is the `overlay-state` the device reached.

A device MUST NOT store or log an auth key or a login link.

> [!NOTE]
> An auth key suits a fleet, since a tagged key signs a device in without a person present and without a login that lapses.
> A login link suits a technician with an account who has no key.
> `needs-approval` is a sign-in that succeeded and is waiting on an administrator, which is nothing more the operator can do at the device.

## Turning on and off, and logging out

`turn-on`, `turn-off` and `log-out` carry nothing beyond `ACT`.

A device MUST answer each with `overlay-done` carrying as `result` an object whose `state` is the `overlay-state` the device reached, and with `overlay-failed` with a `reason` where the overlay does not reach it.

A device MUST reach `stopped` after `turn-off`, `running` or `starting` after `turn-on`, and `needs-login` after `log-out`.

## Network check

`netcheck` carries nothing beyond `ACT`.

A device MUST run `netcheck` without regard to whether it is signed in, and MUST answer `overlay-done` carrying as `result` an object with these members:

| member | type | meaning |
| --- | --- | --- |
| `udp` | boolean | whether the device can exchange UDP datagrams with a relay |
| `ipv4` | boolean | whether the device reaches the internet over IPv4 |
| `ipv6` | boolean | whether the device reaches the internet over IPv6 |
| `nat-varies` | boolean or null | whether the NAT in front of the device presents it at a different address to each destination; null where the test could not tell |
| `captive-portal` | boolean or null | whether the network the device is on intercepts web traffic until a person signs in to it; null where the test could not tell |
| `nearest-relay` | string or null | the name of the relay with the lowest latency; null where none answered |
| `relays` | array | each relay that answered, as an object of `name` and `latency` in seconds, ordered by rising latency |

A device MUST answer `overlay-failed` with a `reason` where it could not run the test.

A device answering `overlay-done` with no relay in `relays` MUST carry `udp` as false.

> [!NOTE]
> The test measures the network the device is on and not the overlay, which is why it runs for a device that is not signed in: a device that cannot sign in is usually on a network that blocks the overlay, and the result says which part.
> A `nat-varies` of true is the case in which two machines behind such NATs cannot connect directly and the overlay falls back to a relay.
> An empty `relays` is a network that reaches no relay at all.

## Ping

`ping` MAY carry `PEER`, the machine to test, named as the overlay names machines on the tailnet.

A device MUST test the machine named `canopy` where `PEER` is absent.

A device MUST answer `overlay-done` carrying as `result` an object with these members:

| member | type | meaning |
| --- | --- | --- |
| `reachable` | boolean | whether the machine answered |
| `path` | string or null | `direct` where it answered over a direct connection, `relayed` where through a relay; null where it did not answer |
| `relay` | string or null | the relay that carried it where `path` is `relayed`; null otherwise |
| `latency` | number or null | the round trip in seconds; null where it did not answer |

A device MUST answer `overlay-failed` with a `reason` where `PEER` names no machine on its tailnet, or where it could not run the test.

> [!NOTE]
> A machine that exists and does not answer is a result and not a failure, because that is what the operator asked the device to find out.

## Logging

A device MUST log every act asked for, with whether it was accepted, and the `name` and `version` the asking client gave in its `hello`.

A device MUST log an act it began and could not carry out, with the reason it failed.

## Message types

| type | sent by | on | carries |
| --- | --- | --- | --- |
| `overlay` | client | an overlay stream, as the first message | nothing beyond `type` |
| `overlay-acts` | device | the overlay stream | `acts` |
| `overlay-act` | client | the overlay stream | `ACT` and the act's parameters |
| `overlay-login-link` | device | the overlay stream | `url` |
| `overlay-done` | device | the overlay stream | `result` |
| `overlay-failed` | device | the overlay stream | `reason` |
| `cannot-act` | device | the overlay stream | as [MSG](../messages.md) has it |
