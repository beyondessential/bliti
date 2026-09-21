# Optimise the browser client's wasm bundle size

The card changes how the browser client is built, not what it does, so most of the coverage is that
the protocol still behaves and that the smaller artefact is what a plain build produces.

## The protocol still works

- [x] The full Rust suite passes with snow's crypto named primitive by primitive rather than through
      the `default-resolver-crypto` umbrella (120 tests, including the handshake).
- [x] The daemon builds and its tests pass against the same narrowed `bliti-core`.
- [ ] A client and a device complete a handshake and exchange messages end to end, against a build
      carrying only the four named primitives.

## The build produces the smaller artefact by default

- [x] `just build-wasm` builds with the `wasm` profile and writes bindings Vite picks up.
- [x] `npm run build` drives the same wasm build through its `prebuild` hook and bundles the result.
- [ ] A fresh checkout can run `just setup` and then `just build` with no other instructions.
- [ ] CI's browser-client job installs `just`, reads the wasm-bindgen version from the lockfile, and
      builds the same artefact a laptop does.

## The artefact carries nothing it should not

- [x] The built module contains no absolute path from the building machine.
- [ ] The module loads in a browser and the bindings match the runtime compiled into it, confirming
      the lockfile-derived wasm-bindgen version is the right pair.
