---
id: WIFI
---

# Joining wifi

The Join wifi screen is where an operator joins the device to one wireless network: the simple case of the network configuration of [NET](overview.md), presented for a facility operator rather than a technician.

It is one of the three ways of [NSET](setup.md), reached from the Network screen, and its changes are proposed and kept as [NSET](setup.md) has it.
[NSCR](screen.md) is the full editor, reached under Advanced, and holds everything this screen leaves out: more than one network, the ordering of [LINK](attachment.md), enterprise networks, and the radio settings of [HOT](hotspot.md).

## The one network

The application MUST title the screen Join wifi, and MUST carry that title where the device view carries its own, as [VIEW](../device-view.md) has it, with the way back to the Network screen beside it.

The application MUST present a single wireless candidate: the first wireless attachment in the ordering of the configuration in force where there is one, and none where there is not.

The application MUST leave every wired attachment and the ordering of [LINK](attachment.md) as the configuration in force holds them, changing only the one wireless candidate and, as [NSET](setup.md) has it, the hotspot.

The application MUST show the network the device is joined to from `wireless-network`, and, where it reports no wireless network joined, that the device is online by cable where a wired interface carries the `default` route.

The application MUST send a network the operator joins to the top of the attachment ordering, so the device prefers it, and MUST give it `enabled` and `verify` true.

## Choosing a network

The application MUST show the operator where they are in joining, as three steps: the network, its password, and connecting.

The application MUST scan on opening the network step, and MUST list the networks a scan heard by SSID with the strongest signal among each network's access points, as [NSCR](screen.md) lists them, leaving out hidden networks.

The application MUST offer to scan again, and MUST say where a scan heard no network.

The application MUST let the operator pick a network to join, and MUST then ask only for the one secret it needs: the passphrase of a key-based network.

The application MUST send an operator who picks a network the device secures by enterprise to the editor of [NSCR](screen.md), saying the network needs settings kept under Advanced, rather than ask for them here.

The application MUST let the operator join a hidden network by naming it.

## Joining

The application MUST tell the operator, while the device verifies the join, that it is connecting, and MUST let them cancel, discarding the proposal of [CFG](session.md).

On a join applied, the application MUST tell the operator the device is connected to the network.

On a join that fails, the application MUST say why in a facility operator's terms, drawn from the failure of [CFG](session.md): a wrong passphrase where the failure is at the passphrase, out of range where the network was not reached, and the device's `reason` otherwise.

The application MUST let the operator correct the passphrase and try again without picking the network afresh.
