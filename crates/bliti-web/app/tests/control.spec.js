// The Control screen of CSCR and what the application does while a device carries an act out
// (WEB), driven through a fake device at the message layer. What the page sends on a power stream
// is recorded, so "nothing is asked for" is asserted on the wire rather than inferred from the screen.

import { readFile } from 'node:fs/promises'

import { expect, test } from '@playwright/test'

import { IN_FORCE, PI } from './network-fixtures.js'
import {
	answer,
	closeChannel,
	curveSent,
	emit,
	message,
	openChannel,
	openNetwork,
	powerSent,
	returns,
	sayOnCurve,
} from './fake-client.js'

const fact = (name, value) =>
	message({ type: 'fact', at: 1, fact: name, traits: { status: { is: 'passed' } }, kind: 'text', value })
const reading = (measurement, value) =>
	message({ type: 'reading', at: 1, measurement, traits: { status: { is: 'passed' } }, kind: 'fraction', value })

const title = (page) => page.getByRole('heading', { level: 1 })
const power = (page) => page.locator('section').filter({ has: page.getByRole('heading', { name: 'Power' }) })
const battery = (page) => page.locator('section').filter({ has: page.getByRole('heading', { name: 'Battery' }) })

// A curve document as a device carries one: a learnt discharging curve and a charging curve (CRV).
const DOCUMENT = {
	discharging: { points: [[2.571, 0], [2.8, 0.035], [4.2, 1]], 'learnt-from': 4, error: 0.05, duration: 19800 },
	charging: { points: [[3.3, 0], [4.2, 1]], 'learnt-from': 3, error: 0.1, duration: 13200 },
}
// 5h 10m (±12m) and 3h 40m (±20m).
const CURVES = message({
	type: 'curves',
	document: DOCUMENT,
	lasts: { duration: 18600, margin: 720 },
	recharge: { duration: 13200, margin: 1200 },
})

/// Open the Control screen of a device that lists `acts`.
async function openControl(page, acts = ['restart', 'reboot', 'power-off'], { open = true } = {}) {
	if (open) await openChannel(page)
	await answer(page, 'power', message({ type: 'acts', acts }))
	await page.getByRole('button', { name: 'Control' }).click()
	if (acts.length > 0) await power(page).waitFor()
}

/// Open the Control screen of a device that lists every act and answers `curve` with `curves`.
async function openBattery(page, curves = CURVES) {
	await openChannel(page)
	await answer(page, 'curve', curves)
	await openControl(page, undefined, { open: false })
	await battery(page).waitFor()
}

