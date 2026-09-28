// The Control screen of CSCR and what the application does while a device carries an act out
// (WEB), driven through a fake device at the message layer. What the page sends on a control stream
// is recorded, so "nothing is asked for" is asserted on the wire rather than inferred from the screen.

import { expect, test } from '@playwright/test'

import { IN_FORCE, PI } from './network-fixtures.js'
import {
	answer,
	closeChannel,
	controlSent,
	emit,
	message,
	openChannel,
	openNetwork,
	returns,
} from './fake-client.js'

const fact = (name, value) =>
	message({ type: 'fact', at: 1, fact: name, traits: { status: { is: 'passed' } }, kind: 'text', value })
const reading = (measurement, value) =>
	message({ type: 'reading', at: 1, measurement, traits: { status: { is: 'passed' } }, kind: 'fraction', value })

const title = (page) => page.getByRole('heading', { level: 1 })
const power = (page) => page.locator('section').filter({ has: page.getByRole('heading', { name: 'Power' }) })

/// Open the Control screen of a device that lists `acts`.
async function openControl(page, acts = ['restart', 'reboot', 'power-off']) {
	await openChannel(page)
	await answer(page, 'control', message({ type: 'acts', acts }))
	await page.getByRole('button', { name: 'Control' }).click()
	if (acts.length > 0) await power(page).waitFor()
}

/// Ask for an act from the Control screen and confirm it.
async function confirmAct(page, button) {
	await power(page).getByRole('button', { name: button }).click()
	await page.getByRole('alertdialog').getByRole('button', { name: button }).click()
}

test.describe('the device view', () => {
	test('is titled Info, with Control and Disconnect beside the title', async ({ page }) => {
		await openChannel(page)
		await expect(title(page)).toHaveText('Info')
		await expect(page.getByRole('button', { name: 'Control' })).toBeVisible()
		await expect(page.getByRole('button', { name: 'Disconnect' })).toBeVisible()
		await expect(page.getByRole('button', { name: 'Network settings' })).toHaveCount(0)
		await expect(page.getByRole('heading', { name: 'Device', level: 2 })).toBeVisible()
	})

	// Control is the view's primary action, and carries the filled button (VIEW).
	test('offers Control as its primary action', async ({ page }) => {
		await openChannel(page)
		await expect(page.getByRole('button', { name: 'Control' })).not.toHaveClass(/secondary/)
		await expect(page.getByRole('button', { name: 'Disconnect' })).toHaveClass(/secondary/)
	})
})

