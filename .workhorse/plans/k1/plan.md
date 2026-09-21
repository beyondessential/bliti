# Wire compatibility checking

## Approach: an executed oracle, not a schema diff

The technique is cargo-semver-checks' oracle rather than its lint set: never describe the
artefact, use the real thing on both sides and make something else adjudicate. For a Rust API
that adjudicator is the compiler. For a wire protocol there is nothing to compile, so the
oracle is executed instead: the baseline codec and the current codec are linked into one test
binary and cross-exercised, each reading what the other writes.

The point of doing it this way is that nothing describes the wire shape. The message types are
hand-written serde (`Message::to_map`, `Entry::write_into`), and the Rust struct does not match
the wire anyway: `traits` is a raw JSON map, the name member is `fact` or `measurement`
depending on the message type, and critical casing is applied by the envelope rather than the
type. Any extracted or hand-written schema would be a second artefact that can drift. The
oracle has none.

## The envelope already supplies the verdicts

`envelope::read` returns exactly the classification a compatibility check needs, because it is
the same question asked at a different time: can I act on what this peer just said?

| outcome | verdict |
| --- | --- |
| `Err(Fault)` | breaking |
| `Refused` | permitted, loud (a critical member was added) |
| `Skipped` | fine |
| `Message` | fine |

So the check reimplements none of MSG's growth rules. It runs the real reader and reads off
which variant came back.

## The two directions are not symmetric

- **old to new**: nothing old wrote may be missing from new's round trip. New is never the
  older peer, so it must understand everything old could say.
- **new to old**: must not be `Fault`. Old is entitled to `Skip` or `Refuse`; that is forward
  compatibility working, not a break.

The first rule is **containment, not byte equality**. Byte equality fails on a legal additive
change, because new re-serialising adds its own new member. Verified: see below.

## Spike findings (verified, not assumed)

Co-linking works. Two `bliti-core v0.0.0` from different sources build side by side; cargo
keys packages on source, so the shared version number is a non-issue. `channel::envelope` is
already public, so the oracle needs no change to `bliti-core` in order to exist.

Discrimination, against a deliberately broken current build:

| change | result |
| --- | --- |
| member removed (stopped writing `unit`) | caught, names `unit` |
| JSON type changed (`at` number to string) | caught both directions |
| ignorable member added | passes, no false positive |
| `celsius` to `kelvin` | passes: blind spot |

Repurposing is not machine-checkable here and stays a review concern. Worth saying plainly
rather than implying the growth rule is fully enforced.

**Coverage is the binding constraint.** The same `unit` removal passes silently when the
corpus holds no `quantity` entry, because `unit` is only written when present and only
`quantity` sets it. The oracle's reach is exactly the reach of its inputs.

## Where the generator goes

In `bliti-core` behind a feature, not in the oracle crate. In the oracle crate it would have to
construct the baseline and current message types separately, which means two copies that drift,
reintroducing the hand-maintained description the oracle exists to avoid. In `bliti-core` each
build brings its own: the old build generates what old could say, the new build what new can
say, and a newly added message type is exercised without anyone remembering to add it.

Costs, both real: `traits` is a raw JSON map so it needs a bounded JSON generator, and generated
member names must satisfy the envelope's naming rules or the oracle fails for reasons unrelated
to compatibility. Optional members need reliable population rather than chance coverage.

## Corpus: supplementary, as committed JSON bytes

Not a mechanism on its own. It carries specific regressions and cases generation is found not
to reach. Commit the raw JSON, not a generator seed: seeds replay through a generator that
changes, bytes replay verbatim through any build. That also keeps the corpus working in the
`old to new` direction if the baseline commit eventually stops building on a current toolchain,
which is co-linking's one durable weakness. A generated failure gets dumped to the corpus as
JSON when it is found.

## Gating, and the pre-release story

The oracle is gated on `key_schedule::VERSION`, as cargo-semver-checks gates on the major. If
the baseline and current markers differ the check is vacuous and is skipped: VER says a client
will not derive a handle for a version it does not implement, so the two ends never speak.

The baseline is a recorded commit. Before the first release, breaking the protocol is free, so
the rule is that the baseline pointer may be moved. At release the rule becomes that moving it
requires bumping `VERSION`. Same machinery throughout, one line of policy changes, and the
AGENTS.md "before the first release" clause gains a concrete replacement rather than being
dropped on trust. The check is live and exercised from the day it lands instead of sitting
dormant until release.

