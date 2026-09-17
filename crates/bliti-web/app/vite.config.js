import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { VitePWA } from 'vite-plugin-pwa'

// The application is served from a hosted origin and built to static files (BLI-WEB). It is
// installable and works offline once loaded, so a phone that has opened it before is useful at a
// site with no connectivity; the wasm module is part of what is cached, because the protocol is in
// it and the application is nothing without it.
export default defineConfig({
	plugins: [
		react(),
		VitePWA({
			registerType: 'autoUpdate',
			workbox: { globPatterns: ['**/*.{js,css,html,wasm}'] },
			manifest: {
				name: 'bliti',
				short_name: 'bliti',
				description: 'Provision a device by the sticker on its enclosure',
				theme_color: '#1d4ed8',
				background_color: '#fbfbfa',
				display: 'standalone',
				start_url: '/',
			},
		}),
	],
	build: { target: 'es2022' },
})
