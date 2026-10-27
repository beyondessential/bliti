// The acts of the Status screen's Tailscale and Tamanu rows (STAT), over a Tailscale stream and a
// Tamanu stream the screen holds while it shows, driven through a fake device at the message layer.

import { expect, test } from '@playwright/test'

import {
	actStreams,
	feed,
	message,
	openAdvanced,
	openChannel,
	openReadings,
	sayOnTailscale,
	sayOnTamanu,
	standingActs,
	tailscaleSent,
	tamanuSent,
} from './fake-client.js'

const fact = (name, value, is = 'passed', reason) =>
	message({ type: 'fact', at: 1, fact: name, kind: 'text', value, traits: { status: { is, ...(reason ? { reason } : {}) } } })
const broken = (name, reason) => message({ type: 'fact', at: 1, fact: name, kind: 'text', traits: { status: { is: 'broken', reason } } })

const row = (page, name) => page.locator('.health li').filter({ has: page.locator('.name', { hasText: name }) })
const tailscaleRow = (page) => row(page, 'Tailscale')
const tamanuRow = (page) => row(page, 'Tamanu')

// A device whose Tailscale is in `tailscale` and Tamanu in `tamanu`, listing `acts` on each stream.
async function open(page, { tailscale = 'running', tamanu = 'running', acts = {} } = {}) {
	await openChannel(page, fact('tailscale-state', tailscale), fact('tamanu-state', tamanu))
	await standingActs(page, acts)
	// The screen opened its streams with the channel, before the acts were scripted: answer those.
	if (acts.tailscale) await sayOnTailscale(page, { type: 'tailscale-acts', acts: acts.tailscale })
	if (acts.tamanu) await sayOnTamanu(page, { type: 'tamanu-acts', acts: acts.tamanu })
}

test.describe('the streams', () => {
	test('open as the Status screen shows, and close as the operator leaves it', async ({ page }) => {
		await open(page)
		await expect.poll(async () => (await actStreams(page)).tailscale.length).toBe(1)
		expect((await actStreams(page)).tamanu).toHaveLength(1)
		expect((await tailscaleSent(page))[0]).toEqual({ type: 'tailscale' })
		expect((await tamanuSent(page))[0]).toEqual({ type: 'tamanu' })
		await openReadings(page)
		const streams = await actStreams(page)
		expect(streams.tailscale[0]).toEqual({ open: false, closedByPage: true })
		expect(streams.tamanu[0]).toEqual({ open: false, closedByPage: true })
	})

	test('leave both rows without an act where the device never answers, the rows still read from the feed', async ({ page }) => {
		await openChannel(page, fact('tailscale-state', 'needs-login'), fact('tamanu-state', 'stopped'))
		await expect(tailscaleRow(page).locator('.name')).toHaveText('Tailscale not signed in')
		await expect(tamanuRow(page).locator('.name')).toHaveText('Tamanu stopped')
		await expect(tailscaleRow(page).getByRole('button')).toHaveCount(0)
		await expect(tamanuRow(page).getByRole('button')).toHaveCount(0)
	})

	test('offer no act the device has not listed, and follow a new list without the operator', async ({ page }) => {
		await open(page, { tamanu: 'running', acts: { tamanu: [] } })
		await expect(tamanuRow(page).getByRole('button')).toHaveCount(0)
		await sayOnTamanu(page, { type: 'tamanu-acts', acts: ['restart'] })
		await expect(tamanuRow(page).getByRole('button', { name: 'Restart' })).toBeVisible()
	})
})

