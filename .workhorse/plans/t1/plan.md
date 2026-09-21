# Optimise the browser client's wasm bundle size

## Goal

Shrink the browser client's shipping wasm artefact, measured before and after, with the smaller
artefact produced by the default build rather than behind a flag someone has to remember. Any
diagnostic capability given up is a decision taken knowingly.

## The baseline in the card description is the wrong artefact

The card's `1,081,989` raw / `293,340` gzipped matches the **cargo output**, not what ships.
`wasm-bindgen` gc's the module before Vite ever sees it, so the shipping artefact
(`app/src/wasm/bliti_web_bg.wasm`) starts at **633,460 raw / 216,746 gzip-9 / 168,057 brotli**.
Measured here: cargo output 1,061,813 raw / 294,791 gzip-9, within drift of the card's figures.

The premise of "a 1 MB bundle" overstates the shipping module by ~70%. All numbers below are the
shipping artefact.

## Measured results

Profile matrix (shipping artefact, bytes):

| config | raw | gzip -9 | brotli -q11 |
| --- | --- | --- | --- |
| baseline (`O3`, cgu 16, `lto` off) | 633,460 | 216,746 | 168,057 |
| `lto` + cgu 1 + `O3` | 573,551 | 207,640 | 162,598 |
| **`lto` + cgu 1 + `Os`** | **476,723** | **169,767** | **138,881** |
| `lto` + cgu 1 + `Oz` | 482,849 | 171,228 | 141,227 |
| `lto` + cgu 1 + `Os` + `panic=abort` | 476,366 | 169,777 | 138,967 |
| `lto` + cgu 1 + `Oz` + `panic=abort` | 482,289 | 171,079 | 141,171 |

Cumulative, on top of `lto` + cgu 1 + `Os`:

| config | raw | gzip -9 | brotli -q11 | zstd -19 |
| --- | --- | --- | --- | --- |
| A baseline as shipped | 633,460 | 216,746 | 168,057 | 180,640 |
| B profile (`lto`, cgu 1, `Os`) | 476,723 | 169,767 | 138,881 | 148,182 |
| **C + snow granular features** | **443,224** | **158,228** | **130,161** | **138,890** |
| D + hand-rolled base32 | 425,553 | 161,639 | 133,250 | 142,014 |

## Decisions settled by measurement

- **`opt-level = "s"`, not `"z"`.** `"s"` beats `"z"` by 1,461 gzipped bytes. The card's caution
  about the scratch-crate figure was right: `"z"` is not reliably smaller. `opt-level` is the
  dominant profile lever at 21.7% gzipped on its own; `lto` + cgu 1 adds ~4%.
- **No `panic = "abort"`.** It saves 149 gzipped bytes at `Oz` and is 10 bytes *worse* at `Os` —
  noise either way. There is no size-versus-diagnostics trade to make, so
  `console_error_panic_hook` and its readable panic messages stay.
- **No `wasm-opt`.** It cuts 10.8% raw but only 1.35% gzipped, because the bytes it removes are
  highly compressible. Not worth a required toolchain dependency for the build and CI. Dropped.
- **Snow's granular crypto features are the best value in the card.** The protocol fixes one
  pattern, `Noise_NNpsk0_25519_ChaChaPoly_BLAKE2s`, but `default-resolver-crypto` is an umbrella
  that also enables `use-aes-gcm` and `use-sha2`, compiling in SHA-256, SHA-512, BLAKE2b and AES
  that the handshake can never reach (snow dispatches over them at runtime, so nothing is statically
  dead). Selecting `use-chacha20poly1305`, `use-blake2`, `use-curve25519`, `use-getrandom` instead
  saves 11,539 gzipped bytes (6.8%) for a two-line Cargo.toml change with no diagnostic cost. It
  compresses nearly proportionally to its raw saving because it removes genuinely distinct code.
- **Keep `data-encoding`; do not hand-roll base32.** It is the largest single item in the module
  (`decode_mut` 30,354 B, `encode_mut` 14,040 B) and a hand-rolled RFC 4648 codec is 17,671 bytes
  smaller raw — but **3,411 bytes larger gzipped**. Its big functions are table-driven and
  repetitive, so they compress to almost nothing, while the replacement's bounds-check and panic
  strings do not. Reverted. The house rule preferring small dependencies over reimplementing the
  wheel holds here on the evidence.
