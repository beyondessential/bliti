// The fake device of WEB, "The fake device": the real client with nothing faked around it, so the
// fake is reached the way an operator reaches it, from dev mode.

import { expect, test } from '@playwright/test'

import { devMode, fakeButton as fake, openAdvanced, openReal, typeCode } from './fake-client.js'

const title = (page) => page.getByRole('heading', { level: 1 })
const section = (page, name) => page.locator('section').filter({ has: page.getByRole('heading', { name, exact: true }) })

const open = (page, options) => openReal(page, '/', options)

/// Reach the fake device from the step for reading a code, in dev mode.
async function connected(page, options) {
	await open(page, options)
	await devMode(page)
	await fake(page).click()
	await page.getByRole('button', { name: 'Connect', exact: true }).click()
	await expect(title(page)).toHaveText('fake-iti')
}

const advanced = openAdvanced

test.describe('reaching it', () => {
	test('its code reads only in dev mode', async ({ page }) => {
		await open(page)
		await typeCode(page, ' bliti:fake ')
		await expect(page.getByText('Code scanned')).toHaveCount(0)
		await expect(page.locator('.why')).toBeVisible()
		await devMode(page)
		await typeCode(page, ' bliti:fake ')
		await expect(page.getByText('Now connect to the fake Iti')).toBeVisible()
	})

	test('is let go of once dev mode is off and the page is not on it', async ({ page }) => {
		await open(page)
		await devMode(page)
		await fake(page).click()
		await expect(page.getByText('Now connect to the fake Iti')).toBeVisible()
		await devMode(page)
		await expect(page.getByText('Scan the code on your Iti')).toBeVisible()
	})

	test('is offered in dev mode and not out of it', async ({ page }) => {
		await open(page)
		await expect(fake(page)).toHaveCount(0)
		await devMode(page)
		await expect(fake(page)).toBeVisible()
		await devMode(page)
		await expect(fake(page)).toHaveCount(0)
	})

	test('goes through finding and connecting with no chooser, and offers no code to save', async ({ page }) => {
		await open(page)
		await devMode(page)
		await fake(page).click()
		await expect(page.getByText('Code scanned')).toBeVisible()
		await expect(page.getByRole('button', { name: 'Save code for reprinting' })).toHaveCount(0)
		await expect(page.getByRole('button', { name: 'Find my Iti' })).toHaveCount(0)
		await page.getByRole('button', { name: 'Connect', exact: true }).click()
		await expect(title(page)).toHaveText('fake-iti')
		await expect(page.locator('.software')).toContainText('Fake Tamanu Iti')
		await expect(page.getByRole('button', { name: 'Disconnect' })).toBeVisible()
	})

	test('says it is fake in its software identity, and FAKE in place of a payload suffix', async ({ page }) => {
		await connected(page)
		await page.getByRole('button', { name: 'Disconnect' }).click()
		await page.getByRole('button', { name: 'Scan a different code' }).click()
		await expect(page.getByText('Code ending FAKE')).toBeVisible()
		await page.getByRole('button', { name: 'Connect to fake-iti' }).click()
		await page.getByRole('button', { name: 'Connect', exact: true }).click()
		await advanced(page)
		await page.getByRole('button', { name: 'All readings' }).click()
		await expect(page.locator('.software')).toHaveText('bliti-fake 0.0.0-fake')
	})
})

