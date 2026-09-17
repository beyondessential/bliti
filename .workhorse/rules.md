# Spec and plan rules

Specs live in `.workhorse/specs/<name>.md`; plans live in `.workhorse/plans/`.

For the spec file format, the frontmatter `id`, code-to-spec traceability, the fold/create/split decision, and when a change warrants a spec update, follow `.agents/docs/spec-format.md` and the Workhorse skills.
This file records only the house conventions that sit on top of that.

A spec is the durable description of **what** the system requires.
It is read by someone deciding whether the implementation is correct, or re-implementing the feature from scratch — not as a narrative of how the code works or how it came to be.

## House style

Specs are written in markdown prose with each sentence on its own line and no hard-wrapping, rather than the checkbox acceptance-criteria style shown in `spec-format.md`.
This balances ease of writing and diff parseability.
Use a standards/RFC spec voice, as in RFC 2119.
Do not include justifications unless the justification is critical to a spec.

## Cross-references

Specs reference each other with markdown links under the target's id, e.g. `[BAK](backup.md)`.
Code references the spec it implements with an inline `// spec: BAK` comment, as described in `spec-format.md`.

## What, not how

- Describe **what** the system requires, not **how** the code achieves it.
  Keep out of spec text: tool and command names (`sfdisk`, `kopia snapshot create`), crate and library names, syscall names (`splice(2)`), internal API details, data-structure choices, and environment variable names used only by the implementation.
- Acceptable, because external actors or other components depend on them: interface contracts — config file paths and formats, on-disk and on-the-wire shapes, endpoint shapes, partition UUIDs, credential scopes.
- The test: would someone re-implementing the feature from scratch be constrained to the same choice?
  If not, it's an implementation detail and doesn't belong in the spec.
  Note that wherever contracts exist, enough must be included in the spec such that someone can write a *compatible* re-implementation from scratch.

## Present, not past

- State what the system does, not how it got there.
  No "this supersedes X", "formerly Y", "a spike settled Z", changelog entries, or migration narration.
- When something is removed or replaced, edit the spec to describe the new reality and delete the old text, rather than describing the transition.
  The git history is the record of change; the spec is the record of the present.

## Own behaviour, not a dependency's internals

- Describe the system's own behaviour and contracts: the request shape it handles, the guarantee it makes.
  Don't narrate a dependency's decision logic or version-specific quirks beyond the minimum needed to justify a requirement.
- Don't scaffold or label: no "Strategy A/B", "Phase N", or plan tags in spec prose.
  Describe the mechanism directly.

