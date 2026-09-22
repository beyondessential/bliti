import { expect, test } from '@playwright/test'

// What a browser needs in order to offer the install (WEB). None of this is observable from the
// application's own interface, and a manifest missing any of it still links and still runs, so the
// only way it stays met is a test that reads the manifest the build actually emitted.

const DISPLAY_MODES = ['fullscreen', 'standalone', 'minimal-ui']

const manifest = async (request) => {
	const res = await request.get('/manifest.webmanifest')
	expect(res.status()).toBe(200)
	return { doc: await res.json() }
}

const pngIcons = (doc, purpose) =>
	doc.icons.filter(
		(i) =>
			i.type === 'image/png' &&
			(i.purpose ?? 'any').split(/\s+/).includes(purpose),
	)

const square = (icon) => {
	const sizes = icon.sizes.split(/\s+/).map((s) => s.split('x').map(Number))
	return sizes.filter(([w, h]) => w === h).map(([w]) => w)
}

test.describe('the manifest', () => {
	test('declares the name, start URL and display mode the install needs', async ({ request }) => {
		const { doc } = await manifest(request)
		expect(doc.name || doc.short_name).toBeTruthy()
		expect(doc.start_url).toBeTruthy()
		expect(DISPLAY_MODES).toContain(doc.display)
		// A stated preference for a native application is a refusal of the install.
		expect(doc.prefer_related_applications ?? false).toBe(false)
	})

	test('declares a 192px and a 512px square PNG icon', async ({ request }) => {
		const { doc } = await manifest(request)
		const sizes = pngIcons(doc, 'any').flatMap(square)
		expect(Math.max(...sizes, 0)).toBeGreaterThanOrEqual(512)
		expect(sizes.some((s) => s >= 192 && s < 512)).toBe(true)
	})

	test('declares a maskable icon', async ({ request }) => {
		const { doc } = await manifest(request)
		expect(pngIcons(doc, 'maskable').flatMap(square)).toContain(512)
	})

	test('serves every icon it declares, at the size it claims', async ({ request }) => {
		const { doc } = await manifest(request)
		expect(doc.icons.length).toBeGreaterThan(0)
		for (const icon of doc.icons) {
			const res = await request.get(icon.src)
			expect(res.status(), `${icon.src} is served`).toBe(200)
			expect(res.headers()['content-type']).toContain(icon.type.split('/')[1].replace('+xml', ''))
			if (icon.type === 'image/png') {
				// PNG carries its dimensions big-endian at a fixed offset in the IHDR chunk.
				const body = await res.body()
				const width = body.readUInt32BE(16)
				const height = body.readUInt32BE(20)
				expect([width, height], `${icon.src} is ${icon.sizes}`).toEqual([
					square(icon)[0],
					square(icon)[0],
				])
			}
		}
	})
})

test.describe('the application', () => {
	test('links the manifest and a home-screen icon for platforms that ignore it', async ({
		page,
		request,
	}) => {
		await page.goto('/')
		await expect(page.locator('link[rel="manifest"]')).toHaveAttribute(
			'href',
			/manifest\.webmanifest$/,
		)
		const apple = page.locator('link[rel="apple-touch-icon"]')
		await expect(apple).toHaveCount(1)
		const res = await request.get(await apple.getAttribute('href'))
		expect(res.status()).toBe(200)
	})

	test('registers a service worker that serves its own assets', async ({ page, request }) => {
		expect((await request.get('/sw.js')).status()).toBe(200)
		await page.goto('/')
		await expect
			.poll(
				() => page.evaluate(() => navigator.serviceWorker.getRegistrations().then((r) => r.length)),
				{ timeout: 15_000 },
			)
			.toBeGreaterThan(0)
	})
})