test.describe('connected to it', () => {
	test('shows the rows of the Status screen', async ({ page }) => {
		await connected(page)
		const rows = page.locator('.health li .name')
		await expect(rows).toHaveText(['Network connected', 'Tailscale connected', 'Tamanu running'])
	})

	test('every screen below the Status screen has something to show', async ({ page }) => {
		await connected(page)
		await advanced(page)

		await page.getByRole('button', { name: 'All readings' }).click()
		await expect(page.locator('.tile').first()).toBeVisible()
		await page.getByRole('button', { name: 'Back' }).click()

		await page.getByRole('button', { name: 'Power and battery' }).click()
		await expect(section(page, 'Power').getByRole('button', { name: 'Restart' })).toBeVisible()
		await expect(section(page, 'Battery').getByRole('button', { name: 'Export' })).toBeVisible()
		await page.getByRole('button', { name: 'Back' }).click()

		await page.getByRole('button', { name: 'Network settings' }).click()
		await expect(page.getByRole('heading', { name: 'Connections' })).toBeVisible()
		await page.getByRole('button', { name: 'Back' }).click()

		await page.getByRole('button', { name: 'Tailscale', exact: true }).click()
		await expect(title(page)).toHaveText('Tailscale')
		await expect(section(page, 'Network check').getByRole('button', { name: 'Run' })).toBeVisible()
		await expect(page.getByRole('button', { name: 'Turn off' })).toBeVisible()
	})

	test('answers a network check and a ping', async ({ page }) => {
		await connected(page)
		await advanced(page)
		await page.getByRole('button', { name: 'Tailscale', exact: true }).click()
		await section(page, 'Network check').getByRole('button', { name: 'Run' }).click()
		await expect(page.getByText('Can connect')).toBeVisible()
		await page.getByRole('button', { name: 'Ping' }).click()
		await expect(page.getByText('answered')).toBeVisible()
	})

	test('accepts a Tailscale act and a Tamanu act and reports nothing different after', async ({ page }) => {
		await connected(page)
		const rows = page.locator('.health li .name')
		await page.locator('.health li').filter({ hasText: 'Tamanu' }).getByRole('button', { name: 'Restart' }).click()
		await page.getByRole('alertdialog', { name: 'Restart Tamanu?' }).getByRole('button', { name: 'Restart' }).click()
		await expect(page.locator('.health li').filter({ hasText: 'Tamanu' }).getByRole('button', { name: 'Restart' })).toBeEnabled()
		await expect(rows).toHaveText(['Network connected', 'Tailscale connected', 'Tamanu running'])
		await advanced(page)
		await page.getByRole('button', { name: 'Tailscale', exact: true }).click()
		await page.getByRole('button', { name: 'Turn off' }).click()
		await page.getByRole('alertdialog').getByRole('button', { name: 'Turn off' }).click()
		await expect(page.getByRole('button', { name: 'Turn off' })).toBeEnabled()
		await expect(page.getByRole('listitem').filter({ hasText: 'Connected' }).first()).toBeVisible()
	})

	test('answers a proposed configuration as saved, and holds the canned one again in a new session', async ({ page }) => {
		await connected(page)
		await advanced(page)
		await page.getByRole('button', { name: 'Network settings' }).click()
		await expect(page.getByRole('heading', { name: 'Connections' })).toBeVisible()
		await page.locator('.order li').filter({ hasText: 'Fake-Staff' }).locator('.what').click()
		await page.getByLabel('Name', { exact: true }).fill('Renamed')
		await page.getByRole('button', { name: 'Apply' }).click()
		await expect(page.locator('.order li').filter({ hasText: 'Renamed' })).toBeVisible()
		await page.getByRole('button', { name: 'Back' }).click()
		await advanced(page)
		await page.getByRole('button', { name: 'Network settings' }).click()
		await expect(page.locator('.order li').filter({ hasText: 'Fake-Staff' })).toBeVisible()
		await expect(page.locator('.order li').filter({ hasText: 'Renamed' })).toHaveCount(0)
	})

	for (const [act, saying] of [['Restart', 'Restarting'], ['Reboot', 'Rebooting']]) {
		test(`goes away and comes back on ${act.toLowerCase()}`, async ({ page }) => {
			await connected(page)
			await advanced(page)
			await page.getByRole('button', { name: 'Power and battery' }).click()
			await section(page, 'Power').getByRole('button', { name: act }).click()
			await page.getByRole('alertdialog').getByRole('button', { name: act }).click()
			await expect(page.getByText(saying)).toBeVisible()
			await expect(page.locator('.health li .name')).toHaveCount(3)
			await expect(title(page)).toHaveText('fake-iti')
			await expect(page.getByRole('button', { name: 'Disconnect' })).toBeVisible()
		})
	}

	test('goes away on power-off, and is found again', async ({ page }) => {
		await connected(page)
		await advanced(page)
		await page.getByRole('button', { name: 'Power and battery' }).click()
		await section(page, 'Power').getByRole('button', { name: 'Power off' }).click()
		await page.getByRole('alertdialog').getByRole('button', { name: 'Power off' }).click()
		await expect(page.getByText('Shutting down')).toBeVisible()
		await expect(page.getByRole('button', { name: 'Connect', exact: true })).toBeVisible()
		await page.getByRole('button', { name: 'Connect', exact: true }).click()
		await expect(title(page)).toHaveText('fake-iti')
	})

	test('records what it and the page said', async ({ page }) => {
		await connected(page)
		const said = page.locator('.log .line .said')
		await expect(said.filter({ hasText: 'hello  name bliti-web' })).toHaveCount(1)
		await expect(said.filter({ hasText: 'hello  name bliti-fake' })).toHaveCount(1)
		await expect(said.filter({ hasText: 'tamanu' }).first()).toBeVisible()
	})

	test('stays connected when dev mode is turned off, and is let go of on disconnecting', async ({ page }) => {
		await connected(page)
		await devMode(page)
		await expect(page.locator('.log')).toHaveCount(0)
		await expect(page.getByRole('button', { name: 'Disconnect' })).toBeVisible()
		await expect(page.locator('.health li')).toHaveCount(3)
		await page.getByRole('button', { name: 'Disconnect' }).click()
		await expect(page.getByText('Scan the code on your Iti')).toBeVisible()
		await expect(page.getByText('fake-iti')).toHaveCount(0)
	})

	test('says accepted before it goes away', async ({ page }) => {
		await connected(page)
		await advanced(page)
		await page.getByRole('button', { name: 'Power and battery' }).click()
		await section(page, 'Power').getByRole('button', { name: 'Restart' }).click()
		await page.getByRole('alertdialog').getByRole('button', { name: 'Restart' }).click()
		await expect(page.getByText('Restarting')).toBeVisible()
		const said = await page.locator('.log .line .said').allTextContents()
		const accepted = said.indexOf('accepted')
		expect(accepted).toBeGreaterThan(-1)
		expect(said.findIndex((line) => line.startsWith('going-away'))).toBeGreaterThan(accepted)
	})

	test('is held again after a reload, ready to connect', async ({ page }) => {
		await connected(page)
		await page.reload()
		await expect(page.getByText('Code scanned')).toBeVisible()
		await page.getByRole('button', { name: 'Connect', exact: true }).click()
		await expect(title(page)).toHaveText('fake-iti')
	})
})