## Correction to the card description

The card says the version marker covers the message size ceiling that D1 tightened to 128 KiB.
That ceiling is gone: message size is now bounded structurally by the three-byte length prefix,
with no runtime check (`stream.rs`, "there is no ceiling to check and nothing to refuse"). There
is no size ceiling for a compatibility check to watch.

## Generation: proptest

Chosen for shrinking. The failure mode this exists to serve is "someone broke the wire and needs
to see which member", and a wall of generated JSON does not serve it. Shrinking also feeds the
corpus decision: a shrunk case is small enough to be worth committing verbatim.

### Generate the representable set, not the constructible one

`Entry`'s fields are all public, so a struct literal reaches shapes the constructors never
compose: only `Entry::quantity` sets `unit`, but any entry may carry one. Generate over the
representable set. Some shapes will look semantically odd, such as a `text` entry carrying a
unit, and that is the right trade: the wire admits them, and the web client or a third
implementation may emit them.

Verified to matter. With the removal of `unit` staged as a break, generation found it and shrank
to a `text` reading carrying a unit and nothing else. Generating through the constructors would
have confined `unit` to `quantity` entries and produced a larger, less pointed case.

### Coverage is adequate, given explicit optionality

Measured over 2000 samples, across the ~980 that were entries: `unit` present 490, `value`
absent 470, traits non-empty 718, an upper case trait name 485, a nested trait 702. The cliff the
hand-written corpus fell off does not appear, because optionality is generated explicitly rather
than reached by chance.

### Conformance is the real work in the strategy

The envelope faults on a member name that is mixed case, that carries anything but letters,
digits and hyphens, or that appears twice once lowercased. A generator ignoring those rules
produces messages both builds reject identically, which the oracle would read as a
compatibility failure. So the naming rules belong in the strategy, at every depth, including
inside `traits` and inside any generated `value`.

### The containment walk is case-blind

Found by generation, shrunk to `traits: {"A": null}`. The envelope lowercases member names on
read, so a critical trait `A` returns as `a`; comparing raw bytes reads that as a loss. A
member's case marks criticality, which is metadata rather than content, so the containment walk
folds case before comparing. That criticality itself may have changed is the ledger's question,
not the oracle's, and this is a second argument for the ledger existing.

### Seeds

Leave proptest on a random seed rather than pinning one. A compat check that explores more of
the space over time is worth more than one that is reproducible by construction, and every case
it finds becomes permanent the moment it is written to the corpus as JSON. The consequence is
that a latent break can surface on a change that did not cause it. That is the check working.

### Consequence for `bliti-core`

The generator lives in `bliti-core` behind a feature, so proptest becomes an optional dependency
there rather than a dev-dependency: a dev-dependency is invisible to anything that depends on
the crate, and the oracle depends on two builds of it.

## Acknowledging a deliberate break: a ledger

A member added in upper case is critical, and MSG permits it without moving the version marker.
It is nonetheless a break: every peer older than the change refuses that message type outright.
It should be possible, and impossible by accident.

The checking therefore splits in two, answering different questions:

- **the oracle** asks whether anything broke that must not. Behaviour, from real code on both
  sides.
- **the ledger** asks whether the set of deliberate breaks changed. A declaration, checked
  against real code.

The oracle structurally cannot do the second job: a refusal is legal, so it has nothing to fail
on. The ledger is what turns "permitted" into "permitted, but say so". It is the shape
`#[expect(..., reason = "...")]` already has in this repo, and the same bargain.

**The ledger is keyed on the code, not on the run.** Collecting the refusals the oracle happens
to observe would make it a function of random generation: flaky, and able to pass by luck. It is
keyed instead on `MessageSet::critical_members`, a `&'static` list and a property of the build,
independent of proptest entirely.

Enumerating it needs one addition: `MessageSet::known_types() -> &'static [&'static str]`, with
`knows()` gaining a default implementation of membership in it. That removes a remembering-point
rather than adding one, since the `Message` variants and the `matches!` in `knows` are currently
two lists that must silently agree. `knows` has one real implementation and one call site, so the
refactor is contained.

The ledger itself is hand-written TOML: every (message type, critical member) pair, with a
reason. A test asserts exact correspondence with `known_types()` by `critical_members()` in both
directions, so a missing entry fails and a stale one does too. Hand-written, so the reasons are
prose; enforced, so it cannot drift.

Adding an entry remains possible with a worthless reason. It is a diff in a file whose only
purpose is deliberate breaks, which is as far as enforcement reaches without demanding a marker
bump.

