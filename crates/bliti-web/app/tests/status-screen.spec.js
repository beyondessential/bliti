// The Status screen of STAT, the landing a channel opens on, driven by facts fed in batches as the
// device sends them (NFO).

import { expect, test } from '@playwright/test'

import { IN_FORCE, PI, bar } from './network-fixtures.js'
import {
	UNNAMED,
	answer,
	emit,
	feed,
	message,
	openChannel,
	openChecking,
	openNetworkScreen,
	openPower,
	openReadings,
	say,
	stateOf,
} from './fake-client.js'
import { ON_CABLE } from './setup-fixtures.js'

const passed = { status: { is: 'passed' } }
const fact = (name, value, traits = {}, kind = 'text') =>
	message({ type: 'fact', at: 1, fact: name, kind, value, traits: { ...passed, ...traits } })
const ended = (name, value, traits = {}, kind = 'text') =>
	message({ type: 'fact', at: 2, fact: name, kind, value, traits: { ...traits, status: { is: 'ended', reason: 'no longer applies' } } })
const carrier = (iface) => fact('wired-carrier', true, { interface: { name: iface } }, 'boolean')
const address = (iface, value) => fact('network-address', value, { interface: { name: iface } }, 'ipv4')

const title = (page) => page.getByRole('heading', { level: 1 })
const rows = (page) => page.locator('.health li .name')
const row = (page, name) => page.locator('.health li').filter({ has: page.locator('.name', { hasText: name }) })

test.describe('the landing', () => {
	test('is titled with the hostname, with the identity in short beneath and Disconnect beside it', async ({ page }) => {
		await openChannel(
			page,
			fact('hostname', 'clinic-iti-07'),
			fact('board', 'Raspberry Pi 5 Model B Rev 1.0', { short: 'Raspberry Pi 5' }),
			fact('board-revision', 'd04170'),
			fact('os', 'Ubuntu 26.04 LTS', { short: 'Ubuntu 26.04' }),
			fact('kernel', '6.8.0'),
		)
		await expect(title(page)).toHaveText('clinic-iti-07')
		await expect(page.locator('.heading.title .software')).toHaveText('Raspberry Pi 5 · Ubuntu 26.04')
		await expect(page.locator('.heading.title .actions button')).toHaveText(['Disconnect'])
	})

	// The product stands in for the board where it has a value (STAT).
	test('names the product in place of the board', async ({ page }) => {
		await openChannel(
			page,
			fact('hostname', 'clinic-iti-07'),
			fact('product', 'Tamanu Iti v4'),
			fact('board', 'Raspberry Pi 5 Model B Rev 1.0', { short: 'Raspberry Pi 5' }),
			fact('os', 'Ubuntu 26.04 LTS', { short: 'Ubuntu 26.04' }),
		)
		await expect(page.locator('.heading.title .software')).toHaveText('Tamanu Iti v4 · Ubuntu 26.04')
	})

	// A product that could not be told leaves the board to name the device (STAT).
	test('names the board where the product is broken', async ({ page }) => {
		await openChannel(
			page,
			fact('hostname', 'clinic-iti-07'),
			message({ type: 'fact', at: 1, fact: 'product', kind: 'text', traits: { status: { is: 'broken', reason: 'no HAT EEPROM' } } }),
			fact('board', 'Raspberry Pi 5 Model B Rev 1.0', { short: 'Raspberry Pi 5' }),
			fact('os', 'Ubuntu 26.04 LTS', { short: 'Ubuntu 26.04' }),
		)
		await expect(page.locator('.heading.title .software')).toHaveText('Raspberry Pi 5 · Ubuntu 26.04')
	})

	// A board with no short form is named by its value, without its revision (STAT).
	test('names a board or os with no short form by its value', async ({ page }) => {
		await openChannel(
			page,
			fact('hostname', 'dev-laptop'),
			fact('board', 'Dell Inc. G7 7700'),
			fact('board-revision', 'A01'),
			fact('os', 'Arch Linux', { short: 'Arch Linux' }),
			fact('kernel', '7.2.11'),
		)
		await expect(page.locator('.heading.title .software')).toHaveText('Dell Inc. G7 7700 · Arch Linux')
	})

	test('is titled with the end of the code where the device reports no hostname', async ({ page }) => {
		await openChannel(page)
		await expect(title(page)).toHaveText(UNNAMED)
		await expect(page.locator('.heading.title .software')).toHaveCount(0)
	})
})

