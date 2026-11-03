import { expect, test } from '@playwright/test'

// The shipping guide (ASM): a page of its own at /ship.

const PAGE = '/ship'
const TITLE = 'Ship an Iti'

const controlled = (page) => page.waitForFunction(() => navigator.serviceWorker.controller !== null)

test('serves the guide at its path, titled Ship an Iti', async ({ page }) => {
	await page.goto(PAGE)
	await expect(page).toHaveTitle(TITLE)
	await expect(page.getByRole('heading', { level: 1, name: TITLE })).toBeVisible()
})

test('links to the assembly guide and the testing guide', async ({ page }) => {
	await page.goto(PAGE)
	await expect(page.getByRole('link', { name: 'Assemble', exact: true })).toHaveAttribute('href', '/assemble')
	await expect(page.getByRole('link', { name: 'Test it' })).toHaveAttribute('href', '/test')
})

test('does not open by saying a tested Iti can wait in storage', async ({ page }) => {
	await page.goto(PAGE)
	await expect(page.getByText('can wait in storage')).toHaveCount(0)
})

test('says what it needs, with the supply required', async ({ page }) => {
	await page.goto(PAGE)
	const needs = page.locator('section', { has: page.getByRole('heading', { name: 'You need' }) })
	await expect(needs).toContainText('The finished, tested Iti')
	await expect(needs).toContainText('A 27W power supply')
	await expect(needs).not.toContainText('Optional')
})

test('marks only preparing the Iti for its destination as optional', async ({ page }) => {
	await page.goto(PAGE)
	const prepare = page.locator('section', { has: page.getByRole('heading', { name: 'Prepare it for its destination' }) })
	await expect(prepare).toContainText('Optional, but recommended.')
	const send = page.locator('section', { has: page.getByRole('heading', { name: 'Send it' }) })
	await expect(send).not.toContainText('Optional')
	await expect(send).toContainText('Check the Iti has its sticker')
	await expect(send.locator('label')).toHaveText([/Check the Iti has its sticker/, /Pack it in the box with padding/, 'Send it', 'Mark it as sent in inventory, and to whom'])
	await expect(send).toContainText('Mark it as sent in inventory, and to whom')
})

test('gives the HS codes, the weights and the lithium declaration', async ({ page }) => {
	await page.goto(PAGE)
	const declare = page.locator('section', { has: page.getByRole('heading', { name: 'Declaring it' }) })
	for (const text of ['8471.30.00.31', '8504.40.30.59', '150 g', 'UN3481', 'packing instruction 967, section II', '20 Wh', 'No more than 4 Itis in a package']) {
		await expect(declare).toContainText(text)
	}
})

test('ticks its lines off', async ({ page }) => {
	await page.goto(PAGE)
	await page.locator('#s-send').check()
	await expect(page.locator('#s-send')).toBeChecked()
})

test('opens offline at its path, with and without a trailing slash', async ({ page, context }) => {
	await page.goto('/')
	await controlled(page)
	await context.setOffline(true)
	for (const path of [PAGE, `${PAGE}/`]) {
		await page.goto(path)
		await expect(page.getByRole('heading', { level: 1, name: TITLE })).toBeVisible()
	}
})