Constraint: `envelope::write` uppercases only top-level members, so this build cannot emit a
nested critical member. The ledger covers everything reachable today. Nested criticality on write
would require member paths rather than names.

## The walk stays in the oracle crate

Not upstreamed into `envelope`, deliberately, for two reasons.

They are not the same function. `collect_unknown` ignores scalar differences outright, its match
ending in a `_ => {}`, because it answers only which members this build failed to understand, for
the criticality refusal. The oracle needs fidelity, presence and value both, which is how a `at`
changing from `1` to `"1"` is caught. Sharing would mean widening a shipped function for a
test-only need.

The stronger reason is that the adjudicator must not be one of the things under test. Were the
walk in `bliti-core`, the oracle would have to pick the baseline's copy or the current one, and a
change to the walk would alter the comparison semantics along with the thing being compared. The
Rust oracle's adjudicator is the compiler, which belongs to neither crate version.

The copy carries a comment saying so, so that it is not later deduplicated as an oversight.

## Where things live

`wire-breaks.toml` at the repository root is the ledger: the acknowledged critical members, each
with a reason.

The baseline is a `[workspace.dependencies]` entry in the root `Cargo.toml`, the package renamed
and pinned by `rev`, taken by the oracle crate with `.workspace = true`. Cargo requires the rev in
a manifest, so this is the only home that avoids duplicating it or reading it from a build script,
and it puts the pointer in the most-read manifest in the repository. Verified to work, including
the rename alongside `git` and `rev`.

Its initial value is the commit introducing the ledger and the generator. The oracle calls the
baseline build's own generator, so the baseline cannot precede the commit that adds one. Because
this repository does not squash, the rev stays reachable and the pointer cannot dangle.

It is a pointer, not a constant. It moves when `VERSION` bumps, since a baseline at a different
marker makes the check vacuous and it would otherwise stay skipped for good. Before the first
release, where breaking the protocol is free and moving the marker is not wanted, a deliberate
break moves the baseline instead. After the release, moving it requires the marker to move too.

## Build

- [x] `MessageSet::known_types`, with `knows` derived from it, so a type cannot join the set
      without appearing where criticality is enumerated
- [x] `bliti-core`'s message generator, behind a `generate` feature, with its own tests for
      conformance, optional-member coverage and every known type being reached
- [x] `bliti-core-baseline` pinned in the workspace manifest, and `bliti-wire-compat` added as a
      member so the check runs under the existing `cargo test` CI job rather than a new one
- [x] The oracle: both read directions, the case-folding containment walk, and the version-marker
      gate that reports itself skipped rather than passing
- [x] `wire-breaks.toml` and its three checks: unrecorded, stale, and reasonless entries
- [x] The baseline snapshot, 240 messages as the baseline writes them, one per line in
      `baseline-snapshot.jsonl`, and the regression corpus beside it as an empty directory with
      the note saying what belongs there
- [x] Contributor note in `CONTRIBUTING.md` covering the two things the check asks of an author

### Verified by staging each break and reverting it

- [x] A member no longer written fails, naming the snapshot line that carried it
- [x] A member whose JSON type changed fails, showing both values
- [x] An added ignorable member passes, in both directions
- [x] A new critical member fails the ledger, naming the type and member

## Follow-up after this merges

Move `bliti-core-baseline` to the merge commit and add `"generate"` to its features. The revision
pinned now precedes the generator, so the baseline cannot be asked to produce messages live and
the baseline-to-current direction runs from the recorded snapshot alone. Moving it upgrades that
direction to live generation, at which point `baseline-snapshot.jsonl` has no further purpose and
is deleted whole.

Until that is done, a member removal is caught only where the snapshot carries an example of it.
It is broad enough to be a real check rather than a token one, but it is a snapshot and not a
search.

### Two artefacts, not one

The snapshot and the regression corpus were briefly the same directory of 240 anonymous files,
which conflated things with different lifecycles: the snapshot is scaffolding with a demolition
date, while a recorded regression is curated, named for what it pins, and permanent. Separated so
that the snapshot can be deleted in one action without taking anything curated with it, and so a
real regression is not lost among machine-generated neighbours.

proptest's own `*.proptest-regressions` files are ignored rather than committed, against its usual
advice. A seed replays only through the generator that found it, and that generator lives in
`bliti-core` and changes; the durable form is the message, written to the corpus as JSON.
