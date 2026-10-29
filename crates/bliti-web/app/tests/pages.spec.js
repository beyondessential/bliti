import { expect, test } from '@playwright/test'

import { emitState, openProvision, prompt, stateOf, step } from './fake-provision-client.js'

// A second page beside the operator app (WEB): built, served at its path, held by the service
// worker, and opened offline by the path it was navigated at. The provisioning client is that page.

const PAGE = '/provision'
const TITLE = 'Provision an Iti'

const controlled = (page) => page.waitForFunction(() => navigator.serviceWorker.controller !== null)

test('serves a page at its path, with its own script', async ({ page }) => {
	await openProvision(page)
	await expect(page.getByRole('heading', { name: TITLE })).toBeVisible()
	await expect(page).toHaveTitle(TITLE)
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
		await openProvision(page)
		await expect(page.getByRole('heading', { name: TITLE })).toBeVisible()
	})

	test('opens a page at its path with a trailing slash', async ({ page, context }) => {
		await page.goto('/')
		await controlled(page)

		await context.setOffline(true)
		await page.goto(`${PAGE}/`)
		await expect(page.getByRole('heading', { name: TITLE })).toBeVisible()
	})

	// The pictures of the board are held with the page, so an operator at a site with no connection sees
	// each where it belongs (PWEB, "Pictures").
	test('shows the pictures of the board', async ({ page, context }) => {
		await page.goto('/')
		await controlled(page)

		await context.setOffline(true)
		await openProvision(page, {
			opening: stateOf({
				run: 'waiting',
				steps: [
					step('hardware', { state: 'waiting' }),
					step('eeprom', { state: 'pending' }),
				],
				prompts: [prompt('case', 'hardware')],
			}),
		})
		const loaded = (image) => image.evaluate((img) => img.complete && img.naturalWidth > 0)
		await expect(page.locator('img.board')).toHaveCount(1)
		expect(await loaded(page.locator('img.board'))).toBe(true)

		await page.getByRole('button', { name: 'Find Iti' }).click()
		const picture = page.locator('img.board')
		await expect(picture).toHaveAttribute('alt', /CASE switch/)
		await expect.poll(() => loaded(picture)).toBe(true)

		await emitState(page, {
			run: 'waiting',
			steps: [step('hardware', { state: 'passed' }), step('eeprom', { state: 'waiting' })],
			prompts: [prompt('id-wp', 'eeprom')],
		})
		await expect(picture).toHaveAttribute('alt', /ID WP pads/)
		await expect.poll(() => loaded(picture)).toBe(true)
	})
})
