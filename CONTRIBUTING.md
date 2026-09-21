# bliti Contributions Guide

Thank you for taking the time to contribute, and welcome to this open-source project!

## Code of Conduct

This project and everyone participating in it is governed by the
[BES International Open Source Code of Conduct](CODE_OF_CONDUCT.md).
By participating, you are expected to uphold this code. Please report unacceptable behavior
to [opensource@tamanu.io](opensource@tamanu.io).

## Commit Convention

The subject (first line) of commit messages must be in [Conventional Commit](https://www.conventionalcommits.org/en/v1.0.0/)
format. This is used for version bumps on releases and also for general historical purposes.

```plain
type: <description>
type(scope): <description>
```

## The wire protocol

`bliti-wire-compat` checks the wire protocol against a baseline build, in the same spirit as
`cargo-semver-checks` for a Rust API: the baseline and the current build are linked together and
each reads what the other writes, so nothing describes the wire shape and there is no schema to
drift. It runs as part of `cargo test`.

Two things it may ask of you.

**A member older peers will refuse.** A member named in upper case is critical, and adding one is
permitted without moving the version marker, but every peer older than the change stops acting on
that message type. Record it in `wire-breaks.toml` with a reason. The check fails until you do.

**Moving the baseline.** The baseline is the `rev` of `bliti-core-baseline` in the workspace
`Cargo.toml`. Before the first release, breaking the protocol is free, so a deliberate break moves
it. Afterwards, moving it means moving the version marker of VER too.

Repurposing a member, keeping its name and type while changing its meaning, is not machine
checkable and remains a matter for review.

## License

Any contributions you make will be licensed under [the General Public License version 3.0](./COPYING).
