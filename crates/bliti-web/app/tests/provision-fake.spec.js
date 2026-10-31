// The fake device of PWEB, "The fake device": the real provisioning client with nothing faked around
// it, so the fake is reached the way an operator reaches it, from dev mode.

import { expect, test } from '@playwright/test'

import { fakeButton as fake, openReal } from './fake-client.js'
import { devMode } from './fake-provision-client.js'

const box = (page, title) => page.getByRole('checkbox', { name: new RegExp(`^${title}`) })

const open = (page, options) => openReal(page, '/provision', options)

async function connected(page, options) {
	await open(page, options)
	await devMode(page)
	await fake(page).click()
	await page.getByText('Choose what to do').waitFor()
}

test('is offered while finding a device in dev mode, and not out of it', async ({ page }) => {
	await open(page)
	await expect(fake(page)).toHaveCount(0)
	await devMode(page)
	await expect(fake(page)).toBeVisible()
	await devMode(page)
	await expect(fake(page)).toHaveCount(0)
})

test('is held back while the chooser is open', async ({ page }) => {
	await page.addInitScript(() =>
		Object.defineProperty(navigator, 'bluetooth', { value: { requestDevice: () => new Promise(() => {}) }, configurable: true }),
	)
	await open(page, { bluetooth: false })
	await devMode(page)
	await page.getByRole('button', { name: 'Find Iti' }).click()
	await expect(page.getByRole('button', { name: 'Find Iti' })).toBeDisabled()
	await expect(fake(page)).toBeDisabled()
})

test('connects with no chooser and is named FAKE', async ({ page }) => {
	await connected(page)
	await expect(page.locator('.chip')).toHaveText('FAKE')
	await expect(page.getByRole('button', { name: 'Disconnect' })).toBeVisible()
})

test('is idle with every step listed and ticked, and no medium refused', async ({ page }) => {
	await connected(page)
	await expect(page.locator('label.pick')).toHaveCount(8)
	await expect(page.locator('label.pick').first()).toHaveText('Check the hardware')
	await expect(page.locator('label.pick').last()).toHaveText('Make the label')
	for (const label of await page.locator('label.pick input').all()) await expect(label).toBeChecked()
	await expect(page.getByText('Choose what to do')).toBeVisible()
})

test('finishes a run at once with the steps chosen, and no record', async ({ page }) => {
	await connected(page)
	await box(page, 'Program the EEPROM').uncheck()
	await page.getByRole('button', { name: 'Start' }).click()
	await expect(page.getByText('Provisioned')).toBeVisible()
	await expect(page.getByText('FAKE is ready')).toBeVisible()
	await expect(page.getByText('Recovery passphrase')).toHaveCount(0)
	await expect(page.getByText('Saved', { exact: true })).toHaveCount(0)
	const said = page.locator('.log .line .said').filter({ hasText: /^provision-state  finished/ })
	await expect(said).toContainText('hardware passed')
	await expect(said).toContainText('eeprom unchosen')
})

test('adds no label to the list, and does not say it did', async ({ page }) => {
	await connected(page)
	await page.getByRole('button', { name: 'Start' }).click()
	await expect(page.getByText('Provisioned')).toBeVisible()
	await expect(page.getByText('Its label is in your list.')).toHaveCount(0)
	await expect(page.getByRole('button', { name: /^Labels/ })).toHaveCount(0)
	await page.getByRole('button', { name: 'Provision another Iti' }).click()
	await expect(page.getByRole('button', { name: /^Labels/ })).toHaveCount(0)
})

test('runs again when the steps are chosen again', async ({ page }) => {
	await connected(page)
	await page.getByRole('button', { name: 'Start' }).click()
	await page.getByRole('button', { name: 'Run steps again' }).click()
	await expect(page.getByText('Choose what to do')).toBeVisible()
	await box(page, 'Check the hardware').uncheck()
	await page.getByRole('button', { name: 'Start' }).click()
	await expect(page.getByText('Provisioned')).toBeVisible()
})

test('records what it and the page said', async ({ page }) => {
	await connected(page)
	await page.getByRole('button', { name: 'Start' }).click()
	const said = page.locator('.log .line .said')
	await expect(said.filter({ hasText: /^provision-act  start/ })).toHaveCount(1)
	await expect(said.filter({ hasText: /^provision-accepted/ })).toHaveCount(1)
	await expect(said.filter({ hasText: /^provision-state  finished/ })).toHaveCount(1)
})

test('stays connected when dev mode is turned off', async ({ page }) => {
	await connected(page)
	await devMode(page)
	await expect(page.locator('.log')).toHaveCount(0)
	await expect(page.getByRole('button', { name: 'Disconnect' })).toBeVisible()
	await expect(page.getByText('Choose what to do')).toBeVisible()
})

test.describe('in a browser that cannot open a channel', () => {
	test('is offered in dev mode beside the problem, and nothing else is', async ({ page }) => {
		await open(page, { bluetooth: false })
		await expect(page.getByText("This browser can't connect to an Iti")).toBeVisible()
		await expect(fake(page)).toHaveCount(0)
		await devMode(page)
		await expect(fake(page)).toBeVisible()
	})

	test('connects, and says so again once let go of', async ({ page }) => {
		await connected(page, { bluetooth: false })
		await page.getByRole('button', { name: 'Start' }).click()
		await expect(page.getByText('Provisioned')).toBeVisible()
		await page.getByRole('button', { name: 'Provision another Iti' }).click()
		await expect(page.getByText("This browser can't connect to an Iti")).toBeVisible()
		await expect(fake(page)).toBeVisible()
	})
})