test.describe('the rows', () => {
	test('are checking until the first batch has ended', async ({ page }) => {
		await openChecking(page)
		await expect(rows(page)).toHaveText(['Checking network', 'Checking Tailscale', 'Checking Tamanu'])
		await expect(page.locator('.health button')).toHaveCount(0)

		// Held until the batch ends.
		await emit(page, carrier('end0'))
		await emit(page, address('end0', '192.0.2.4'))
		await emit(page, fact('tamanu-state', 'running'))
		await expect(rows(page)).toHaveText(['Checking network', 'Checking Tailscale', 'Checking Tamanu'])

		await feed(page)
		// A device that reports no Tailscale has no Tailscale row once the first batch has ended.
		await expect(rows(page)).toHaveText(['Network connected', 'Tamanu running'])
		await expect(row(page, 'Network').locator('.detail')).toHaveText('Cable')
	})

	test('show every state of the three in order', async ({ page }) => {
		await openChannel(
			page,
			carrier('end0'),
			address('end0', '192.0.2.4'),
			fact('tailscale-state', 'running'),
			fact('tailscale-name', 'clinic-iti-07'),
			fact('tamanu-state', 'degraded', { status: { is: 'warning', reason: 'Facility server is running 2.33.2.' } }),
			fact('tamanu-web-version', '2.34.1'),
			fact('tamanu-facility-server-version', '2.33.2'),
		)
		await expect(rows(page)).toHaveText(['Network connected', 'Tailscale connected', 'Tamanu partly running'])
		await expect(row(page, 'Tailscale').locator('.detail')).toHaveText('Device nameclinic-iti-07')
		await expect(row(page, 'Tamanu').locator('.detail.reason')).toHaveText('Facility server is running 2.33.2.')
		await expect(row(page, 'Tamanu').locator('.detail:not(.reason)')).toHaveText('Web 2.34.1·Facility server 2.33.2')
		await expect(page.locator('.health .mark')).toHaveClass([/good/, /good/, /warn/])
	})

	// An address moving from one interface to another is its ending and its replacement in one batch,
	// so the row never says the device is not connected between the two (NFO, STAT).
	test('change a batch at a time, never between an entry ending and its replacement', async ({ page }) => {
		await openChannel(page, carrier('end0'), address('end0', '192.0.2.4'), fact('wireless-network', 'Clinic-Staff', { interface: { name: 'wlan0' } }))
		await expect(rows(page).first()).toHaveText('Network connected')

		await page.evaluate(() => {
			window.__said = []
			new MutationObserver(() => {
				for (const name of document.querySelectorAll('.health .name')) window.__said.push(name.textContent)
			}).observe(document.querySelector('.health'), { subtree: true, childList: true, characterData: true })
		})
		await emit(page, ended('network-address', '192.0.2.4', { interface: { name: 'end0' } }, 'ipv4'))
		await emit(page, address('wlan0', '192.0.2.4'))
		await feed(page)

		await expect(row(page, 'Network').locator('.detail')).toHaveText('Clinic-Staff')
		expect(await page.evaluate(() => window.__said)).not.toContain('Network not connected')
	})

	test('offer Set up while the network is not connected, and Change once it is', async ({ page }) => {
		await openChannel(page, carrier('end0'))
		await expect(rows(page).first()).toHaveText('Network not connected')
		await expect(row(page, 'Network').locator('.detail')).toHaveText('Cable plugged in, no address')
		await expect(row(page, 'Network').getByRole('button')).toHaveText('Set up')

		await feed(page, address('end0', '192.0.2.4'))
		await expect(row(page, 'Network').getByRole('button')).toHaveText('Change')
	})
})

