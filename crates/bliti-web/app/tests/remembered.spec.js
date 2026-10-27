// The devices a tab remembers, and what a reload comes back to (WEB, "Remembering devices", "After a
// reload"). Session storage is the browser's own, so a reload here is a reload there.

import { expect, test } from '@playwright/test'

import { emit, feed, installFakeClient, message, typeCode as read } from './fake-client.js'

const A = 'BLITI:AHFYTP4T6K2M9WQX'
const B = 'BLITI:AHFYTP4T6K2M2KXA'
const C = 'BLITI:AHFYTP4T6K2MH6TE'
const D = 'BLITI:AHFYTP4T6K2MQ7RM'

const hostnameFact = (value) =>
	message({ type: 'fact', at: 1, fact: 'hostname', traits: { status: { is: 'passed' } }, kind: 'text', value })

const recent = (page) => page.locator('section').filter({ has: page.getByRole('heading', { name: 'Recently used' }) })
const held = (page) => page.locator('section').filter({ hasText: 'Now find your Iti' })
const title = (page) => page.getByRole('heading', { level: 1 })
// The step the operator is on, of reaching a device (WEB).
const step = (page) => page.locator('.steps [aria-current="step"] .name')

/// Open a channel to the device `code` belongs to, optionally hearing its hostname.
async function open(page, code, hostname) {
	await read(page, code)
	await page.getByRole('button', { name: 'Find my Iti' }).click()
	// Titled with the end of its code until it says what it is called (STAT).
	await expect(title(page)).toHaveText(code.slice(-4))
	if (hostname) {
		await feed(page, hostnameFact(hostname))
		await expect(title(page)).toHaveText(hostname)
		// Remembered once the page has rendered the fact, so a reload straight after could lose it.
		await expect
			.poll(() => page.evaluate(() => sessionStorage.getItem('bliti.recent') ?? ''))
			.toContain(hostname)
	}
}

/// Reach a device, let it go, and go back to reading a code.
async function visit(page, code, hostname) {
	await open(page, code, hostname)
	await page.getByRole('button', { name: 'Disconnect' }).click()
	await page.getByRole('button', { name: 'Scan a different code' }).click()
}

test.beforeEach(async ({ page }) => {
	await page.addInitScript(installFakeClient)
	await page.goto('/')
})

