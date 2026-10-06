---
id: WIFI
---

# Joining wifi

The Wifi screen is where an operator joins the device to one wireless network: the simple case of the network configuration of [NET](overview.md), presented for a facility operator rather than a technician.

[NSCR](screen.md) is the full editor, reached under Advanced, and holds everything this screen leaves out: more than one network, the ordering of [LINK](attachment.md), enterprise networks, and the radio settings of [HOT](hotspot.md).
This screen edits one wireless network through the session of [CFG](session.md), as [NSCR](screen.md) does, and leaves the wired attachments and the hotspot as they are.

## The one network

The application MUST present a single wireless candidate: the wireless attachment of the configuration in force where there is one, and none where there is not.

The application MUST leave every wired attachment, the hotspot, and the ordering of [LINK](attachment.md) as the configuration in force holds them, changing only the one wireless candidate.

The application MUST show the network the device is joined to from `wireless-network`, and that the device is using the wired network alone where it reports no wireless network joined.

The application MUST send a network the operator joins to the top of the attachment ordering, so the device prefers it, and MUST give it `enabled` and `verify` true.

> [!NOTE]
> A facility operator has one wifi network in mind: the clinic's. The ordering, a second network and the rest are a technician's, and live under Advanced where a technician looks for them.

## Choosing a network

The application MUST offer to scan, and MUST list the networks a scan heard by SSID with the strongest signal among each network's access points, as [NSCR](screen.md) lists them, leaving out hidden networks unless the operator asks for them.

The application MUST let the operator pick a network to join, and MUST then ask only for the one secret it needs: the passphrase of a key-based network.

The application MUST send an operator who picks a network the device secures by enterprise to the editor of [NSCR](screen.md), saying the network needs settings kept under Advanced, rather than ask for them here.

The application MUST let the operator join a hidden network by naming it, as [NSCR](screen.md) does.

## Joining

The application MUST propose the network through the session of [CFG](session.md), and MUST verify it as any proposal is verified.

The application MUST tell the operator, while the device verifies the join, that it is connecting.

On a join applied, the application MUST tell the operator the device has connected and MUST ask them to keep it, confirming the proposal of [CFG](session.md) on keep and discarding it otherwise, as a network not kept reverts.

On a join that fails, the application MUST say why in a facility operator's terms, drawn from the failure of [CFG](session.md): a wrong passphrase where the failure is at the passphrase, out of range where the network was not reached, and the device's `reason` otherwise.

The application MUST let the operator correct the passphrase and try again without picking the network afresh.

> [!NOTE]
> A wifi change cannot drop the channel, which is Bluetooth, so the keep-it step is safe: the device tries the network, the operator sees it worked, and only then is it recorded. A join that silently stuck would leave a clinic on a network nobody chose to keep.
