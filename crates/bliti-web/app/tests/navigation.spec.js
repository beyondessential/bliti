// Moving between the screens of a connected device by the browser's way back as by their own, dev
// mode and its activity record, and connecting once a device is picked (WEB).

import { expect, test } from '@playwright/test'

import { IN_FORCE, PI, ap } from './network-fixtures.js'
import {
	UNNAMED,
	answer,
	devMode,
	emit,
	installFakeClient,
	message,
	openChannel,
	openNetwork,
	openNetworkScreen,
	openPower,
	stateOf,
	typeCode,
} from './fake-client.js'
import { ON_CABLE, openWay } from './setup-fixtures.js'

const title = (page) => page.getByRole('heading', { level: 1 })

/// Open a channel from a page that has somewhere to go back to, so leaving the application shows.
async function openFromBlank(page) {
	await page.goto('about:blank')
	await openChannel(page)
	await expect(title(page)).toHaveText(UNNAMED)
}

test.describe('the browser’s way back', () => {
	test('leaves a screen as its Back does', async ({ page }) => {
		await openFromBlank(page)
		await openPower(page)
		await expect(title(page)).toHaveText('Power and battery')
		await page.goBack()
		await expect(title(page)).toHaveText(UNNAMED)
		await openNetworkScreen(page)
		await expect(title(page)).toHaveText('Network settings')
		await page.goBack()
		await expect(title(page)).toHaveText(UNNAMED)
	})

	test('leaves the application from the screen a channel opens on', async ({ page }) => {
		await openFromBlank(page)
		await openPower(page)
		await page.getByRole('button', { name: 'Back', exact: true }).click()
		await expect(title(page)).toHaveText(UNNAMED)
		await page.goBack()
		await expect(page).toHaveURL('about:blank')
	})

	test('asks before leaving network settings with edits not applied, and stays on Stay', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await page.getByLabel('Country').selectOption('FJ')
		await page.goBack()
		await expect(page.getByRole('alertdialog', { name: 'Leave the network settings?' })).toBeVisible()
		await expect(title(page)).toHaveText('Network settings')
		await page.getByRole('button', { name: 'Stay' }).click()
		await expect(page.getByLabel('Country')).toHaveValue('FJ')

		await page.goBack()
		await page.getByRole('alertdialog').getByRole('button', { name: 'Leave' }).click()
		await expect(title(page)).toHaveText(UNNAMED)
	})

	test('does not ask again once Leave has been chosen from the screen’s own Back', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await page.getByLabel('Country').selectOption('FJ')
		await page.getByRole('button', { name: 'Back', exact: true }).click()
		await page.getByRole('alertdialog').getByRole('button', { name: 'Leave' }).click()
		await expect(title(page)).toHaveText(UNNAMED)
		await expect(page.getByRole('alertdialog')).toHaveCount(0)
	})

	test('finds no entry left behind by a screen the device took the operator off', async ({ page }) => {
		await openFromBlank(page)
		await answer(page, 'power', message({ type: 'acts', acts: ['reboot'] }))
		await openPower(page)
		await expect(title(page)).toHaveText('Power and battery')
		await emit(page, message({ type: 'going-away', act: 'reboot', cause: 'manual-control' }))
		await expect(page.getByRole('status')).toHaveText('Rebooting…')
		await expect(title(page)).toHaveText(UNNAMED)
		await page.goBack()
		await expect(page).toHaveURL('about:blank')
	})

	test('goes nowhere forward once the channel has closed', async ({ page }) => {
		await openFromBlank(page)
		await openPower(page)
		await page.goBack()
		await page.getByRole('button', { name: 'Disconnect' }).click()
		await page.goForward()
		await expect(page.getByRole('button', { name: 'Find my Iti' })).toBeVisible()
	})
})

test.describe('dev mode', () => {
	test('shows the activity record once a title is tapped seven times in a row, and hides it again', async ({ page }) => {
		await openChannel(page)
		await expect(page.locator('.log')).toHaveCount(0)
		await page.locator('.heading.title h1').click({ clickCount: 6 })
		await expect(page.locator('.log')).toHaveCount(0)

		await page.waitForTimeout(1_100)
		await devMode(page)
		await expect(page.locator('.log')).toContainText('code read')
		await openPower(page)
		await expect(page.locator('.log')).toBeVisible()

		await page.waitForTimeout(1_100)
		await devMode(page)
		await expect(page.locator('.log')).toHaveCount(0)
	})

	test('shows the record as soon as it is on, before anything is recorded', async ({ page }) => {
		await page.addInitScript(installFakeClient)
		await page.goto('/')
		await devMode(page)
		await expect(page.getByRole('heading', { name: 'Activity' })).toBeVisible()
		await expect(page.getByText('Nothing yet.')).toBeVisible()
	})

	test('lasts for the life of the tab', async ({ page }) => {
		await page.addInitScript(installFakeClient)
		await page.goto('/')
		await devMode(page)
		await typeCode(page, 'BLITI:AHFYTP4T6K2M9WQX')
		await expect(page.locator('.log')).toBeVisible()
		await page.reload()
		await typeCode(page, 'BLITI:AHFYTP4T6K2M9WQX')
		await expect(page.locator('.log')).toBeVisible()
	})
})