test.describe('the network row', () => {
	test('Change opens the Network screen, and its Back returns to the Status screen, closing the session', async ({ page }) => {
		await openChannel(page, carrier('end0'), address('end0', '192.0.2.4'))
		await answer(page, 'configure', [message({ type: 'configuration', document: ON_CABLE, capabilities: PI }), stateOf()])
		await row(page, 'Network').getByRole('button', { name: 'Change' }).click()
		await expect(title(page)).toHaveText('Network')
		await page.getByRole('button', { name: 'Back', exact: true }).click()
		await expect(title(page)).toHaveText(UNNAMED)
		expect(await page.evaluate(() => window.__blitiSessions.map((each) => each.closedByPage))).toEqual([true])
	})

	test('Set up says it is waiting on the device until the session answers', async ({ page }) => {
		await openChannel(page, carrier('end0'))
		await row(page, 'Network').getByRole('button', { name: 'Set up' }).click()
		await expect(row(page, 'Network').getByRole('status')).toHaveText('Asking the Iti…')
		await expect(title(page)).toHaveText(UNNAMED)
		await say(page, message({ type: 'configuration', document: ON_CABLE, capabilities: PI }))
		await expect(title(page)).toHaveText(UNNAMED)
		await say(page, stateOf())
		await expect(title(page)).toHaveText('Network')
	})

	test('stays on the Status screen where someone else holds the session, saying so until asked again', async ({ page }) => {
		await openChannel(page, carrier('end0'))
		await answer(page, 'configure', [message({ type: 'configuration', document: ON_CABLE, capabilities: PI }), stateOf({ busy: true })])
		await row(page, 'Network').getByRole('button', { name: 'Set up' }).click()
		await expect(row(page, 'Network')).toContainText('Someone else is making changes')
		await expect(title(page)).toHaveText(UNNAMED)
		expect(await page.evaluate(() => window.__blitiSessions.map((each) => each.closedByPage))).toEqual([true])

		await answer(page, 'configure', [message({ type: 'configuration', document: ON_CABLE, capabilities: PI }), stateOf()])
		await row(page, 'Network').getByRole('button', { name: 'Set up' }).click()
		await expect(title(page)).toHaveText('Network')
		await page.getByRole('button', { name: 'Back', exact: true }).click()
		await expect(row(page, 'Network')).not.toContainText('Someone else')
	})

	test('clears the note once the operator leaves the Status screen', async ({ page }) => {
		await openChannel(page, carrier('end0'))
		await answer(page, 'configure', [message({ type: 'configuration', document: ON_CABLE, capabilities: PI }), stateOf({ busy: true })])
		await row(page, 'Network').getByRole('button', { name: 'Set up' }).click()
		await expect(row(page, 'Network')).toContainText('Someone else is making changes')
		await openReadings(page)
		await page.getByRole('button', { name: 'Back', exact: true }).click()
		await expect(row(page, 'Network')).not.toContainText('Someone else')
	})

	test('network settings opens under Advanced as it did, read-only where someone else holds the session', async ({ page }) => {
		await openChannel(page, carrier('end0'))
		await answer(page, 'configure', [message({ type: 'configuration', document: IN_FORCE, capabilities: PI }), stateOf({ busy: true })])
		await openNetworkScreen(page)
		await expect(bar(page)).toHaveAttribute('data-stage', 'watching')
		await expect(page.getByLabel('Country')).toBeDisabled()
	})
})

test.describe('Advanced', () => {
	test('is closed when a channel opens, and leads to every other screen', async ({ page }) => {
		await openChannel(page)
		const advanced = page.getByRole('button', { name: 'Advanced' })
		await expect(advanced).toHaveAttribute('aria-expanded', 'false')
		await expect(page.getByRole('button', { name: 'All readings' })).toHaveCount(0)
		await advanced.click()
		await expect(page.locator('.advanced .inside button')).toHaveText(['All readings', 'Network settings', 'Power and battery'])
	})

	test('stays as the operator left it across screens, and closes on a new channel', async ({ page }) => {
		await openChannel(page)
		await openReadings(page)
		await expect(title(page)).toHaveText('All readings')
		await page.getByRole('button', { name: 'Back', exact: true }).click()
		await expect(page.getByRole('button', { name: 'Advanced' })).toHaveAttribute('aria-expanded', 'true')

		// The browser's way back as the screen's own.
		await openPower(page)
		await page.goBack()
		await expect(title(page)).toHaveText(UNNAMED)
		await expect(page.getByRole('button', { name: 'Advanced' })).toHaveAttribute('aria-expanded', 'true')

		await page.getByRole('button', { name: 'Advanced' }).click()
		await expect(page.getByRole('button', { name: 'Advanced' })).toHaveAttribute('aria-expanded', 'false')

		await page.getByRole('button', { name: 'Disconnect' }).click()
		await page.getByRole('button', { name: 'Find my Iti' }).click()
		await expect(page.getByRole('button', { name: 'Advanced' })).toHaveAttribute('aria-expanded', 'false')
	})
})

// A feed declined while the page is hidden misses every entry that ended meanwhile, so the first batch
// of the resumed feed replaces what was held (NFO).
test('a resumed feed drops what ended while it was declined', async ({ page }) => {
	await openChannel(page, carrier('end0'), address('end0', '192.0.2.4'), fact('hotspot', 'Clinic-Field-05'))
	await expect(row(page, 'Network').locator('.detail')).toHaveText('Cable·Hotspot')

	await page.evaluate(() => {
		Object.defineProperty(document, 'hidden', { value: true, configurable: true })
		document.dispatchEvent(new Event('visibilitychange'))
	})
	await page.evaluate(() => {
		Object.defineProperty(document, 'hidden', { value: false, configurable: true })
		document.dispatchEvent(new Event('visibilitychange'))
	})
	await expect.poll(() => page.evaluate(() => window.__blitiFeeds.length)).toBe(2)

	await feed(page, carrier('end0'), address('end0', '192.0.2.4'))
	await expect(row(page, 'Network').locator('.detail')).toHaveText('Cable')
})
