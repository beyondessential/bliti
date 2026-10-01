import { execSync } from 'node:child_process'

import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

import pkg from './package.json' with { type: 'json' }
import serviceWorker from './sw-plugin.js'

// The dev server tags the version with the commit it serves, as build metadata, so a page can be
// matched against the checkout behind it. Built bundles report the package version alone.
function devVersion() {
	try {
		const commit = execSync('git describe --always --dirty --exclude="*"', { encoding: 'utf8' }).trim()
		return `${pkg.version}+${commit}`
	} catch {
		return pkg.version
	}
}

export default defineConfig(({ command, mode }) => ({
	plugins: [react(), serviceWorker()],
	// The test build goes somewhere of its own, so it cannot overwrite the bundle CI uploads: the two
	// are deliberately different builds, because only one of them carries the harness's seam.
	build: { target: 'es2022', outDir: mode === 'test' ? 'dist-test' : 'dist' },
	define: {
		// What this client reports itself as. Opaque to the device, which logs it (BLI-MSG).
		__APP_VERSION__: JSON.stringify(command === 'serve' ? devVersion() : pkg.version),
		// The harness's seam for supplying its own client, built only under `--mode test`. A constant
		// false elsewhere, so the branch and the global it reads are gone from the shipped bundle.
		__TEST_SEAM__: JSON.stringify(mode === 'test'),
	},
}))
