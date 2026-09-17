import { defineConfig, devices } from '@playwright/test'

// The harness fakes at the message layer: decoded messages are fed to the application with no wasm
// and no Bluetooth in the loop. Nothing here pretends to be a Bluetooth stack, and protocol and
// transport coverage stays in the Rust tests.
export default defineConfig({
	testDir: './tests',
	fullyParallel: true,
	reporter: process.env.CI ? 'github' : 'list',
	use: { baseURL: 'http://localhost:4173', trace: 'on-first-retry' },
	projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
	webServer: {
		command: 'npm run build && npm run preview -- --port 4173 --strictPort',
		url: 'http://localhost:4173',
		reuseExistingServer: !process.env.CI,
		timeout: 120_000,
	},
})
