---
id: TSCR
---

# Tailscale screen

The screen an operator controls a device's Tailscale from, and the screen they sign it in from, through the acts of [TSC](tailscale.md), by our application.

[TSC](tailscale.md) binds every implementation.
This spec says how ours applies it, and binds nothing else that controls a bliti device.

## What it holds

The application MUST title the screen Tailscale, and MUST carry that title where the device view carries its own, as [VIEW](../device-view.md) has it, with the way back to the Status screen of [STAT](../status-screen.md) beside it.

The application MUST hold the screen's sections in the order details, acts, network check, ping.

The application MUST open a Tailscale stream when the operator opens the screen, and MUST close it when they leave.

## Details

The application MUST show how Tailscale stands with the mark, the words and the `reason` the Tailscale row of [STAT](../status-screen.md#the-tailscale-row) shows for it.

Beneath it, the application MUST show, each labelled and only where it carries a value, the name the device is reached by from `tailscale-name`, the tailnet from `tailnet`, and when the sign-in lapses from `tailscale-key-expiry`, as a date and as a time from now.

## Acts

The application MUST offer, of connect, turn on, turn off and log out, the acts the device lists, and no others, in that order.

The application MUST leave out the acts section until the device has listed its acts, and where it lists none of these.

Connect MUST open the sign-in screen.

The application MUST ask the operator to confirm a turn off or a log out, each time, before asking the device for it.

The confirmation MUST name the act in the words of the act's button, and MUST be shown beneath the act's row in place of its button, leaving the other acts and sections in view.

The confirmation of a turn off MUST say that the fleet cannot reach the device through Tailscale until it is turned on again.

The confirmation of a log out MUST say that the fleet cannot reach the device through Tailscale until it is signed in again.

The application MUST NOT offer an act again from asking the device for it until the device refuses it, sends its result or its failure, or lists its acts anew.

The application MUST render the reason of a `cannot-act` or a `tailscale-failed` as the device wrote it.

## Network check

The application MUST offer to run a network check where the device lists `netcheck`.

While a network check is under way, the application MUST show that it is checking.

The application MUST show the result of the latest network check the device sent while the channel has been open, whether it was run from this screen or from the Status screen.

The application MUST show the result as the verdict the Tailscale row of [STAT](../status-screen.md#the-tailscale-row) gives it, and beneath it:

- whether the device can exchange UDP with a relay, and whether it reaches the internet over IPv4 and over IPv6;
- whether the network presents the device at a different address to each destination, and whether it asks for a sign-in, each where the result says;
- the relays that answered, nearest first, each by its name, or its number where the device gives no name, with its latency.

The application MUST offer to share the result as text, through the share sheet of the system where the browser offers one, and otherwise by copying it.

The shared text MUST carry the device's `hostname`, when the check ran, the `result` as the device sent it, as JSON, without `report`, and then `report` as the device sent it.

## Ping

The application MUST offer to ping where the device lists `ping`, with a field for the machine to test, by its name on the tailnet.

The application MUST leave out `PEER` where the field is empty, so the device chooses the machine.

The application MUST show the result as the machine the device tested, whether it answered, and where it did, whether directly or through a relay, naming the relay, and the round trip.

## Signing in

The application MUST title the sign-in screen Connect to Tailscale, carrying that title as the Tailscale screen carries its own, with the way back beside it.

The way back MUST return the operator to the screen they opened the sign-in screen from.

The application MUST open a Tailscale stream when the operator opens the sign-in screen, and MUST close it when they leave.

The application MUST offer to sign in by auth key, and beside it to sign in by login link instead.

Both ways MUST offer a field for the name the device registers under, filled at first with the device's `hostname` of [NFO](../device-info.md).

The application MUST send the name as `NAME` only where the operator has changed it.

The application MUST return the operator to the screen they opened the sign-in screen from once `tailscale-state` is `running` or `needs-approval`.

The application MUST render the reason of a `cannot-act` or a `tailscale-failed` as the device wrote it, and MUST then offer to sign in again.

### By auth key

The application MUST offer a field for the auth key, hiding what is typed until the operator asks to see it.

The application MUST offer to fill the field by scanning a QR code with the camera.

The application MUST NOT keep an auth key once the operator leaves the sign-in screen.

### By login link

Once the operator asks for a login link, the application MUST show that it is waiting for one until the device sends `tailscale-login-link`.

The application MUST then show the latest login link the device sent, as a QR code, and MUST offer to open it, to copy it, and to share it through the share sheet of the system, where the browser offers one.

The application MUST say, while it shows the link, that the device signs in once someone approves it.

Where the stream closes while the application shows a login link, the application MUST stop showing it and offer to get a new one.

> [!NOTE]
> The QR code lets a second device approve the link, and sharing sends it to someone with an account, so the operator's own page need not leave the foreground.
> A phone may end the Bluetooth link of a page it puts in the background, as [WEB](../web-app.md#while-the-page-is-hidden) has it, and the device ends a sign-in by login link with the stream that asked for it.

## Wording

The application MUST supply its own wording for every act and every result, and MUST NOT put the vocabulary of [TSC](tailscale.md) in front of an operator.

The application MUST name `sign-in` as connecting, `turn-on` and `turn-off` as turning on and off, `log-out` as logging out, `netcheck` as a network check, and `ping` as a ping.