test.describe('remembered', () => {
	test('is listed in dev mode and not out of it', async ({ page }) => {
		await connected(page)
		await page.getByRole('button', { name: 'Disconnect' }).click()
		await page.getByRole('button', { name: 'Scan a different code' }).click()
		await expect(page.getByRole('heading', { name: 'Recently used' })).toBeVisible()
		await expect(page.getByText('Code ending FAKE')).toBeVisible()
		await devMode(page)
		await expect(page.getByRole('heading', { name: 'Recently used' })).toHaveCount(0)
		await devMode(page)
		await expect(page.getByText('Code ending FAKE')).toBeVisible()
	})
})

test.describe('in a browser that cannot open a channel', () => {
	test('is offered in dev mode beside the problem, and nothing else is', async ({ page }) => {
		await open(page, { bluetooth: false })
		await expect(page.getByText("This browser can't connect to an Iti")).toBeVisible()
		await expect(fake(page)).toHaveCount(0)
		await devMode(page)
		await expect(fake(page)).toBeVisible()
	})

	test('connects and says so again once let go of', async ({ page }) => {
		await connected(page, { bluetooth: false })
		await page.getByRole('button', { name: 'Disconnect' }).click()
		await expect(page.getByText("This browser can't connect to an Iti")).toBeVisible()
		await expect(fake(page)).toBeVisible()
	})

	test('says so again after a power-off', async ({ page }) => {
		await connected(page, { bluetooth: false })
		await advanced(page)
		await page.getByRole('button', { name: 'Power and battery' }).click()
		await section(page, 'Power').getByRole('button', { name: 'Power off' }).click()
		await page.getByRole('alertdialog').getByRole('button', { name: 'Power off' }).click()
		await expect(page.getByText("This browser can't connect to an Iti")).toBeVisible()
	})

	test('comes back to the fake device after a reload', async ({ page }) => {
		await connected(page, { bluetooth: false })
		await page.reload()
		await expect(page.getByText('Code scanned')).toBeVisible()
	})
})
