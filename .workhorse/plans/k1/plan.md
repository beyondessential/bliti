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

## Open

- [ ] Generation library choice, and how optional members are reliably populated
- [ ] Whether `Refused` (critical member added) fails CI, warns, or needs explicit acknowledgement
- [ ] Upstream the containment walk: `envelope` has it privately as `collect_unknown` and exposes
      only the single-build `round_trip_omissions`
- [ ] How the baseline commit is recorded, and how it is moved