test.describe('the Control screen', () => {
	test('holds the network screen, and each Back returns one level', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await expect(title(page)).toHaveText('Network')
		await page.getByRole('button', { name: 'Back', exact: true }).click()
		await expect(title(page)).toHaveText('Control')
		await page.getByRole('button', { name: 'Back', exact: true }).click()
		await expect(title(page)).toHaveText('Info')
	})

	test('opens a control stream while it is open, and closes it on leaving', async ({ page }) => {
		await openControl(page)
		expect(await controlSent(page)).toEqual([{ type: 'control' }])
		await page.getByRole('button', { name: 'Back', exact: true }).click()
		expect(await page.evaluate(() => window.__blitiControls.map((each) => each.closedByPage))).toEqual([true])
	})

	test('offers the acts the device listed, in its own order, and no others', async ({ page }) => {
		await openControl(page, ['power-off', 'hibernate', 'restart'])
		await expect(power(page).locator('.acts .name')).toHaveText(['Restart bliti', 'Power off'])
	})

	// A device older than this build never answers `control`, and looks like one offering nothing.
	test('leaves out the power section until the device lists its acts, and where it lists none', async ({
		page,
	}) => {
		await openChannel(page)
		await page.getByRole('button', { name: 'Control' }).click()
		await expect(page.getByRole('button', { name: 'Network settings' })).toBeVisible()
		await expect(power(page)).toHaveCount(0)

		await page.evaluate(() => window.__blitiControl.emit({ kind: 'message', message: { type: 'acts', acts: [] } }))
		await expect(power(page)).toHaveCount(0)
	})

	// Every act, every time (CSCR).
	test('asks for nothing until the act is confirmed', async ({ page }) => {
		await openControl(page)
		for (const button of ['Restart', 'Reboot', 'Power off']) {
			await power(page).getByRole('button', { name: button }).click()
			await expect(page.getByRole('alertdialog')).toBeVisible()
			await page.getByRole('button', { name: 'Cancel' }).click()
			await expect(page.getByRole('alertdialog')).toHaveCount(0)
		}
		expect(await controlSent(page)).toEqual([{ type: 'control' }])

		await confirmAct(page, 'Reboot')
		expect(await controlSent(page)).toEqual([{ type: 'control' }, { type: 'act', act: 'reboot' }])
	})

	test('names the act in its confirmation, and says a power off stays off', async ({ page }) => {
		await openControl(page)
		await power(page).getByRole('button', { name: 'Restart' }).click()
		await expect(page.getByRole('alertdialog')).toHaveText(/^Restart bliti\?/)
		await page.getByRole('button', { name: 'Cancel' }).click()

		await power(page).getByRole('button', { name: 'Reboot' }).click()
		await expect(page.getByRole('alertdialog')).toContainText('Reboot the device?')
		await expect(page.getByRole('alertdialog')).not.toContainText('stays off')
		await page.getByRole('button', { name: 'Cancel' }).click()

		await power(page).getByRole('button', { name: 'Power off' }).click()
		await expect(page.getByRole('alertdialog')).toContainText('Power off the device?')
		await expect(page.getByRole('alertdialog')).toContainText('It stays off until it is turned on at the device.')
	})

	test('says unsaved network settings will be lost, only while the device is trying some', async ({ page }) => {
		await openControl(page)
		await power(page).getByRole('button', { name: 'Reboot' }).click()
		await expect(page.getByRole('alertdialog')).not.toContainText('not saved')
		await page.getByRole('button', { name: 'Cancel' }).click()

		await emit(page, fact('network-configuration', 'provisional'))
		await power(page).getByRole('button', { name: 'Reboot' }).click()
		await expect(page.getByRole('alertdialog')).toContainText(
			'The network settings it is trying are not saved, and will be lost.',
		)
	})

	test('says network edits not applied will be lost, and shows them kept', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await answer(page, 'control', message({ type: 'acts', acts: ['reboot'] }))
		await page.getByLabel('Country').selectOption('FJ')
		await page.getByRole('button', { name: 'Back', exact: true }).click()

		await expect(title(page)).toHaveText('Control')
		await expect(page.getByText('1 network change not applied.')).toBeVisible()
		await power(page).getByRole('button', { name: 'Reboot' }).click()
		await expect(page.getByRole('alertdialog')).toContainText('The network changes not applied will be lost.')
	})

	test('renders a refusal as the device wrote it', async ({ page }) => {
		await openControl(page)
		await answer(page, 'act', message({ type: 'refused', reason: 'the device is already rebooting' }))
		await confirmAct(page, 'Power off')
		await expect(power(page).getByText('the device is already rebooting')).toBeVisible()
		await expect(title(page)).toHaveText('Control')
	})
})

