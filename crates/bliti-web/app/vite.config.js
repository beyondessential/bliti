import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { VitePWA } from 'vite-plugin-pwa'

import pkg from './package.json' with { type: 'json' }

// The application is served from a hosted origin and built to static files (BLI-WEB). It is
// installable and works offline once loaded, so a phone that has opened it before is useful at a
// site with no connectivity; the wasm module is part of what is cached, because the protocol is in
// it and the application is nothing without it.
export default defineConfig(({ mode }) => ({
	plugins: [
		react(),
		VitePWA({
			registerType: 'autoUpdate',
			workbox: { globPatterns: ['**/*.{js,css,html,wasm}'] },
			// The manifest's own icons are precached for us. This one is reached from the markup
			// instead, so it has to be named, and it is worth caching because an installed
			// application may never be online again after the install.
			includeAssets: ['apple-touch-icon.png'],
			manifest: {
				name: 'bliti',
				short_name: 'bliti',
				description: 'Provision a device by the QR code on its enclosure',
				theme_color: '#1d4ed8',
				background_color: '#fbfbfa',
				display: 'standalone',
				start_url: '/',
				id: '/',
				// Chromium offers to install only once a 192px and a 512px icon are declared, so these
				// two are what makes the application installable rather than merely manifested. The
				// maskable pair is cropped to Android's safe zone; see icons.sh.
				icons: [
					{ src: '/icon-192.png', sizes: '192x192', type: 'image/png', purpose: 'any' },
					{ src: '/icon-512.png', sizes: '512x512', type: 'image/png', purpose: 'any' },
					{ src: '/icon-maskable-192.png', sizes: '192x192', type: 'image/png', purpose: 'maskable' },
					{ src: '/icon-maskable-512.png', sizes: '512x512', type: 'image/png', purpose: 'maskable' },
					{ src: '/icon.svg', sizes: 'any', type: 'image/svg+xml', purpose: 'any' },
				],
			},
		}),
	],
	// The test build goes somewhere of its own, so it cannot overwrite the bundle CI uploads: the two
	// are deliberately different builds, because only one of them carries the harness's seam.
	build: { target: 'es2022', outDir: mode === 'test' ? 'dist-test' : 'dist' },
	define: {
		// What this client reports itself as. Opaque to the device, which logs it (BLI-MSG).
		__APP_VERSION__: JSON.stringify(pkg.version),
		// The harness's seam for supplying its own client, built only under `--mode test`. A constant
		// false elsewhere, so the branch and the global it reads are gone from the shipped bundle.
		__TEST_SEAM__: JSON.stringify(mode === 'test'),
	},
}))
