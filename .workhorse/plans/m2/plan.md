# Country from nearby networks

## Decisions from workshopping

- The signal is the Country element (802.11d, element ID 7) in each access point's beacon or probe response. The device already walks these elements in `observe/bss.rs`, so reading it is one more lookup.
- The device reports each access point's advertised country on its `access-points` entry as an optional `country` member, left out where the access point names none. It makes no guess of its own. The client tallies, as it already groups access points into networks in `scan.js`.
- The suggestion fills the Country field only. The operator still applies, as NSCR requires for every edit.
- Any scan feeds it, and the Country section gets its own Scan button, the same one as beside SSID, so an operator who only runs a hotspot can still use it.
- Shown prominently while the country is unset, and as a quiet line when the field holds a different country. Nothing is shown when they agree.
- The most-advertised country is offered, and any others heard are offered beside it. No counts are shown: it saves scrolling the country list, and the operator knows where they are standing. A tie offers each tied country equally and picks none.

## Counting

- Every access point heard counts once, with no weighting by signal and no grouping of the SSIDs one physical access point runs.
- Ignore elements naming no ISO 3166-1 country (`XX`, `EU`, anything not in `countries.js`), and any country the device's capabilities do not offer.

## Indoor and outdoor

- Each `access-points` entry also carries where its Country element says the access point operates: indoor, outdoor, or both. Left out where the element names no country, or where its third octet names an operating class table instead of an environment.
- Using it in the siting view is card A3. This card only puts it on the wire.

## Following the joined access point

- An unset device may take the country advertised by the access point its wireless client has joined. This needs an exception to the rule in the network overview spec (NET) that an unset device stays on channels every domain permits.
- bliti never asks the kernel to ignore Country elements: it only sets the domain, through the cfg80211 module option at boot and `NL80211_CMD_REQ_SET_REG` after that. Whether the hint is honoured is up to the kernel and the driver. So the device-side work is checking on the prototype device (`iw reg get` before and after joining an access point that advertises a country), not code to turn anything off.
- A domain the kernel adopts from a joined access point changes the usable channels without a proposal. CFG only has the device send `capabilities` when a proposal or a return to the recorded configuration changes them, so it needs to cover this case too.