/// Pick a file for import, holding `text`.
async function pickFile(page, name, text) {
	const choosing = page.waitForEvent('filechooser')
	await battery(page).getByRole('button', { name: 'Import' }).click()
	await (await choosing).setFiles({ name, mimeType: 'application/json', buffer: Buffer.from(text) })
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

	test('opens a power stream and a curve stream while it is open, and closes both on leaving', async ({ page }) => {
		await openControl(page)
		expect(await powerSent(page)).toEqual([{ type: 'power' }])
		expect(await curveSent(page)).toEqual([{ type: 'curve' }])
		await page.getByRole('button', { name: 'Back', exact: true }).click()
		expect(await page.evaluate(() => window.__blitiPowerStreams.map((each) => each.closedByPage))).toEqual([true])
		expect(await page.evaluate(() => window.__blitiCurveStreams.map((each) => each.closedByPage))).toEqual([true])
	})

	test('holds its sections in the order network, power, battery', async ({ page }) => {
		await openBattery(page)
		await expect(page.locator('section h2').filter({ hasNotText: 'Activity' })).toHaveText(['Network', 'Power', 'Battery'])
	})

	test('offers the acts the device listed, in its own order, and no others', async ({ page }) => {
		await openControl(page, ['power-off', 'hibernate', 'restart'])
		await expect(power(page).locator('.acts .name')).toHaveText(['Restart bliti', 'Power off'])
	})

	// A device older than this build never answers `power`, and looks like one offering nothing.
	test('leaves out the power section until the device lists its acts, and where it lists none', async ({
		page,
	}) => {
		await openChannel(page)
		await page.getByRole('button', { name: 'Control' }).click()
		await expect(page.getByRole('button', { name: 'Network settings' })).toBeVisible()
		await expect(power(page)).toHaveCount(0)

		await page.evaluate(() => window.__blitiPowerStream.emit({ kind: 'message', message: { type: 'acts', acts: [] } }))
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
		expect(await powerSent(page)).toEqual([{ type: 'power' }])

		await confirmAct(page, 'Reboot')
		expect(await powerSent(page)).toEqual([{ type: 'power' }, { type: 'act', act: 'reboot' }])
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
		await answer(page, 'power', message({ type: 'acts', acts: ['reboot'] }))
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

test.describe('the battery section', () => {
	test('says how long a full charge lasts and a full recharge takes, each with its margin', async ({ page }) => {
		await openBattery(page)
		await expect(battery(page).locator('.learnt').first()).toHaveText(
			'A full charge lasts 5h 10m (±12m).A full recharge takes 3h 40m (±20m).',
		)
		await expect(battery(page)).toContainText(
			'The device learns its battery curve over time, which improves these estimates.',
		)
		await expect(battery(page).locator('.acts .name')).toHaveText(['Export curve', 'Import curve', 'Reset curve'])
	})

	test('says nothing of a recharge where the device gives none', async ({ page }) => {
		await openBattery(
			page,
			message({ type: 'curves', document: { discharging: DOCUMENT.discharging }, lasts: { duration: 18600, margin: 2700 } }),
		)
		await expect(battery(page).locator('.learnt').first()).toHaveText('A full charge lasts 5h 10m (±45m).')
		await expect(battery(page)).not.toContainText('recharge')
	})

	// A device older than this build never answers `curve`, and one managing no backup supply answers
	// with no document; both look like a device with nothing to offer here (CSCR).
	test('is left out until the device answers, and where it answers with no document', async ({ page }) => {
		await openControl(page)
		await expect(power(page)).toBeVisible()
		await expect(battery(page)).toHaveCount(0)

		await sayOnCurve(page, message({ type: 'curves' }))
		await expect(battery(page)).toHaveCount(0)

		await sayOnCurve(page, CURVES)
		await expect(battery(page)).toBeVisible()
	})

	test('shows the figures from the latest curves the device sent', async ({ page }) => {
		await openBattery(page)
		await sayOnCurve(
			page,
			message({ type: 'curves', document: DOCUMENT, lasts: { duration: 19800, margin: 300 }, recharge: { duration: 12600, margin: 600 } }),
		)
		await expect(battery(page).locator('.learnt').first()).toHaveText(
			'A full charge lasts 5h 30m (±5m).A full recharge takes 3h 30m (±10m).',
		)
	})

	test('exports the curve document as a file', async ({ page }) => {
		await openBattery(page)
		const downloading = page.waitForEvent('download')
		await battery(page).getByRole('button', { name: 'Export' }).click()
		const download = await downloading
		expect(download.suggestedFilename()).toBe('battery-curve.json')
		expect(JSON.parse(await readFile(await download.path(), 'utf8'))).toEqual(DOCUMENT)
	})

	// Every import, every time (CSCR).
	test('asks for no import until it is confirmed, and says what the device has learnt is replaced', async ({
		page,
	}) => {
		await openBattery(page)
		const picked = { discharging: { ...DOCUMENT.discharging, 'learnt-from': 9 } }
		await pickFile(page, 'from-site-4.json', JSON.stringify(picked))
		const dialog = page.getByRole('alertdialog')
		await expect(dialog).toContainText('Import from-site-4.json?')
		await expect(dialog).toContainText('It replaces what the device has learnt.')
		await dialog.getByRole('button', { name: 'Cancel' }).click()
		await expect(dialog).toHaveCount(0)
		expect(await curveSent(page)).toEqual([{ type: 'curve' }])

		await pickFile(page, 'from-site-4.json', JSON.stringify(picked))
		await answer(page, 'load', message({ type: 'accepted' }))
		await page.getByRole('alertdialog').getByRole('button', { name: 'Import' }).click()
		expect(await curveSent(page)).toEqual([{ type: 'curve' }, { type: 'load', document: picked }])
		await expect(battery(page).getByRole('button', { name: 'Import' })).toBeEnabled()
	})

	test('asks for no reset until it is confirmed, and says what the device has learnt is discarded', async ({
		page,
	}) => {
		await openBattery(page)
		await battery(page).getByRole('button', { name: 'Reset' }).click()
		const dialog = page.getByRole('alertdialog')
		await expect(dialog).toContainText('Reset the charge curve?')
		await expect(dialog).toContainText('What the device has learnt is discarded.')
		await dialog.getByRole('button', { name: 'Cancel' }).click()
		await expect(dialog).toHaveCount(0)
		expect(await curveSent(page)).toEqual([{ type: 'curve' }])

		await battery(page).getByRole('button', { name: 'Reset' }).click()
		await page.getByRole('alertdialog').getByRole('button', { name: 'Reset' }).click()
		expect(await curveSent(page)).toEqual([{ type: 'curve' }, { type: 'reset' }])
	})

	test('renders a refused import as the device wrote it', async ({ page }) => {
		await openBattery(page)
		await answer(
			page,
			'load',
			message({ type: 'refused', reason: "the discharging curve's first point is at 3.1 V, above the 2.8 V floor" }),
		)
		await pickFile(page, 'battery-curve.json', JSON.stringify({ discharging: { points: [[3.1, 0], [4.2, 1]] } }))
		await page.getByRole('alertdialog').getByRole('button', { name: 'Import' }).click()
		await expect(
			battery(page).getByText("the discharging curve's first point is at 3.1 V, above the 2.8 V floor"),
		).toBeVisible()
		await expect(battery(page).getByRole('button', { name: 'Import' })).toBeEnabled()
	})

	test('asks for nothing with a file that is not JSON', async ({ page }) => {
		await openBattery(page)
		await pickFile(page, 'notes.txt', 'not a curve')
		await expect(page.getByRole('alertdialog')).toHaveCount(0)
		await expect(battery(page).getByText('notes.txt is not JSON.')).toBeVisible()
		expect(await curveSent(page)).toEqual([{ type: 'curve' }])
	})
})

test.describe('when the device goes away', () => {
	test('an act accepted keeps the title and the device named, in place of the tiles', async ({ page }) => {
		await openChannel(page)
		await emit(page, fact('hostname', 'tamanu-iti-04'))
		await emit(page, reading('cpu-usage', 0.12))
		await answer(page, 'power', message({ type: 'acts', acts: ['reboot'] }))
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
		await emit(page, message({ type: 'going-away', act: 'restart', cause: 'manual-control' }))
		await expect(page.getByRole('status')).toHaveText('Restarting bliti…')
	})

	test('an act this build does not know is left to end the channel as anything else does', async ({ page }) => {
		await openChannel(page)
		await emit(page, message({ type: 'going-away', act: 'hibernate', cause: 'manual-control' }))
		await expect(page.getByRole('status')).toHaveCount(0)
		await expect(page.getByRole('button', { name: 'Control' })).toBeVisible()
	})

	test('after a reboot, the device is reached again on its own once it is back', async ({ page }) => {
		await openChannel(page)
		await emit(page, message({ type: 'going-away', act: 'reboot', cause: 'manual-control' }))
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
		await emit(page, message({ type: 'going-away', act: 'restart', cause: 'manual-control' }))
		await closeChannel(page)

		await expect(page.getByText('The device has not come back. Check it is on and nearby.')).toBeVisible()
		await expect(page.getByRole('button', { name: 'Find the device' })).toBeVisible()
		expect(await page.evaluate(() => window.__blitiReconnects)).toBeGreaterThan(1)
	})

	test('a device the browser can only reach through the chooser is offered to be found again', async ({
		page,
	}) => {
		await openChannel(page)
		await emit(page, message({ type: 'going-away', act: 'reboot', cause: 'manual-control' }))
		await returns(page, 'chooser')
		await closeChannel(page)

		await expect(page.getByText('The device has to be picked again.')).toBeVisible()
		await expect(page.getByRole('button', { name: 'Find the device' })).toBeVisible()
	})

	test('disconnecting stops the attempts to reach it', async ({ page }) => {
		await openChannel(page)
		await emit(page, message({ type: 'going-away', act: 'reboot', cause: 'manual-control' }))
		await closeChannel(page)
		await page.getByRole('button', { name: 'Disconnect' }).click()

		await expect(page.getByRole('button', { name: 'Find the device' })).toBeVisible()
		const attempts = await page.evaluate(() => window.__blitiReconnects)
		await page.waitForTimeout(300)
		expect(await page.evaluate(() => window.__blitiReconnects)).toBe(attempts)
		await expect(page.getByRole('status')).toHaveCount(0)
	})

	test('a low-battery shutdown says the battery is low', async ({ page }) => {
		await openChannel(page)
		await emit(page, message({ type: 'going-away', act: 'power-off', cause: 'low-battery' }))
		await expect(page.getByRole('status')).toHaveText('Shutting down…Its battery is low.')
	})

	// A cause this build does not know is read by its act alone.
	test('an act with a cause this build does not know is said to be under way as any other', async ({ page }) => {
		await openChannel(page)
		await emit(page, message({ type: 'going-away', act: 'power-off', cause: 'overheating' }))
		await expect(page.getByRole('status')).toHaveText('Shutting down…')
	})

	test('a power off is said to be under way for a moment, and nothing reconnects', async ({ page }) => {
		await openChannel(page)
		await emit(page, message({ type: 'going-away', act: 'power-off', cause: 'manual-control' }))
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
