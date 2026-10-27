// A phone ends the Bluetooth link of a browser it has put in the background. The application holds
// the operator where they were, and reaches the device again once the page is shown (WEB, "While the
// page is hidden").

import { expect, test } from '@playwright/test'

import { IN_FORCE, PI } from './network-fixtures.js'
import {
	UNNAMED,
	answer,
	closeChannel,
	message,
	openAdvanced,
	openChannel,
	openNetwork,
	openReadings,
	returns,
	stateOf,
} from './fake-client.js'

const title = (page) => page.getByRole('heading', { level: 1 })
const reconnects = (page) => page.evaluate(() => window.__blitiReconnects)

/// Hide the page, or show it again, as switching to another app and back does.
async function setHidden(page, hidden) {
	await page.evaluate((hidden) => {
		Object.defineProperty(document, 'hidden', { value: hidden, configurable: true })
		document.dispatchEvent(new Event('visibilitychange'))
	}, hidden)
}

/// The phone dropping the link of a page it has put away.
async function dropWhileHidden(page) {
	await setHidden(page, true)
	await closeChannel(page)
}

test('a link dropped while hidden is reached again once shown, on the screen the operator was on', async ({ page }) => {
	await openChannel(page)
	await openReadings(page)
	await dropWhileHidden(page)

	// Nothing is tried, and nothing said lost, while the page stays hidden.
	await page.waitForTimeout(200)
	expect(await reconnects(page)).toBe(0)
	await expect(page.getByText('Lost connection to your Iti')).toHaveCount(0)

	await returns(page, 'ok')
	await setHidden(page, false)
	await expect(title(page)).toHaveText('All readings')
	await expect(page.getByRole('status')).toHaveCount(0)
	expect(await reconnects(page)).toBe(1)

	await page.getByRole('button', { name: 'Back', exact: true }).click()
	await expect(title(page)).toHaveText(UNNAMED)
	// Advanced stays as the operator left it: only opening a channel closes it (STAT).
	await expect(page.getByRole('button', { name: 'Advanced' })).toHaveAttribute('aria-expanded', 'true')
})

test('says it is reconnecting while it tries, and keeps trying for a while', async ({ page }) => {
	await openChannel(page)
	await dropWhileHidden(page)
	await returns(page, 'fail', 'ok')
	await setHidden(page, false)
	await expect(page.getByRole('status')).toHaveText('Reconnecting to your Iti…')
	await expect(page.getByRole('button', { name: 'Advanced' })).toHaveCount(0)
	await expect(page.getByRole('button', { name: 'Advanced' })).toBeVisible()
	expect(await reconnects(page)).toBe(2)
})

test('a device that does not answer is said to be lost', async ({ page }) => {
	await openChannel(page)
	await dropWhileHidden(page)
	await setHidden(page, false)
	await expect(page.getByText('Lost connection to your Iti')).toBeVisible()
	await expect(page.getByRole('button', { name: 'Try again' })).toBeVisible()
	expect(await reconnects(page)).toBeGreaterThan(1)
})

test('a device the browser can only reach through the chooser is said to be lost', async ({ page }) => {
	await openChannel(page)
	await dropWhileHidden(page)
	await returns(page, 'chooser')
	await setHidden(page, false)
	await expect(page.getByText('Lost connection to your Iti')).toBeVisible()
	expect(await reconnects(page)).toBe(1)
})

test('disconnecting stops it reconnecting', async ({ page }) => {
	await openChannel(page)
	await dropWhileHidden(page)
	await setHidden(page, false)
	await page.getByRole('button', { name: 'Disconnect' }).click()
	await expect(page.getByRole('button', { name: 'Find my Iti' })).toBeVisible()
	const attempts = await reconnects(page)
	await page.waitForTimeout(300)
	expect(await reconnects(page)).toBe(attempts)
	await expect(page.getByText('Lost connection to your Iti')).toHaveCount(0)
})

test('a link dropped while the page is in view is lost at once', async ({ page }) => {
	await openChannel(page)
	await closeChannel(page)
	await expect(page.getByText('Lost connection to your Iti')).toBeVisible()
	expect(await reconnects(page)).toBe(0)
})

// A browser that froze a hidden page may end its link only as the page comes back (WEB).
test('a link dropped just after the page is shown again is reached again', async ({ page }) => {
	await openChannel(page)
	await openReadings(page)
	await setHidden(page, true)
	await setHidden(page, false)
	await returns(page, 'ok')
	await closeChannel(page)
	await expect(title(page)).toHaveText('All readings')
	await expect(page.getByText('Lost connection to your Iti')).toHaveCount(0)
	expect(await reconnects(page)).toBe(1)
})

test('a link dropped once the page has been shown a while is lost at once', async ({ page }) => {
	await openChannel(page)
	await setHidden(page, true)
	await setHidden(page, false)
	await page.waitForTimeout(500)
	await closeChannel(page)
	await expect(page.getByText('Lost connection to your Iti')).toBeVisible()
	expect(await reconnects(page)).toBe(0)
})

test('network settings keep the edits not applied across the reconnect', async ({ page }) => {
	await openNetwork(page, { document: IN_FORCE, capabilities: PI })
	await page.getByLabel('Country').selectOption('FJ')
	await dropWhileHidden(page)

	await answer(page, 'configure', [message({ type: 'configuration', document: IN_FORCE, capabilities: PI }), stateOf()])
	await returns(page, 'ok')
	await setHidden(page, false)

	await expect(page.getByLabel('Country')).toBeEnabled()
	await expect(page.getByLabel('Country')).toHaveValue('FJ')
	await expect(page.getByRole('button', { name: 'Apply' })).toBeEnabled()
	expect(await page.evaluate(() => window.__blitiSessions.length)).toBe(2)
})

test('network settings opened read-only after the reconnect follow the device, not the edits', async ({ page }) => {
	await openNetwork(page, { document: IN_FORCE, capabilities: PI })
	await page.getByLabel('Country').selectOption('FJ')
	await dropWhileHidden(page)

	await answer(page, 'configure', [message({ type: 'configuration', document: IN_FORCE, capabilities: PI }), stateOf({ busy: true })])
	await returns(page, 'ok')
	await setHidden(page, false)

	await expect(page.getByLabel('Country')).toBeDisabled()
	await expect(page.getByLabel('Country')).toHaveValue('VU')
})

test('power and battery opens its streams again on the reconnected channel', async ({ page }) => {
	await openChannel(page)
	await answer(page, 'power', message({ type: 'acts', acts: ['reboot'] }))
	await openAdvanced(page)
	await page.getByRole('button', { name: 'Power and battery' }).click()
	await expect(title(page)).toHaveText('Power and battery')
	await dropWhileHidden(page)

	await answer(page, 'power', message({ type: 'acts', acts: ['reboot'] }))
	await returns(page, 'ok')
	await setHidden(page, false)
	await expect(page.getByRole('button', { name: 'Reboot' })).toBeVisible()
	expect(await page.evaluate(() => window.__blitiPowerStreams.length)).toBe(2)
})
