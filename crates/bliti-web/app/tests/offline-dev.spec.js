import { readFile, readdir, rm, writeFile } from 'node:fs/promises'

import { expect, test } from '@playwright/test'

// The worker the dev server serves (WEB): it precaches the module graph, so the dev client works
// offline after one load online, and answers from the network while it can, so edits still show.
// Some of these edit the application's own source, so they run one at a time.

test.describe.configure({ mode: 'serial' })

const SRC = new URL('../src/', import.meta.url)
const PUBLIC = new URL('../public/', import.meta.url)

const listOf = async (request) => {
	const sw = await (await request.get('/sw.js')).text()
	return { sw, list: JSON.parse(sw.match(/\{"networkFirst".*?\]\}/)[0]) }
}

const controlled = (page) => page.waitForFunction(() => navigator.serviceWorker.controller !== null)

const fetchText = (page, path) =>
	page.evaluate(async (path) => {
		try {
			const res = await fetch(path)
			return res.ok ? await res.text() : `status ${res.status}`
		} catch (error) {
			return String(error)
		}
	}, path)

// An edit to a source file, undone however the test ends. The dev server sees the change through
// its watcher, so a test waits on what it serves rather than on the write.
async function editing(file, change, body) {
	const url = new URL(file, SRC)
	const original = await readFile(url, 'utf8')
	await writeFile(url, change(original))
	try {
		await body()
	} finally {
		await writeFile(url, original)
	}
}

test('the dev worker precaches the module graph and public/, answered from the network first', async ({
	request,
}) => {
	const { list } = await listOf(request)
	expect(list.networkFirst).toBe(true)
	expect(list.entries.every((entry) => entry.revision === null)).toBe(true)
	const urls = list.entries.map((entry) => entry.url)
	for (const url of ['/', '/@vite/client', '/src/main.jsx', '/src/wasm/bliti_web_bg.wasm']) {
		expect(urls).toContain(url)
	}
	for (const file of await readdir(PUBLIC)) expect(urls).toContain(`/${file}`)
})

test.describe('with no service worker to see the load', () => {
	test.use({ serviceWorkers: 'block' })

	test('lists every URL a load asks the dev server for', async ({ page, request }) => {
		const asked = []
		page.on('request', (req) => {
			const url = new URL(req.url())
			asked.push(url.pathname + url.search)
		})
		await page.goto('/')
		await expect(page.locator('#root')).not.toBeEmpty()
		await page.waitForLoadState('networkidle')
		const urls = new Set((await listOf(request)).list.entries.map((entry) => entry.url))
		expect(asked.filter((url) => !urls.has(url))).toEqual([])
	})
})

test('keeps its bytes across an edit that changes no imports', async ({ request }) => {
	const { sw } = await listOf(request)
	expect((await listOf(request)).sw).toBe(sw)
	await editing(
		'path.js',
		(source) => `${source}\n// edited\n`,
		async () => {
			await expect
				.poll(async () => (await request.get('/src/path.js')).text())
				.toContain('// edited')
			expect((await listOf(request)).sw).toBe(sw)
		},
	)
})

test('works offline after one load online', async ({ page, context }) => {
	await page.goto('/')
	await controlled(page)

	await context.setOffline(true)
	await page.reload()
	await expect(page.locator('#root')).not.toBeEmpty()
	expect(await fetchText(page, '/src/wasm/bliti_web_bg.wasm')).not.toMatch(/^(status|TypeError)/)
})

test('answers a precached module from the network while online', async ({ page, context }) => {
	await page.goto('/')
	await controlled(page)
	expect(await fetchText(page, '/src/path.js')).not.toContain('// marked')
	await editing(
		'path.js',
		(source) => `${source}\n// marked\n`,
		async () => {
			await expect.poll(() => fetchText(page, '/src/path.js')).toContain('// marked')
			await context.setOffline(true)
			expect(await fetchText(page, '/src/path.js')).toContain('// marked')
		},
	)
})

test('answers anything else from the network while online, and its latest offline', async ({
	page,
	context,
}) => {
	const probe = new URL(`__probe-${test.info().testId}.js`, SRC)
	const path = `/src/${probe.pathname.split('/').pop()}`
	await writeFile(probe, 'export default "first"\n')
	try {
		await page.goto('/')
		await controlled(page)
		expect(await fetchText(page, path)).toContain('first')
		await writeFile(probe, 'export default "second"\n')
		await expect.poll(() => fetchText(page, path)).toContain('second')
		await context.setOffline(true)
		expect(await fetchText(page, `${path}?t=1`)).toContain('second')
	} finally {
		await rm(probe, { force: true })
	}
})

test('takes an edit live, and keeps it for an offline reload', async ({ page, context }) => {
	await page.goto('/')
	await controlled(page)
	const heading = page.locator('h1').first()
	await expect(heading).toHaveText('bliti')
	await page.evaluate(() => {
		window.__stayed = true
	})
	const title = `bliti ${test.info().testId}`
	await editing(
		'App.jsx',
		(source) => source.replaceAll('<h1>bliti</h1>', `<h1>${title}</h1>`),
		async () => {
			await expect(heading).toHaveText(title)
			expect(await page.evaluate(() => window.__stayed)).toBe(true)

			await context.setOffline(true)
			await page.reload()
			await expect(page.locator('h1').first()).toHaveText(title)
		},
	)
})
