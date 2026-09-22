# Sampler blocking reads off the async runtime

Scenarios verifying that moving the sampler's gathering off the async runtime leaves its cadence, window, and output unchanged (NFO, "Sampling"), and that a slow source cannot take the session down with it.

## Isolation from a slow source

Operational concerns rather than protocol requirements: nothing a reader observes changes, so these cite no spec criterion.

- [x] A source that blocks mid-sample does not stall the runtime carrying the session: an async timer still fires and a session opens and closes while a source sits blocked
- [x] Once a blocked source answers, sampling carries on and the reading reaches the live feed

## Unchanged behaviour

- [x] A subscriber receives readings as they are taken (verifies spec: NFO)
- [x] The current snapshot merges fast and slow tiers so a feed opening sees a full set at once (verifies spec: NFO)
- [x] The first sample establishes a baseline and reports no rate for cumulative counters (verifies spec: NFO)
- [ ] After the idle stop clears and a session reopens, every counter baselines again and the snapshot starts empty (verifies spec: NFO)
- [ ] On real hardware, sampling delivers the same readings at the same cadence (verifies spec: NFO)
