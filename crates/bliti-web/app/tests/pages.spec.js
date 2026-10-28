import { expect, test } from '@playwright/test'

// A second page beside the operator app (WEB): built, served at its path, held by the service
// worker, and opened offline by the path it was navigated at. The fixture page stands in for the
// provisioning client, which does not exist yet; the build carries it under `--mode test` alone.

const PAGE = '/tests/second-page'

const controlled = (page) => page.waitForFunction(() => navigator.serviceWorker.controller !== null)

test('serves a page at its path, with its own script', async ({ page }) => {
	await page.goto(PAGE)
	await expect(page.locator('#second-page')).toHaveText('Second page loaded')
	await expect(page.locator('#root')).toHaveCount(0)
})

test('answers a path that is no page with a 404, not the operator app', async ({ page }) => {
	const res = await page.goto('/no-such-page')
	expect(res.status()).toBe(404)
})

test('answers a path that is no page with a 404 where a worker controls the origin', async ({
	page,
}) => {
	await page.goto('/')
	await controlled(page)
	const res = await page.goto('/no-such-page')
	expect(res.status()).toBe(404)
})

test.describe('offline after one load of the operator app', () => {
	test('opens a page at its path', async ({ page, context }) => {
		await page.goto('/')
		await controlled(page)

		await context.setOffline(true)
		await page.goto(PAGE)
		await expect(page.locator('#second-page')).toHaveText('Second page loaded')
	})

	test('opens a page at its path with a trailing slash', async ({ page, context }) => {
		await page.goto('/')
		await controlled(page)

		await context.setOffline(true)
		await page.goto(`${PAGE}/`)
		await expect(page.locator('#second-page')).toHaveText('Second page loaded')
	})
})