test.describe('when the device goes away', () => {
	test('an act accepted keeps the title and the device named, in place of the tiles', async ({ page }) => {
		await openChannel(page)
		await emit(page, fact('hostname', 'tamanu-iti-04'))
		await emit(page, reading('cpu-usage', 0.12))
		await answer(page, 'control', message({ type: 'acts', acts: ['reboot'] }))
		await page.getByRole('button', { name: 'Control' }).click()
		await answer(page, 'act', message({ type: 'accepted' }))
		await confirmAct(page, 'Reboot')

		await expect(title(page)).toHaveText('Info')
		await expect(page.getByRole('status')).toHaveText('Rebooting…')
		await expect(page.getByText('tamanu-iti-04')).toBeVisible()
		await expect(page.getByText('12%')).toHaveCount(0)
		await expect(page.getByRole('button', { name: 'Disconnect' })).toBeVisible()
		await expect(page.getByRole('button', { name: 'Control' })).toHaveCount(0)
	})

	// Another operator's act reaches this one on the feed (CTL).
	test('an act announced on the feed is said to be under way, whoever asked for it', async ({ page }) => {
		await openChannel(page)
		await emit(page, message({ type: 'going-away', act: 'restart' }))
		await expect(page.getByRole('status')).toHaveText('Restarting bliti…')
	})

	test('an act this build does not know is left to end the channel as anything else does', async ({ page }) => {
		await openChannel(page)
		await emit(page, message({ type: 'going-away', act: 'hibernate' }))
		await expect(page.getByRole('status')).toHaveCount(0)
		await expect(page.getByRole('button', { name: 'Control' })).toBeVisible()
	})

	test('after a reboot, the device is reached again on its own once it is back', async ({ page }) => {
		await openChannel(page)
		await emit(page, message({ type: 'going-away', act: 'reboot' }))
		await returns(page, 'fail', 'fail', 'ok')
		await closeChannel(page)

		await expect(page.getByRole('status')).toHaveText('Rebooting…')
		await expect(page.getByRole('button', { name: 'Control' })).toBeVisible()
		await expect(page.getByRole('status')).toHaveCount(0)
		expect(await page.evaluate(() => window.__blitiReconnects)).toBe(3)

		await emit(page, reading('cpu-usage', 0.3))
		await expect(page.getByText('30%')).toBeVisible()
	})

	test('a device that does not come back is offered to be found again', async ({ page }) => {
		await openChannel(page)
		await emit(page, message({ type: 'going-away', act: 'restart' }))
		await closeChannel(page)

		await expect(page.getByText('The device has not come back. Check it is on and nearby.')).toBeVisible()
		await expect(page.getByRole('button', { name: 'Find the device' })).toBeVisible()
		expect(await page.evaluate(() => window.__blitiReconnects)).toBeGreaterThan(1)
	})

	test('a device the browser can only reach through the chooser is offered to be found again', async ({
		page,
	}) => {
		await openChannel(page)
		await emit(page, message({ type: 'going-away', act: 'reboot' }))
		await returns(page, 'chooser')
		await closeChannel(page)

		await expect(page.getByText('The device has to be picked again.')).toBeVisible()
		await expect(page.getByRole('button', { name: 'Find the device' })).toBeVisible()
	})

	test('disconnecting stops the attempts to reach it', async ({ page }) => {
		await openChannel(page)
		await emit(page, message({ type: 'going-away', act: 'reboot' }))
		await closeChannel(page)
		await page.getByRole('button', { name: 'Disconnect' }).click()

		await expect(page.getByRole('button', { name: 'Find the device' })).toBeVisible()
		const attempts = await page.evaluate(() => window.__blitiReconnects)
		await page.waitForTimeout(300)
		expect(await page.evaluate(() => window.__blitiReconnects)).toBe(attempts)
		await expect(page.getByRole('status')).toHaveCount(0)
	})

	test('a power off is said to be under way for a moment, and nothing reconnects', async ({ page }) => {
		await openChannel(page)
		await emit(page, message({ type: 'going-away', act: 'power-off' }))
		await expect(page.getByRole('status')).toHaveText('Shutting down…')
		await closeChannel(page)

		await expect(page.getByRole('status')).toHaveText('Shutting down…')
		await expect(page.getByRole('button', { name: 'Disconnect' })).toBeVisible()
		await expect(page.getByRole('button', { name: 'Find the device' })).toBeVisible()
		await expect(page.getByRole('status')).toHaveCount(0)
		await expect(page.getByText('turned off')).toHaveCount(0)
		expect(await page.evaluate(() => window.__blitiReconnects)).toBe(0)
	})
})
