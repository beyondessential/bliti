# Country from nearby networks

## Decisions from workshopping

- The signal is the Country element (802.11d, element ID 7) in each access point's beacon or probe response. The device already walks these elements in `observe/bss.rs`, so reading it is one more lookup.
- The device reports each access point's advertised country on its `access-points` entry, and makes no guess of its own. The client tallies, as it already groups access points into networks in `scan.js`.
- The suggestion fills the Country field only. The operator still applies, as NSCR requires for every edit.
- Any scan feeds it, and the Country section gets its own Scan button, the same one as beside SSID, so an operator who only runs a hotspot can still use it.
- Shown prominently while the country is unset, and as a quiet line when the field holds a different country. Nothing is shown when they agree.
- Plurality with counts: the most-advertised country is offered with "N of M", and any others heard are offered beside it. A tie offers each tied country equally and picks none.

## Counting

- Count distinct BSSIDs, so an access point heard on two radios counts once.
- Ignore elements naming no ISO 3166-1 country (`XX`, `EU`, anything not in `countries.js`), and any country the device's capabilities do not offer.
- M is the number of access points that named a country, not every access point heard.
