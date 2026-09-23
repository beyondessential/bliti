---
id: LINK
---

# Network attachment

A device attaches to a network by working down an ordered list of candidates.
The list is the `attachments` member of the document of [NET](overview.md).

## Borrowed terms

| term | meaning |
| --- | --- |
| candidate | One way a device might attach to a network: a wireless network to join, or a wired port with its addressing. |
| carrier | Whether a physical link is electrically up, before any addressing. |

## The ordering

`attachments` MUST be an ordered array in which a wireless network and a wired configuration are peers.

Each candidate MUST carry:

| member | type | required | meaning |
| --- | --- | --- | --- |
| `kind` | string | yes | `wireless`, `wired-dynamic` or `wired-static` |
| `label` | string | yes | what the operator calls this candidate |
| `nameservers` | array | no | the resolvers of this link, in the order they are queried |

A `wireless` candidate MUST carry the members of [WLAN](wireless.md).

A `wired-dynamic` candidate MUST carry the `interface` it applies to, and MUST take its addressing from DHCP or from stateless address autoconfiguration.

A `wired-static` candidate MUST carry the `interface` it applies to, its `addresses` with their prefix lengths, and its `gateway`.

A device MUST accept at most one `wired-dynamic` candidate per interface, and MAY accept several `wired-static` candidates for one interface.

A device MUST treat a `wired-static` candidate carrying no `gateway` as invalid.

> [!NOTE]
> A flat ordering is what expresses a site whose own wireless is better than whatever its wall port reaches. An ordering by link could not, because the two would not be comparable.
> Several statics on one interface is what carries a device between sites: two sites offering a wall port on different subnets, neither handing out DHCP, are two candidates the device tells apart by trying them.
> The gateway is what makes a static candidate distinguishable, so a candidate without one cannot be told from any other. A wired network with no router is configured as the only static on its interface, where there is nothing to tell apart.

## Selecting a candidate

A device MUST bring up at most one candidate per interface, and MAY hold candidates on several interfaces up at once.

A device MUST bring up a `wireless` candidate naming an `interface` only on that interface.

A device MUST bring up a `wireless` candidate naming no `interface` on a wireless interface able to carry it and carrying no candidate higher in the ordering, and MUST prefer, among those, the one hearing the network best.

A device MUST take the candidate highest in the ordering among those it has brought up as the one carrying the default route.

> [!NOTE]
> The ordering decides which attachment carries traffic, not which exists. A device reachable on both a wall port and a wireless network at the same time is easier to find than one reachable on whichever it preferred.
> A wireless candidate naming no interface means the same on devices whose interfaces are named differently, as a USB adapter's commonly carries its address. Naming one is for an operator who wants a particular adapter to carry a particular network.

## Verification

A device MUST establish a candidate by observing, in order, the stages:

| stage | observed |
| --- | --- |
| `carrier` | the interface has carrier |
| `association` | a wireless interface has associated |
| `addressing` | an address is held, whether leased, autoconfigured or configured |
| `gateway` | the gateway answers |

A wired candidate has no `association` stage.

A device MUST treat a candidate that reaches the last of these as established, and one that does not as failed.

A device MUST NOT require a name to resolve, or a host beyond the gateway to answer, to establish a candidate.

> [!NOTE]
> The four stages catch a wrong key, an absent network, a network that associates and then hands out nothing, and a lease on a network that does not route, which is nearly everything that goes wrong.
> Reaching past the gateway would fail on a network that is deliberately offline, and a device serving a clinic with no connectivity is on exactly such a network.

## Selection is continuous

A device MUST select among its candidates whenever it is running, with or without a client connected.

A device MUST select again when:

- an interface gains or loses carrier, or a wireless network comes into or goes out of range
- the candidate carrying the default route stops verifying
- a candidate above the one in force becomes available

A device MUST NOT poll its candidates to detect these, except as follows.

A device MUST scan for a `wireless` candidate ranked above the candidate carrying the default route while the network is out of range, backing off between scans, and MUST NOT scan for one ranked below it.

> [!NOTE]
> Selecting on the candidate in force failing is what catches a site that changed around a device whose cable never moved.
> Selecting when something better returns is what stops a device sitting on a fallback for the rest of its life.
> Driving all three from events leaves a settled device doing nothing, which matters on a device that may be running from a battery.
> A network coming into range is the one event nothing announces: a radio hears it only by scanning. A device on its best candidate has nothing to scan for, so it still settles.

## Resolvers

A device MUST query the `nameservers` of a candidate before any the network supplies for that link.

A device MUST query the resolvers a network supplies where the candidate names none.

A device MUST send a query for a name under a search domain a network supplies for a link to the resolvers that network supplies.

> [!NOTE]
> Resolvers belong to a link because a site's internal names commonly resolve only on that site's own network, and a device may hold several links at once.
> A configured resolver answering that a name does not exist has answered, so nothing falls through to the site's own. Sending the site's own domain to the site's resolvers is what lets an operator add a public resolver without losing the site's names, where the site hands out its domain.
