import { execSync } from 'node:child_process'
import { readdirSync } from 'node:fs'
import { resolve } from 'node:path'

import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

import pkg from './package.json' with { type: 'json' }
import serviceWorker from './sw-plugin.js'

const ROOT = import.meta.dirname

// Every HTML file at the top of the app is a page, served at its name without the extension (WEB), so
// adding one is adding the file. The test build carries one more, which the harness loads to cover
// what a second page needs of the build and the service worker.
const pagesOf = (mode) => [
	...readdirSync(ROOT).filter((file) => file.endsWith('.html')),
	...(mode === 'test' ? ['tests/second-page.html'] : []),
]

// The dev server tags the version with the commit it serves, as build metadata, so a page can be
// matched against the checkout behind it. A build is tagged with BLITI_BUILD where it names itself,
// as the daemon is, and otherwise reports the package version alone.
function devVersion() {
	try {
		const commit = execSync('git describe --always --dirty --exclude="*"', { encoding: 'utf8' }).trim()
		return `${pkg.version}+${commit}`
	} catch {
		return pkg.version
	}
}

function buildVersion() {
	return process.env.BLITI_BUILD ? `${pkg.version}+${process.env.BLITI_BUILD}` : pkg.version
}

export default defineConfig(({ command, mode }) => {
	const pages = pagesOf(mode)
	return {
		plugins: [react(), serviceWorker({ pages })],
		// Pages are files and nothing falls back to index.html, so a path that is no page is a 404 here as
		// it is deployed. The client keeps its screens in history state, not in the URL.
		appType: 'mpa',
		// The test build goes somewhere of its own, so it cannot overwrite the bundle CI uploads: the two
		// are deliberately different builds, because only one of them carries the harness's seam.
		build: {
			target: 'es2022',
			outDir: mode === 'test' ? 'dist-test' : 'dist',
			rollupOptions: { input: pages.map((page) => resolve(ROOT, page)) },
		},
		define: {
			// What this client reports itself as. Opaque to the device, which logs it (BLI-MSG).
			__APP_VERSION__: JSON.stringify(command === 'serve' ? devVersion() : buildVersion()),
			// The harness's seam for supplying its own client, built only under `--mode test`. A constant
			// false elsewhere, so the branch and the global it reads are gone from the shipped bundle.
			__TEST_SEAM__: JSON.stringify(mode === 'test'),
		},
	}
})