test.describe('the Recently used list', () => {
	test('lists a device by hostname and the last four characters of its code once a channel has opened', async ({ page }) => {
		await visit(page, A, 'clinic-store-2')
		await expect(recent(page).locator('li')).toHaveCount(1)
		await expect(recent(page).locator('li .name')).toHaveText('clinic-store-2')
		await expect(recent(page).locator('li .muted')).toHaveText('Code ending 9WQX')
		await expect(recent(page).getByRole('button', { name: 'Connect to clinic-store-2' })).toBeVisible()
	})

	test('sits above the means of reading a code', async ({ page }) => {
		await visit(page, A, 'clinic-store-2')
		await expect(page.locator('section').first()).toContainText('Recently used')
		await expect(page.locator('section').nth(1)).toContainText('Or connect to a different Iti')
	})

	test('leaves out a code no channel has opened with', async ({ page }) => {
		await read(page, A)
		await page.getByRole('button', { name: 'Scan a different code' }).click()
		await expect(page.getByText('Scan the code on your Iti')).toBeVisible()
		await expect(recent(page)).toHaveCount(0)
	})

	test('lists a device that reported no hostname by those characters alone', async ({ page }) => {
		await visit(page, A)
		await expect(recent(page).locator('li .name')).toHaveCount(0)
		await expect(recent(page).locator('li')).toHaveText(/Code ending 9WQX/)
		await expect(recent(page).getByRole('button', { name: 'Connect to the Iti with code ending 9WQX' })).toBeVisible()
	})

	test('keeps three, most recently opened first, and reopening one moves it to the top', async ({ page }) => {
		await visit(page, A, 'a')
		await visit(page, B, 'b')
		await visit(page, C, 'c')
		await visit(page, D, 'd')
		await expect(recent(page).locator('li .name')).toHaveText(['d', 'c', 'b'])

		await recent(page).getByRole('button', { name: 'Connect to b' }).click()
		await page.getByRole('button', { name: 'Find my Iti' }).click()
		await page.getByRole('button', { name: 'Disconnect' }).click()
		await page.getByRole('button', { name: 'Scan a different code' }).click()
		await expect(recent(page).locator('li .name')).toHaveText(['b', 'd', 'c'])
	})

	test('keeps a hostname heard before when the device does not say it again', async ({ page }) => {
		await visit(page, A, 'clinic-store-2')
		await visit(page, A)
		await expect(recent(page).locator('li .name')).toHaveText('clinic-store-2')
	})

	test('choosing a device holds its code with its hostname, and finds it as any code', async ({ page }) => {
		await visit(page, A, 'clinic-store-2')
		await recent(page).getByRole('button', { name: 'Connect to clinic-store-2' }).click()
		await expect(held(page).locator('.device-name')).toHaveText('clinic-store-2')
		await expect(step(page)).toHaveText('Find Iti')
		await expect(held(page).locator('.chip')).toHaveText('AHOW2EZUD4343RQ')
		await page.getByRole('button', { name: 'Find my Iti' }).click()
		await expect(title(page)).toHaveText(A.slice(-4))
	})

	test('a code typed in shows the hostname where it is a remembered device', async ({ page }) => {
		await visit(page, A, 'clinic-store-2')
		await read(page, A.toLowerCase())
		await expect(held(page).locator('.device-name')).toHaveText('clinic-store-2')
	})

	test('survives a reload', async ({ page }) => {
		await visit(page, A, 'clinic-store-2')
		await page.reload()
		await expect(recent(page).locator('li .name')).toHaveText('clinic-store-2')
	})

	test('is not carried into another tab', async ({ page, context }) => {
		await visit(page, A, 'clinic-store-2')
		const other = await context.newPage()
		await other.addInitScript(installFakeClient)
		await other.goto('/')
		await expect(other.getByText('Scan the code on your Iti')).toBeVisible()
		await expect(recent(other)).toHaveCount(0)
	})

	test('forgets a code that no longer reads', async ({ page }) => {
		await visit(page, A, 'a')
		await visit(page, B, 'b')
		await page.addInitScript((code) => (window.__blitiUnreadable = [code]), A)
		await page.reload()
		await expect(recent(page).locator('li .name')).toHaveText(['b'])
		const stored = await page.evaluate(() => JSON.parse(sessionStorage.getItem('bliti.recent')))
		expect(stored.map((each) => each.code)).toEqual([B])
	})
})

test.describe('a reload', () => {
	test('while connected comes back holding that device, ready to find it', async ({ page }) => {
		await open(page, A, 'clinic-store-2')
		await page.reload()
		await expect(held(page).locator('.device-name')).toHaveText('clinic-store-2')
		await expect(page.getByRole('button', { name: 'Find my Iti' })).toBeEnabled()
	})

	test('while the device restarts comes back holding that device', async ({ page }) => {
		await open(page, A, 'clinic-store-2')
		await emit(page, message({ type: 'going-away', act: 'restart', cause: 'manual-control' }))
		await expect(page.getByText('Restarting bliti…')).toBeVisible()
		await page.reload()
		await expect(held(page).locator('.device-name')).toHaveText('clinic-store-2')
	})

	test('after disconnecting comes back holding no code', async ({ page }) => {
		await open(page, A, 'clinic-store-2')
		await page.getByRole('button', { name: 'Disconnect' }).click()
		await page.reload()
		await expect(step(page)).toHaveText('Scan code')
		await expect(held(page)).toHaveCount(0)
		await expect(recent(page).locator('li .name')).toHaveText('clinic-store-2')
	})

	test('comes back holding no code where the device it was on no longer reads', async ({ page }) => {
		await open(page, A, 'clinic-store-2')
		await page.addInitScript((code) => (window.__blitiUnreadable = [code]), A)
		await page.reload()
		await expect(step(page)).toHaveText('Scan code')
		await expect(held(page)).toHaveCount(0)
		await expect(recent(page)).toHaveCount(0)
		await expect(page.locator('.why')).toHaveCount(0)
	})
})

// A code never arrives by a link, so an address carrying a fragment is just an address.
test('a fragment in the address is passed over', async ({ page }) => {
	await page.goto('about:blank')
	await page.goto(`/#${A.split(':').pop()}`)
	await expect(step(page)).toHaveText('Scan code')
	await expect(held(page)).toHaveCount(0)
	await expect(page.locator('.why')).toHaveCount(0)
})
