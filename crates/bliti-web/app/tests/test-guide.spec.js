import { expect, test } from '@playwright/test'

import { heading, numbers, panel, tickParts } from './guide.js'

// The testing guide (ASM): a page of its own at /test, with its own parts, ending with the Iti marked
// tested in inventory.

const PAGE = '/test'
const TITLE = 'Test an Iti'

const controlled = (page) => page.waitForFunction(() => navigator.serviceWorker.controller !== null)

test('serves the guide at its path, titled Test an Iti', async ({ page }) => {
	await page.goto(PAGE)
	await expect(page).toHaveTitle(TITLE)
	await expect(page.getByRole('heading', { level: 1, name: TITLE })).toBeVisible()
})

test('starts with its own parts', async ({ page }) => {
	await page.goto(PAGE)
	const parts = page.locator('.parts label')
	await expect(parts).toHaveText([
		'The assembled Iti',
		/USB-C cable and power/,
		'Network cable',
		'Wi-Fi network name and password',
		'A phone with Bluetooth and Chrome',
	])
})

test('ends with marking the Iti as tested in inventory', async ({ page }) => {
	await page.goto(PAGE)
	await expect(page.locator('.build > li').last()).toContainText('Mark the Iti as tested in inventory')
})

test('links to the operator app where it has the reader connect to the Iti', async ({ page }) => {
	await page.goto(PAGE)
	await expect(page.locator('.build').getByRole('link', { name: 'operator app' })).toHaveAttribute('href', '/')
})

test.describe('the steps', () => {
	test('hold the first step until the Iti, the power and the cable are ticked', async ({ page }) => {
		await page.goto(PAGE)
		await expect(numbers(page).nth(0)).toBeDisabled()
		await page.locator('#t-iti').check()
		await page.locator('#t-psu').check()
		await expect(numbers(page).nth(0)).toBeDisabled()
		await page.locator('#t-net').check()
		await expect(numbers(page).nth(0)).toBeEnabled()
	})

	test('are done in order, the first open and the rest closed', async ({ page }) => {
		await page.goto(PAGE)
		await tickParts(page)
		await expect(panel(page, 0)).toHaveJSProperty('open', true)
		await expect(numbers(page).nth(1)).toBeDisabled()
		await numbers(page).nth(0).click()
		await expect(numbers(page).nth(1)).toBeEnabled()
		await expect(panel(page, 1)).toHaveJSProperty('open', true)
		await expect(panel(page, 0)).toHaveJSProperty('open', false)
	})

	test('open when their line is clicked, to read ahead', async ({ page }) => {
		await page.goto(PAGE)
		await heading(page, 3).click()
		await expect(panel(page, 3)).toHaveJSProperty('open', true)
	})

	test('hold the test procedure until the Wi-Fi details are ticked', async ({ page }) => {
		await page.goto(PAGE)
		await tickParts(page)
		await page.locator('#t-wifi').uncheck()
		const count = await numbers(page).count()
		for (let i = 0; i < count - 2; i++) await numbers(page).nth(i).click()
		await expect(numbers(page).nth(count - 2)).toBeDisabled()
		await page.locator('#t-wifi').check()
		await expect(numbers(page).nth(count - 2)).toBeEnabled()
	})
})

test.describe('starting over', () => {
	test('is offered once every step is done', async ({ page }) => {
		await page.goto(PAGE)
		await tickParts(page)
		const again = page.getByRole('button', { name: 'Do a new one' })
		await expect(again).toBeDisabled()
		const count = await numbers(page).count()
		for (let i = 0; i < count; i++) await numbers(page).nth(i).click()
		await expect(again).toBeEnabled()
	})

	test('clears the steps and the Iti, and keeps what the reader has for testing', async ({ page }) => {
		await page.goto(PAGE)
		await tickParts(page)
		const count = await numbers(page).count()
		for (let i = 0; i < count; i++) await numbers(page).nth(i).click()
		await page.getByRole('button', { name: 'Do a new one' }).click()
		await expect(page.locator('.build > li.done')).toHaveCount(0)
		await expect(page.locator('#t-iti')).not.toBeChecked()
		for (const id of ['t-psu', 't-net', 't-wifi', 't-phone']) await expect(page.locator(`#${id}`)).toBeChecked()
	})

	test('has no tile or count beside it', async ({ page }) => {
		await page.goto(PAGE)
		await expect(page.getByText('steps done')).toHaveCount(0)
		const border = await page.locator('.again').evaluate((el) => getComputedStyle(el).borderTopWidth)
		expect(border).toBe('0px')
	})
})

test.describe('offline after one load of the operator app', () => {
	test('opens the guide at its path, with and without a trailing slash', async ({ page, context }) => {
		await page.goto('/')
		await controlled(page)
		await context.setOffline(true)
		for (const path of [PAGE, `${PAGE}/`]) {
			await page.goto(path)
			await expect(page.getByRole('heading', { level: 1, name: TITLE })).toBeVisible()
		}
	})
})