test.describe('the Tamanu row', () => {
	test('confirms Start in place of the button, the other rows still in view, and Cancel sends nothing', async ({ page }) => {
		await open(page, { tamanu: 'stopped', acts: { tamanu: ['start'] } })
		await tamanuRow(page).getByRole('button', { name: 'Start' }).click()
		const confirming = tamanuRow(page).getByRole('alertdialog', { name: 'Start Tamanu?' })
		await expect(confirming).toBeVisible()
		await expect(tailscaleRow(page)).toBeVisible()
		await confirming.getByRole('button', { name: 'Cancel' }).click()
		await expect(confirming).toHaveCount(0)
		await expect(tamanuRow(page).getByRole('button', { name: 'Start' })).toBeVisible()
		expect(await tamanuSent(page)).toEqual([{ type: 'tamanu' }])
	})

	test('sends start once confirmed, and holds the button back until the device answers', async ({ page }) => {
		await open(page, { tamanu: 'stopped', acts: { tamanu: ['start'] } })
		await tamanuRow(page).getByRole('button', { name: 'Start' }).click()
		await tamanuRow(page).getByRole('alertdialog').getByRole('button', { name: 'Start' }).click()
		expect((await tamanuSent(page)).at(-1)).toEqual({ type: 'tamanu-act', act: 'start' })
		await expect(tamanuRow(page).getByRole('button', { name: 'Start' })).toBeDisabled()
		await sayOnTamanu(page, { type: 'tamanu-accepted' })
		await expect(tamanuRow(page).getByRole('button', { name: 'Start' })).toBeDisabled()
		await sayOnTamanu(page, { type: 'tamanu-acts', acts: ['start'] })
		await expect(tamanuRow(page).getByRole('button', { name: 'Start' })).toBeEnabled()
	})

	for (const state of ['running', 'degraded']) {
		test(`confirms Restart while ${state}, saying Tamanu may be unavailable`, async ({ page }) => {
			await open(page, { tamanu: state, acts: { tamanu: ['restart'] } })
			await tamanuRow(page).getByRole('button', { name: 'Restart' }).click()
			const confirming = tamanuRow(page).getByRole('alertdialog', { name: 'Restart Tamanu?' })
			await expect(confirming).toContainText('Tamanu may be unavailable while it restarts.')
			await confirming.getByRole('button', { name: 'Restart' }).click()
			expect((await tamanuSent(page)).at(-1)).toEqual({ type: 'tamanu-act', act: 'restart' })
		})
	}

	test('shows restarting and starting from the feed, with no act', async ({ page }) => {
		await open(page, { tamanu: 'running', acts: { tamanu: ['restart'] } })
		for (const [state, says] of [
			['restarting', 'Tamanu restarting'],
			['starting', 'Tamanu starting'],
		]) {
			await feed(page, fact('tamanu-state', state))
			await expect(tamanuRow(page).locator('.name')).toHaveText(says)
			await expect(tamanuRow(page).locator('.mark.wait')).toBeVisible()
			await expect(tamanuRow(page).getByRole('button')).toHaveCount(0)
		}
	})

	test('shows a refusal and a failure in its detail as the device wrote them, until asked again', async ({ page }) => {
		await open(page, { tamanu: 'running', acts: { tamanu: ['restart'] } })
		const restart = async () => {
			await tamanuRow(page).getByRole('button', { name: 'Restart' }).click()
			await tamanuRow(page).getByRole('alertdialog').getByRole('button', { name: 'Restart' }).click()
		}
		await restart()
		await sayOnTamanu(page, { type: 'cannot-act', act: 'restart', at: "$['act']", reason: 'An update is being installed.' })
		await expect(tamanuRow(page).locator('.detail.reason')).toHaveText('An update is being installed.')
		await expect(tamanuRow(page).getByRole('button', { name: 'Restart' })).toBeEnabled()
		await restart()
		await expect(tamanuRow(page).locator('.detail.reason')).toHaveCount(0)
		await sayOnTamanu(page, { type: 'tamanu-accepted' })
		await sayOnTamanu(page, { type: 'tamanu-failed', reason: 'facility-server.service failed to start' })
		await expect(tamanuRow(page).locator('.detail.reason')).toHaveText('facility-server.service failed to start')
	})

	test('leaves the refusal behind once the operator leaves the Status screen', async ({ page }) => {
		await open(page, { tamanu: 'running', acts: { tamanu: ['restart'] } })
		await tamanuRow(page).getByRole('button', { name: 'Restart' }).click()
		await tamanuRow(page).getByRole('alertdialog').getByRole('button', { name: 'Restart' }).click()
		await sayOnTamanu(page, { type: 'cannot-act', act: 'restart', at: "$['act']", reason: 'Busy.' })
		await expect(tamanuRow(page)).toContainText('Busy.')
		await openReadings(page)
		await page.getByRole('button', { name: 'Back' }).click()
		await expect(tamanuRow(page).getByRole('button', { name: 'Restart' })).toBeVisible()
		await expect(tamanuRow(page)).not.toContainText('Busy.')
	})

	test('offers nothing absent, unknown, or with no state', async ({ page }) => {
		await open(page, { tamanu: 'absent', acts: { tamanu: ['start', 'restart'] } })
		await expect(tamanuRow(page).getByRole('button')).toHaveCount(0)
		await feed(page, broken('tamanu-state', 'cannot tell'))
		await expect(tamanuRow(page).locator('.name')).toHaveText('Tamanu state unknown')
		await expect(tamanuRow(page).getByRole('button')).toHaveCount(0)
	})
})

