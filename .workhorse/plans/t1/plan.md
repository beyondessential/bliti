# Optimise the browser client's wasm bundle size

## Goal

Shrink the browser client's shipping wasm artefact, measured before and after, with the smaller
artefact produced by the default build rather than behind a flag someone has to remember. Any
diagnostic capability given up is a decision taken knowingly.

Baseline (from the card, to be reproduced on this machine): 1,081,989 bytes raw, 293,340 gzipped.

## Decisions settled in workshop

- **The justfile is the single definition of the build.** `just setup` installs the toolchain
  (wasm32 target, `wasm-bindgen-cli` at the pinned version, `wasm-opt`, and the profiler); `just
  build` produces the shipping artefact. Both a dev and CI run these, so there is no drift between
  what is built locally and what CI uploads. `crates/bliti-web/build.sh`'s logic folds into a
  `just build` recipe and CI calls `just` in its place. AGENTS.md drops the specific prerequisite
  list and points at `just setup`.
- **`wasm-opt` is a required build prerequisite**, installed by `just setup`. The build runs it
  unconditionally so the smaller artefact is the default output.
- **`panic = "abort"` is deferred**: hold the diagnostics-vs-size trade (dropping
  `console_error_panic_hook`'s readable messages) until profiling says how many bytes the panic
  machinery actually costs.

## Approach / sequencing

Measurement points the rest, so it comes first.

- [ ] Reproduce the baseline on this machine (raw + gzipped) so before/after are the same
      measurement. Pin how gzipped is measured; note what a CDN would actually serve (brotli).
- [ ] Profile the shipping (wasm-bindgen output) module with the name section kept, to see where
      the mass sits: profile settings, `serde_json`, `snow`, or somewhere unexpected.
- [ ] Add a size-tuned wasm cargo profile (`lto`, `codegen-units = 1`, `opt-level` — measure `"z"`
      vs `"s"`, "z" is not reliably smaller) and wire it into the build.
- [ ] Run `wasm-opt -Oz` on the wasm-bindgen output.
- [ ] Revisit `panic = "abort"` with the profiling numbers in hand; decide knowingly.
- [ ] Record the before/after in the card.

## Open questions

- How was the card's 293,340 gzipped figure produced (gzip level)? Reproduce and pin.
- Does `wasm-opt` warrant a version pin for reproducible before/after, and how is it installed by
  `just setup` on a fresh machine?