- **Brotli, not zstd, for the wire.** On the config C artefact: brotli -q11 130,161; zstd -19
  138,890; zstd `--ultra -22 --long=27` 138,868 (22 bytes better than -19); a larger brotli window
  changes nothing. Brotli wins by 6.3%. zstd's advantage is decompression speed, not ratio, and this
  is a PWA downloaded once and then service-worker cached, so ratio is what counts.

## `.rodata` investigated: a dead end for size

Exact section breakdown of the config C artefact (parsing the wasm sections directly, which works on
a stripped binary):

| section | bytes | share |
| --- | --- | --- |
| `code` | 386,628 | 87.2% |
| `data` | 51,347 | 11.6% |
| everything else | 5,249 | 1.2% |

The earlier 66,630 figure was the names-kept baseline, a different build. The mass is code, not data:
eliminating the entire data section would take only 11.6% of raw, and far less compressed.

Measured on top of config C:

| config | raw | gzip -9 | brotli | build paths |
| --- | --- | --- | --- | --- |
| C reference | 443,224 | 158,228 | 130,161 | 76 |
| C + `--remap-path-prefix` | 440,088 | 158,077 | 130,149 | 0 |
| C + `futures` without `executor` | 443,224 | 158,233 | 130,391 | 76 |
| C + both | 440,088 | 158,079 | 130,158 | 0 |

- **`futures`' `executor` feature is already free.** Dropping it produces a byte-identical module:
  LTO already eliminates it. Not worth the Cargo.toml churn. (It also cannot be dropped by a member
  crate alone — `default-features = false` cannot override a workspace dependency's, so it would
  mean changing the workspace entry and re-adding `executor` to `bliti-core`'s dev-dependencies.)
- **`--remap-path-prefix` is worth doing, but not for size.** It removes all 76 absolute build paths
  for 3,136 raw bytes and **151 gzipped** (0.1%) — the shared prefixes compress almost perfectly.
  The reasons to do it are that the artefact currently embeds the building developer's home
  directory and username and ships them to every browser, and that builds are not reproducible
  across machines. Note `trim-paths`, the cargo-native option, is **not stabilised** (rejected by
  cargo 1.98.1), so this has to go through `RUSTFLAGS`.

## The pattern in every result

The only changes that moved the compressed number were the ones that removed genuinely distinct
code: `opt-level`, LTO's deduplication of monomorphisations, and snow's unreachable crypto.
Everything that removed redundant or repetitive bytes moved raw size and left the wire alone —
`wasm-opt` (10.8% raw, 1.35% gzipped), the base32 tables (smaller raw, *larger* gzipped), the build
paths (3,136 raw, 151 gzipped). Raw size is worth tracking for browser parse and compile time, but
it is not a proxy for what anyone downloads.

## Where the remaining mass sits

From `twiggy` on a names-kept build (`CARGO_PROFILE_RELEASE_STRIP=none`, needed because the
workspace `strip = "symbols"` removes the name section the profiler reads):

- `data_encoding` encode/decode — 44,394 B, but compresses away (see above)
- `.rodata` — 66,630 B
- `miniz_oxide` inflate/deflate — ~26,700 B, the zlib codec H1 added
- `snow` pattern parsing (`HandshakeTokens::try_from`) — 10,477 B, runtime string dispatch
- `core::fmt` float formatting (`flt2dec::dragon`) — 10,790 B, reached via `serde_json`'s `f64`.
  The protocol does carry floats, so this stays.
- duplicate monomorphisations under cgu 16 — `future_to_promise` 6,354 B twice, several `drop_glue`
  pairs; `lto` + cgu 1 collapses these

## Remaining work

- [ ] Add a `wasm` cargo profile (`inherits = "release"`, `lto = true`, `codegen-units = 1`,
      `opt-level = "s"`) and build the browser client with it.
- [ ] Switch `bliti-core`'s snow dependency to the granular `use-*` features.
- [ ] Add a justfile as the single definition of the build: `just setup` installs the toolchain
      (wasm32 target, `wasm-bindgen-cli` at the version pinned to the crate), `just build` produces
      the artefact. Fold `crates/bliti-web/build.sh` into it and have CI call `just` so local and CI
      builds cannot drift. AGENTS.md points at `just setup` rather than listing prerequisites.
- [ ] Add `--remap-path-prefix` for the registry, toolchain and workspace roots, so the artefact
      stops embedding the building developer's home directory and builds are reproducible.
- [ ] Consider pre-compressing the static bundle to brotli (and gzip as fallback) and serving with
      Caddy's precompressed support.
- [ ] Record the before/after on the card.

## Open questions

- Does the PWA service worker's precache need the compressed variants, or only the origin's
  `Content-Encoding` negotiation?
