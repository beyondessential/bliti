# Sampler blocking reads off the async runtime

Scenarios verifying that moving the sampler's gathering off the async runtime protects the session, while leaving the sampler's cadence, window, and output unchanged (NFO, "Sampling").

## Isolation from a slow source

- [x] A source that blocks mid-sample does not stall the runtime carrying the session: an async timer still fires and a session opens and closes while a source sits blocked (verifies spec: NFO)
- [x] Once a blocked source answers, sampling carries on and the reading reaches the live feed (verifies spec: NFO)

## Unchanged behaviour

- [x] A subscriber receives readings as they are taken (verifies spec: NFO)
- [x] The current snapshot merges fast and slow tiers so a feed opening sees a full set at once (verifies spec: NFO)
- [x] The first sample establishes a baseline and reports no rate for cumulative counters (verifies spec: NFO)
- [ ] After the idle stop clears and a session reopens, every counter baselines again and the snapshot starts empty (verifies spec: NFO)
- [ ] On real hardware, sampling continues to deliver the same readings at the same cadence as before the change
