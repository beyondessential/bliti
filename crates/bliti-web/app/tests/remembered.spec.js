// The devices a tab remembers, and what a reload comes back to (WEB, "Remembering devices", "After a
// reload"). Session storage is the browser's own, so a reload here is a reload there.

import { expect, test } from '@playwright/test'

import { emit, installFakeClient, message } from './fake-client.js'

const A = 'BLITI:AHFYTP4T6K2M9WQX'
const B = 'BLITI:AHFYTP4T6K2M2KXA'
const C = 'BLITI:AHFYTP4T6K2MH6TE'
const D = 'BLITI:AHFYTP4T6K2MQ7RM'

const hostnameFact = (value) =>
	message({ type: 'fact', at: 1, fact: 'hostname', traits: { status: { is: 'passed' } }, kind: 'text', value })

const recent = (page) => page.locator('section').filter({ has: page.getByRole('heading', { name: 'Recent' }) })
const held = (page) => page.locator('section').filter({ has: page.getByRole('heading', { name: 'QR code read' }) })
const title = (page) => page.getByRole('heading', { level: 1 })

async function read(page, code) {
	await page.getByPlaceholder('BLITI:...').fill(code)
	await page.getByRole('button', { name: 'Use', exact: true }).click()
}

/// Open a channel to the device `code` belongs to, optionally hearing its hostname.
async function open(page, code, hostname) {
	await read(page, code)
	await page.getByRole('button', { name: 'Find the device' }).click()
	await expect(title(page)).toHaveText('Info')
	if (hostname) {
		await emit(page, hostnameFact(hostname))
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
	await page.getByRole('button', { name: 'Use another code' }).click()
}

test.beforeEach(async ({ page }) => {
	await page.addInitScript(installFakeClient)
	await page.goto('/')
})

test.describe('the Recent list', () => {
	test('lists a device by hostname and the last four characters of its code once a channel has opened', async ({ page }) => {
		await visit(page, A, 'clinic-store-2')
		await expect(recent(page).locator('li')).toHaveCount(1)
		await expect(recent(page).locator('li .name')).toHaveText('clinic-store-2')
		await expect(recent(page).locator('li .code')).toHaveText('…9WQX')
		await expect(recent(page).getByRole('button', { name: 'Use clinic-store-2' })).toBeVisible()
	})

	test('sits beneath the means of reading a code', async ({ page }) => {
		await visit(page, A, 'clinic-store-2')
		const headings = await page.getByRole('heading', { level: 2 }).allTextContents()
		expect(headings.slice(0, 2)).toEqual(['QR code', 'Recent'])
	})

	test('leaves out a code no channel has opened with', async ({ page }) => {
		await read(page, A)
		await page.getByRole('button', { name: 'Use another code' }).click()
		await expect(page.getByRole('heading', { name: 'QR code', exact: true })).toBeVisible()
		await expect(recent(page)).toHaveCount(0)
	})

	test('lists a device that reported no hostname by those characters alone', async ({ page }) => {
		await visit(page, A)
		await expect(recent(page).locator('li .name')).toHaveCount(0)
		await expect(recent(page).locator('li')).toHaveText(/…9WQX/)
		await expect(recent(page).getByRole('button', { name: 'Use 9WQX' })).toBeVisible()
	})

	test('keeps three, most recently opened first, and reopening one moves it to the top', async ({ page }) => {
		await visit(page, A, 'a')
		await visit(page, B, 'b')
		await visit(page, C, 'c')
		await visit(page, D, 'd')
		await expect(recent(page).locator('li .name')).toHaveText(['d', 'c', 'b'])

		await recent(page).getByRole('button', { name: 'Use b' }).click()
		await page.getByRole('button', { name: 'Find the device' }).click()
		await page.getByRole('button', { name: 'Disconnect' }).click()
		await page.getByRole('button', { name: 'Use another code' }).click()
		await expect(recent(page).locator('li .name')).toHaveText(['b', 'd', 'c'])
	})

	test('keeps a hostname heard before when the device does not say it again', async ({ page }) => {
		await visit(page, A, 'clinic-store-2')
		await visit(page, A)
		await expect(recent(page).locator('li .name')).toHaveText('clinic-store-2')
	})

	test('choosing a device holds its code with its hostname, and finds it as any code', async ({ page }) => {
		await visit(page, A, 'clinic-store-2')
		await recent(page).getByRole('button', { name: 'Use clinic-store-2' }).click()
		await expect(held(page).locator('.device-name')).toHaveText('clinic-store-2')
		await expect(held(page).locator('p.code')).toHaveText(A)
		await expect(page.getByText('A device named AHOW2EZUD4343RQ will show up.')).toBeVisible()
		await page.getByRole('button', { name: 'Find the device' }).click()
		await expect(title(page)).toHaveText('Info')
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
		await expect(other.getByRole('heading', { name: 'QR code', exact: true })).toBeVisible()
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
		await expect(held(page).locator('p.code')).toHaveText(A)
		await expect(held(page).locator('.device-name')).toHaveText('clinic-store-2')
		await expect(page.getByRole('button', { name: 'Find the device' })).toBeEnabled()
	})

	test('while the device restarts comes back holding that device', async ({ page }) => {
		await open(page, A, 'clinic-store-2')
		await emit(page, message({ type: 'going-away', act: 'restart', cause: 'manual-control' }))
		await expect(page.getByText('Restarting bliti…')).toBeVisible()
		await page.reload()
		await expect(held(page).locator('p.code')).toHaveText(A)
	})

	test('after disconnecting comes back holding no code', async ({ page }) => {
		await open(page, A, 'clinic-store-2')
		await page.getByRole('button', { name: 'Disconnect' }).click()
		await page.reload()
		await expect(page.getByRole('heading', { name: 'QR code', exact: true })).toBeVisible()
		await expect(held(page)).toHaveCount(0)
		await expect(recent(page).locator('li .name')).toHaveText('clinic-store-2')
	})

	test('comes back holding no code where the device it was on no longer reads', async ({ page }) => {
		await open(page, A, 'clinic-store-2')
		await page.addInitScript((code) => (window.__blitiUnreadable = [code]), A)
		await page.reload()
		await expect(page.getByRole('heading', { name: 'QR code', exact: true })).toBeVisible()
		await expect(held(page)).toHaveCount(0)
		await expect(recent(page)).toHaveCount(0)
		await expect(page.locator('.bad')).toHaveCount(0)
	})
})

// A code never arrives by a link, so an address carrying a fragment is just an address.
test('a fragment in the address is passed over', async ({ page }) => {
	await page.goto('about:blank')
	await page.goto(`/#${A.split(':').pop()}`)
	await expect(page.getByRole('heading', { name: 'QR code', exact: true })).toBeVisible()
	await expect(held(page)).toHaveCount(0)
	await expect(page.locator('.bad')).toHaveCount(0)
})