test.describe('connecting', () => {
	test.beforeEach(async ({ page }) => {
		await page.addInitScript(installFakeClient)
		await page.addInitScript(() => (window.__blitiHoldConnect = true))
		await page.goto('/')
		await typeCode(page, 'BLITI:AHFYTP4T6K2M9WQX')
		await page.getByRole('button', { name: 'Find my Iti' }).click()
	})

	test('says it is connecting once a device is picked', async ({ page }) => {
		await expect(page.getByText('Connecting to your Iti')).toBeVisible()
		await expect(page.locator('.steps [aria-current="step"] .name')).toHaveText('Connect')
		await page.evaluate(() => window.__blitiReleaseConnect())
		await expect(title(page)).toHaveText(UNNAMED)
	})

	test('can be cancelled, which returns to finding the device', async ({ page }) => {
		await page.getByRole('button', { name: 'Cancel' }).click()
		await expect(page.getByRole('button', { name: 'Find my Iti' })).toBeEnabled()
		await page.evaluate(() => window.__blitiReleaseConnect())
		await page.waitForTimeout(100)
		await expect(page.getByRole('button', { name: 'Find my Iti' })).toBeVisible()
		await expect(page.locator('.why')).toHaveCount(0)
	})
})

test('the page is titled Tamanu Iti', async ({ page }) => {
	await page.addInitScript(installFakeClient)
	await page.goto('/')
	await expect(page).toHaveTitle('Tamanu Iti')
	await expect(title(page)).toHaveText('Tamanu Iti')
})

test('a browser that cannot open a channel says so and offers nothing else', async ({ page }) => {
	await page.addInitScript(installFakeClient)
	await page.addInitScript(() => (window.__blitiClient.unsupported = () => 'Open this page in Chrome on an Android phone.'))
	await page.goto('/')
	await expect(page.getByText("This browser can't connect to an Iti")).toBeVisible()
	await expect(page.getByText('Open this page in Chrome on an Android phone.')).toBeVisible()
	await expect(page.locator('.steps')).toHaveCount(0)
	await expect(page.getByRole('button')).toHaveCount(0)
})

test.describe('the simple network screens', () => {
	test('go up one level at a time by the browser’s way back, closing the session at the Status screen', async ({ page }) => {
		await openFromBlank(page)
		await answer(page, 'configure', [message({ type: 'configuration', document: ON_CABLE, capabilities: PI }), stateOf()])
		await page.locator('.health li').first().getByRole('button').click()
		await expect(title(page)).toHaveText('Network')
		await openWay(page, 'Cable')
		await expect(title(page)).toHaveText('Cable')
		await page.goBack()
		await expect(title(page)).toHaveText('Network')
		expect(await page.evaluate(() => window.__blitiSessions.map((each) => each.closedByPage))).toEqual([false])
		await page.goBack()
		await expect(title(page)).toHaveText(UNNAMED)
		expect(await page.evaluate(() => window.__blitiSessions.map((each) => each.closedByPage))).toEqual([true])
		await page.goBack()
		await expect(page).toHaveURL('about:blank')
	})

	test('come back from the network settings on a session opened again', async ({ page }) => {
		await openFromBlank(page)
		const answered = [message({ type: 'configuration', document: ON_CABLE, capabilities: PI }), stateOf()]
		await answer(page, 'configure', answered, answered, answered)
		await answer(page, 'scan', message({ type: 'networks', 'access-points': [ap({ ssid: 'Clinic-Corp', signal: -50, security: ['enterprise'] })] }))
		await page.locator('.health li').first().getByRole('button').click()
		await openWay(page, 'Wifi')
		await page.locator('.choices button').first().click()
		await page.getByRole('button', { name: 'Open network settings' }).click()
		await expect(title(page)).toHaveText('Network settings')
		await page.getByRole('button', { name: 'Back', exact: true }).click()
		await expect(title(page)).toHaveText('Join wifi')
		await expect(page.getByText('Choose a wifi network')).toBeVisible()
		expect(await page.evaluate(() => window.__blitiSessions.map((each) => each.closedByPage))).toEqual([true, true, false])
	})
})
