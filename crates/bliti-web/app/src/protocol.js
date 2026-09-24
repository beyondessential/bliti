// Loading the wasm module, which carries everything with protocol in it (BLI-WEB).
//
// The client needs it before it reads a code, and the network screen before it checks a document,
// because the pre-proposal check of BLI-NSCR is the device's own checker compiled in. Loaded once, at
// startup; both of those wait on the same load, and retry it where it failed.

import init, { start } from './wasm/bliti_web.js'
import wasmUrl from './wasm/bliti_web_bg.wasm?url'

let loaded

export async function loadProtocol() {
	// The failure is not cached along with the success: a wasm fetch that fails once, on a flaky
	// network before the service worker has cached it, would otherwise leave every later call
	// rethrowing the same stale error with no way back short of a reload.
	loaded ??= init({ module_or_path: wasmUrl })
		.then(() => {
			start()
		})
		.catch((error) => {
			loaded = undefined
			throw error
		})
	await loaded
}
