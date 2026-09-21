# Wire compatibility checking

Scenarios verifying that a breaking change to the wire protocol fails CI, and that a change MSG
permits does not. Each staged break below is applied to the current build, the check run, and the
change reverted.

## The oracle catches what breaks

- [x] A member the baseline writes that the current build no longer writes fails, naming the member
- [x] A member whose JSON type changed fails, showing the old and new values
- [x] A required member the current build stops writing faults the baseline
- [x] The failure names the snapshot line, corpus entry, or generated message that carried it
- [ ] A message type the current build stops knowing fails rather than being skipped
- [ ] A member removed from a type the snapshot carries no example of is caught once the baseline
      generates live (verifies the snapshot is not the only reach)

## The oracle does not fire on what is permitted

- [x] Adding an ignorable member passes in both directions (MSG, "How a message type grows")
- [x] A critical trait nested in `traits` round-trips without being reported lost, its case folded
- [ ] Adding a whole message type passes, the baseline skipping it (MSG, "What is not recognised
      is skipped")
- [ ] Adding a critical member passes the oracle, the baseline refusing rather than faulting

## The ledger

- [x] A member marked critical and not recorded fails, naming the type and member
- [x] An entry recorded but no longer critical fails
- [x] An entry without a real reason fails
- [x] The ledger's initial content matches what the build marks critical today

## Gating and vacuity

- [ ] Where the baseline and current version markers differ, the oracle reports itself skipped
      rather than passing (VER)
- [x] An emptied baseline snapshot fails rather than passing quietly
- [ ] A regression recorded in the corpus is checked on every run

## The generator

- [x] Every generated message is read by its own build without fault, skip or refusal
- [x] Optional members are reached: `unit` present, `value` absent, traits non-empty, a critical
      trait name
- [x] Every type in `known_types` is generated

## Not covered, and cannot be

Repurposing: a `unit` of `celsius` becoming one of `kelvin` parses identically in both builds and
loses nothing. MSG forbids it and no mechanism here can see it. It stays a matter for review.