const ALL = ['sign-in', 'turn-on', 'turn-off', 'log-out', 'netcheck', 'ping']

test.describe('the Tailscale row', () => {
	for (const state of ['needs-login', 'expired']) {
		test(`offers Connect in ${state}, which opens Connect to Tailscale`, async ({ page }) => {
			await open(page, { tailscale: state, acts: { tailscale: ['sign-in', 'netcheck'] } })
			await tailscaleRow(page).getByRole('button', { name: 'Connect' }).click()
			await expect(page.getByRole('heading', { level: 1 })).toHaveText('Connect to Tailscale')
			expect((await tailscaleSent(page)).filter((each) => each.type === 'tailscale-act')).toEqual([])
		})
	}

	test('offers Turn on while turned off, sent without a confirmation', async ({ page }) => {
		await open(page, { tailscale: 'stopped', acts: { tailscale: ['turn-on', 'log-out', 'netcheck'] } })
		await tailscaleRow(page).getByRole('button', { name: 'Turn on' }).click()
		await expect(page.getByRole('alertdialog')).toHaveCount(0)
		expect((await tailscaleSent(page)).at(-1)).toEqual({ type: 'tailscale-act', act: 'turn-on' })
	})

	test('offers Network check while connected and while its state is unknown', async ({ page }) => {
		await open(page, { tailscale: 'running', acts: { tailscale: ALL } })
		await expect(tailscaleRow(page).getByRole('button')).toHaveText(['Network check'])
		await feed(page, broken('tailscale-state', 'tailscaled is not responding.'))
		await expect(tailscaleRow(page).getByRole('button')).toHaveText(['Network check'])
	})

	test('offers nothing while connecting, though the device lists acts', async ({ page }) => {
		await open(page, { tailscale: 'starting', acts: { tailscale: ['turn-off', 'log-out', 'netcheck'] } })
		await expect(tailscaleRow(page).locator('.name')).toHaveText('Tailscale connecting')
		await expect(tailscaleRow(page).getByRole('button')).toHaveCount(0)
	})

	test('confirms Cancel while waiting for approval, preferring to keep waiting', async ({ page }) => {
		await open(page, { tailscale: 'needs-approval', acts: { tailscale: ['log-out', 'netcheck'] } })
		await tailscaleRow(page).getByRole('button', { name: 'Cancel' }).click()
		const confirming = tailscaleRow(page).getByRole('alertdialog', { name: 'Cancel the sign-in?' })
		await expect(confirming.getByRole('button', { name: 'Keep waiting' })).not.toHaveClass(/secondary/)
		await expect(confirming.getByRole('button', { name: 'Cancel sign-in' })).toHaveClass(/secondary/)
		await confirming.getByRole('button', { name: 'Keep waiting' }).click()
		expect((await tailscaleSent(page)).filter((each) => each.type === 'tailscale-act')).toEqual([])
		await tailscaleRow(page).getByRole('button', { name: 'Cancel' }).click()
		await tailscaleRow(page).getByRole('button', { name: 'Cancel sign-in' }).click()
		expect((await tailscaleSent(page)).at(-1)).toEqual({ type: 'tailscale-act', act: 'log-out' })
	})

	test('shows a network check under way, then that the device can connect, through a relay alike', async ({ page }) => {
		await open(page, { acts: { tailscale: ALL } })
		await tailscaleRow(page).getByRole('button', { name: 'Network check' }).click()
		expect((await tailscaleSent(page)).at(-1)).toEqual({ type: 'tailscale-act', act: 'netcheck' })
		await expect(tailscaleRow(page).getByRole('status')).toHaveText('Checking…')
		await expect(tailscaleRow(page).getByRole('button', { name: 'Network check' })).toHaveCount(0)
		await sayOnTailscale(page, { type: 'tailscale-accepted' })
		await sayOnTailscale(page, {
			type: 'tailscale-result',
			result: { udp: false, ipv4: true, ipv6: false, 'nat-varies': null, 'captive-portal': false, relays: [{ id: 5, name: 'Sydney', latency: 0.12 }], report: '{}' },
		})
		await expect(tailscaleRow(page).locator('.detail').filter({ hasText: 'Can connect' })).toBeVisible()
		await expect(tailscaleRow(page).getByRole('button', { name: 'Network check' })).toBeEnabled()
	})

	test('says why the device is blocked, and Details opens the Tailscale screen on that result', async ({ page }) => {
		await open(page, { acts: { tailscale: ALL } })
		const check = async (result) => {
			await tailscaleRow(page).getByRole('button', { name: 'Network check' }).click()
			await sayOnTailscale(page, { type: 'tailscale-result', result: { udp: false, ipv4: true, ipv6: false, relays: [], report: '{}', ...result } })
		}
		const said = tailscaleRow(page).locator('.detail.reason')
		await check({ 'captive-portal': true })
		await expect(said).toContainText('Blocked: the network asks for a sign-in')
		await check({ 'captive-portal': null, ipv4: false })
		await expect(said).toContainText('Blocked: no internet')
		await check({ 'captive-portal': false })
		await expect(said).toContainText("Blocked: Tailscale can't be reached from this network")
		await tailscaleRow(page).getByRole('button', { name: 'Details' }).click()
		await expect(page.getByRole('heading', { level: 1 })).toHaveText('Tailscale')
		await expect(page.locator('.verdict')).toHaveText("Blocked: Tailscale can't be reached from this network")
	})

	test('keeps the verdict until the check runs again or the operator leaves', async ({ page }) => {
		await open(page, { acts: { tailscale: ALL } })
		await tailscaleRow(page).getByRole('button', { name: 'Network check' }).click()
		await sayOnTailscale(page, { type: 'tailscale-result', result: { relays: [{ id: 1, name: null, latency: 0.1 }], report: '' } })
		const verdict = tailscaleRow(page).getByText('Can connect')
		await expect(verdict).toBeVisible()
		await feed(page, fact('tailscale-name', 'clinic-iti-07'))
		await expect(verdict).toBeVisible()
		await tailscaleRow(page).getByRole('button', { name: 'Network check' }).click()
		await expect(verdict).toHaveCount(0)
		await sayOnTailscale(page, { type: 'tailscale-result', result: { relays: [{ id: 1, name: null, latency: 0.1 }], report: '' } })
		await expect(verdict).toBeVisible()
		await openReadings(page)
		await page.getByRole('button', { name: 'Back' }).click()
		await expect(tailscaleRow(page).getByText('Can connect')).toHaveCount(0)
	})

	test('shows a failed network check in its detail', async ({ page }) => {
		await open(page, { acts: { tailscale: ALL } })
		await tailscaleRow(page).getByRole('button', { name: 'Network check' }).click()
		await sayOnTailscale(page, { type: 'tailscale-failed', reason: 'the network check took too long' })
		await expect(tailscaleRow(page).locator('.detail.reason')).toHaveText('the network check took too long')
		await expect(tailscaleRow(page).getByRole('status')).toHaveCount(0)
	})
})

test.describe('Advanced', () => {
	test('holds All readings, Network settings, Tailscale, Power and battery, in that order', async ({ page }) => {
		await open(page)
		await openAdvanced(page)
		await expect(page.locator('.advanced .inside button')).toHaveText(['All readings', 'Network settings', 'Tailscale', 'Power and battery'])
	})

	test('holds no Tailscale where the device reports no tailscale-state', async ({ page }) => {
		await openChannel(page, fact('tamanu-state', 'running'))
		await openAdvanced(page)
		await expect(page.locator('.advanced .inside button')).toHaveText(['All readings', 'Network settings', 'Power and battery'])
	})
})
