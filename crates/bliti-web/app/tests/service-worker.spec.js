import { readFile, readdir, rm, writeFile } from 'node:fs/promises'

import { expect, test } from '@playwright/test'

import { inject } from '../sw-plugin.js'

// How the built worker fills, replaces and clears its precache (WEB). Each test that stands up a
// next version writes it beside the bundle under a name of its own, and removes it after.

const DIST = new URL('../dist-test/', import.meta.url)
const PUBLIC = new URL('../public/', import.meta.url)
const OTHERS = /(^|\/)(sw-)?next-/

const built = async () => {
	const sw = await readFile(new URL('sw.js', DIST), 'utf8')
	return { sw, list: JSON.parse(sw.match(/\{"networkFirst".*?\]\}/)[0]) }
}

const controlled = (page) => page.waitForFunction(() => navigator.serviceWorker.controller !== null)

const precaches = (page) =>
	page.evaluate(async () => (await caches.keys()).filter((name) => name.startsWith('bliti-precache-')))

// Registers a next version and resolves with the state its install settles in.
const install = (page, url) =>
	page.evaluate(async (url) => {
		const reg = await navigator.serviceWorker.register(url, { scope: '/' })
		const worker = reg.installing
		return new Promise((resolve) => {
			const settle = () => worker.state !== 'installing' && resolve(worker.state)
			worker.addEventListener('statechange', settle)
			settle()
		})
	}, url)

async function withNext(change, body) {
	const name = `sw-next-${test.info().testId}.js`
	await writeFile(new URL(name, DIST), change((await built()).sw))
	try {
		await body(`/${name}`)
	} finally {
		await rm(new URL(name, DIST), { force: true })
	}
}

// A next version in which the document alone has changed.
const documentChanged = (sw) =>
	sw.replace(/("url":"\/","revision":")[0-9a-f]+"/, `$1${'0'.repeat(16)}"`)

test('lists every file of the bundle and of public/, each with a revision', async () => {
	const { list } = await built()
	expect(list.networkFirst).toBe(false)
	expect(list.entries.every((entry) => /^[0-9a-f]{16}$/.test(entry.revision))).toBe(true)

	const bundled = (await readdir(DIST, { recursive: true }))
		.filter((file) => /\.(js|css|html|wasm)$/.test(file) && file !== 'sw.js' && !OTHERS.test(file))
		.map((file) => (file === 'index.html' ? '/' : `/${file}`))
	const published = (await readdir(PUBLIC)).map((file) => `/${file}`)
	expect(list.entries.map((entry) => entry.url).sort()).toEqual([...bundled, ...published].sort())
})

test('refuses a worker source that carries no placeholder for its list', () => {
	expect(() => inject('self.addEventListener("fetch", () => {})', { entries: [] })).toThrow()
})

test('controls a first visit once it activates, without a reload', async ({ page }) => {
	await page.goto('/')
	await page.evaluate(() => {
		window.__stayed = true
	})
	await controlled(page)
	expect(await page.evaluate(() => window.__stayed)).toBe(true)
})

test('lets a POST through to the network', async ({ page }) => {
	await page.goto('/')
	await controlled(page)
	const posted = page.waitForResponse((res) => res.request().method() === 'POST')
	await page.evaluate(() => fetch('/', { method: 'POST' }).catch(() => {}))
	expect((await posted).fromServiceWorker()).toBe(false)
})

test('an update fetches only what changed, and copies the rest from the precache it replaces', async ({
	page,
	context,
}) => {
	const { list } = await built()
	const listed = new Set(list.entries.map((entry) => entry.url))
	await page.goto('/')
	await controlled(page)

	const fetched = []
	context.on('request', (req) => {
		const url = new URL(req.url())
		if (req.serviceWorker() && listed.has(url.pathname)) fetched.push(url.pathname)
	})
	await withNext(documentChanged, async (next) => {
		expect(await install(page, next)).toBe('installed')
		expect(fetched).toEqual(['/'])
	})
})

test('an update that changes the worker and not its list leaves the precache in use as it is', async ({
	page,
	context,
}) => {
	const { list } = await built()
	const listed = new Set(list.entries.map((entry) => entry.url))
	await page.goto('/')
	await controlled(page)
	const before = await precaches(page)

	const fetched = []
	context.on('request', (req) => {
		const url = new URL(req.url())
		if (req.serviceWorker() && listed.has(url.pathname)) fetched.push(url.pathname)
	})
	await withNext(
		(sw) => `${sw}\n// the next version\n`,
		async (next) => {
			expect(await install(page, next)).toBe('installed')
			expect(fetched).toEqual([])
			expect(await precaches(page)).toEqual(before)
		},
	)
})

test('an update that cannot fill its precache never activates, and leaves nothing behind', async ({
	page,
	context,
}) => {
	const { list } = await built()
	const wasm = list.entries.find((entry) => entry.url.endsWith('.wasm')).url
	await page.goto('/')
	await controlled(page)
	const before = await precaches(page)

	// The preview server answers a missing file with the page, where an origin answers 404.
	const missing = `/assets/next-missing-${test.info().testId}.wasm`
	await context.route(`**${missing}`, (route) => route.fulfill({ status: 404 }))
	await withNext(
		(sw) => sw.replace(wasm, missing),
		async (next) => {
			expect(await install(page, next)).toBe('redundant')
			expect(await precaches(page)).toEqual(before)

			await context.setOffline(true)
			const ok = await page.evaluate(async (wasm) => (await fetch(wasm)).ok, wasm)
			expect(ok).toBe(true)
		},
	)
})

test('an update that activates clears the precache it replaces and keeps the runtime cache', async ({
	page,
	context,
}) => {
	await page.goto('/')
	await controlled(page)
	const [before] = await precaches(page)

	await withNext(documentChanged, async (next) => {
		// Anything off the list is kept in the runtime cache; the next version's own script will do.
		expect(await page.evaluate(async (next) => (await fetch(next)).ok, next)).toBe(true)
		expect(await install(page, next)).toBe('installed')

		// The next version waits for the last page of this one to go. The page that comes next would
		// register /sw.js again, installing this version's list anew behind the next one.
		await context.route('**/sw.js', (route) => route.abort())
		await page.close()
		const again = await context.newPage()
		await again.goto('/')
		await again.waitForFunction(
			(next) => navigator.serviceWorker.controller?.scriptURL.endsWith(next),
			next,
		)
		const after = await precaches(again)
		expect(after).toHaveLength(1)
		expect(after[0]).not.toBe(before)
		const runtime = await again.evaluate(async (next) => {
			const cache = await caches.open('bliti-runtime')
			return (await cache.match(next)) !== undefined
		}, next)
		expect(runtime).toBe(true)
	})
})
