import { copyFile, readFile, rm, writeFile } from 'node:fs/promises'

import { expect, test } from '@playwright/test'

// Usable offline once loaded (WEB). The wasm module is fetched lazily, on the first code read, so a
// page that loaded while online can still be the one that fetches it after the connection is gone.

const DIST = new URL('../dist-test/', import.meta.url)

const wasmPath = async (request) => {
	const sw = await (await request.get('/sw.js')).text()
	return `/${sw.match(/assets\/bliti_web_bg-[\w-]+\.wasm/)[0]}`
}

const controlled = (page) => page.waitForFunction(() => navigator.serviceWorker.controller !== null)

const fetchOffline = (page, path) =>
	page.evaluate(async (path) => {
		try {
			const res = await fetch(path)
			return res.ok ? (await res.arrayBuffer()).byteLength > 0 : `status ${res.status}`
		} catch (error) {
			return String(error)
		}
	}, path)

test('serves the wasm module offline once loaded', async ({ page, context, request }) => {
	const wasm = await wasmPath(request)
	await page.goto('/')
	await controlled(page)

	await context.setOffline(true)
	await page.reload()
	await expect(page.locator('#root')).not.toBeEmpty()
	expect(await fetchOffline(page, wasm)).toBe(true)
})

test('keeps an open page’s wasm module across an update installed behind it', async ({
	page,
	context,
	request,
}) => {
	const wasm = await wasmPath(request)
	await page.goto('/')
	await controlled(page)

	// The next version is the same worker with the module renamed, as a rebuilt module would be, so
	// the entry the open page needs is one the next version does not list.
	const id = test.info().testId
	const nextWasm = `assets/next-${id}.wasm`
	const nextWorker = `sw-next-${id}.js`
	const sw = await readFile(new URL('sw.js', DIST), 'utf8')
	await copyFile(new URL(wasm.slice(1), DIST), new URL(nextWasm, DIST))
	await writeFile(new URL(nextWorker, DIST), sw.replace(wasm.slice(1), nextWasm))

	try {
		const state = await page.evaluate(async (url) => {
			const reg = await navigator.serviceWorker.register(url, { scope: '/' })
			const worker = reg.installing
			await new Promise((resolve) => {
				const settle = () => worker.state !== 'installing' && resolve()
				worker.addEventListener('statechange', settle)
				settle()
			})
			// A worker that takes over does so as soon as it has installed; one that waits never
			// changes the controller, so this bounds only the passing path.
			await new Promise((resolve) => {
				navigator.serviceWorker.addEventListener('controllerchange', resolve)
				setTimeout(resolve, 2000)
			})
			return worker.state
		}, `/${nextWorker}`)
		expect(state).toBe('installed')

		await context.setOffline(true)
		expect(await fetchOffline(page, wasm)).toBe(true)
	} finally {
		await rm(new URL(nextWasm, DIST), { force: true })
		await rm(new URL(nextWorker, DIST), { force: true })
	}
})
