// The Tailscale screen of TSCR, under Advanced: how the device stands in Tailscale, every act it
// lists, the network check in full and a ping, over a Tailscale stream of its own.

import { expect, test } from '@playwright/test'

import {
	actStreams,
	message,
	openAdvanced,
	openChannel,
	sayOnTailscale,
	standingActs,
	tailscaleSent,
} from './fake-client.js'

const fact = (name, value, is = 'passed', reason) =>
	message({ type: 'fact', at: 1, fact: name, kind: 'text', value, traits: { status: { is, ...(reason ? { reason } : {}) } } })

const title = (page) => page.getByRole('heading', { level: 1 })
const section = (page, name) => page.locator('section').filter({ has: page.getByRole('heading', { name, exact: true }) })

// What the device writes into the clipboard and passes the share sheet, where the page gives either.
function recordHandingOn(withShareSheet) {
	window.__copied = []
	window.__shared = []
	Object.defineProperty(navigator, 'clipboard', { value: { writeText: async (text) => window.__copied.push(text) } })
	if (withShareSheet) Object.defineProperty(navigator, 'share', { value: async (data) => window.__shared.push(data) })
	else Object.defineProperty(navigator, 'share', { value: undefined })
}

// The Tailscale screen of a device in `state`, listing `acts` on every Tailscale stream it opens.
async function openTailscale(page, { state = 'running', acts = [], facts = [], handOn } = {}) {
	await page.addInitScript(recordHandingOn, handOn === 'sheet')
	await openChannel(page, fact('hostname', 'clinic-iti-07'), fact('tailscale-state', state), ...facts)
	await standingActs(page, { tailscale: acts })
	await openAdvanced(page)
	await page.getByRole('button', { name: 'Tailscale', exact: true }).click()
	await expect(title(page)).toHaveText('Tailscale')
}

const RESULT = {
	udp: true,
	ipv4: true,
	ipv6: false,
	'nat-varies': true,
	'captive-portal': false,
	relays: [
		{ id: 5, name: 'Sydney', latency: 0.032 },
		{ id: 12, name: null, latency: 0.041 },
	],
	report: '{\n\t"UDP": true,\n\t"PMP": false\n}\n',
}

test('is under Advanced, titled Tailscale with Back, which returns to the Status screen as the browser does', async ({ page }) => {
	await openTailscale(page)
	await page.getByRole('button', { name: 'Back' }).click()
	await expect(title(page)).toHaveText('clinic-iti-07')
	await openAdvanced(page)
	await page.getByRole('button', { name: 'Tailscale', exact: true }).click()
	await page.goBack()
	await expect(title(page)).toHaveText('clinic-iti-07')
})

test('opens a Tailscale stream of its own, and closes it on leaving', async ({ page }) => {
	await openTailscale(page)
	// The Status screen's, closed, then the screen's own, open.
	await expect.poll(async () => (await actStreams(page)).tailscale.map((each) => each.open)).toEqual([false, true])
	await page.getByRole('button', { name: 'Back' }).click()
	await expect.poll(async () => (await actStreams(page)).tailscale[1]).toEqual({ open: false, closedByPage: true })
})

test('details the state as the row does, then the name, tailnet and sign-in lapse, each where it has a value', async ({ page }) => {
	const lapse = new Date(Date.now() + 40.5 * 86_400_000).toISOString()
	await openTailscale(page, {
		facts: [fact('tailscale-name', 'clinic-iti-07'), fact('tailnet', 'example-health.ts.net'), fact('tailscale-key-expiry', lapse)],
	})
	const details = section(page, 'Details')
	await expect(details.locator('.health .name')).toHaveText('Connected')
	await expect(details.locator('.health .mark.good')).toBeVisible()
	await expect(details.locator('dt')).toHaveText(['Device name', 'Tailnet', 'Sign-in lapses'])
	await expect(details.locator('dd').nth(2)).toContainText('in 40 days')
})

test('details a state with its reason, and leaves out what carries no value', async ({ page }) => {
	await openTailscale(page, { state: 'running', facts: [fact('tailscale-state', 'running', 'warning', 'No route to the relay.')] })
	const details = section(page, 'Details')
	await expect(details.locator('.health .name')).toHaveText('Connected')
	await expect(details.locator('.detail.reason')).toHaveText('No route to the relay.')
	await expect(details.locator('dl')).toHaveCount(0)
})

test('offers the acts listed, in the order Connect, Turn on, Turn off, Log out, and none until listed', async ({ page }) => {
	await openTailscale(page, { state: 'running', acts: ['ping', 'log-out', 'netcheck', 'turn-off'] })
	await expect(section(page, 'Acts').locator('.name')).toHaveText(['Turn off', 'Log out'])
	await sayOnTailscale(page, { type: 'tailscale-acts', acts: ['log-out', 'turn-on', 'sign-in'] })
	await expect(section(page, 'Acts').locator('.name')).toHaveText(['Connect', 'Turn on', 'Log out'])
	await sayOnTailscale(page, { type: 'tailscale-acts', acts: ['netcheck'] })
	await expect(section(page, 'Acts')).toHaveCount(0)
})

test('has no acts section before the device lists any', async ({ page }) => {
	await openTailscale(page)
	await expect(section(page, 'Acts')).toHaveCount(0)
})

test('confirms Turn off and Log out, each saying the fleet loses the Iti until it is back', async ({ page }) => {
	await openTailscale(page, { acts: ['turn-off', 'log-out'] })
	const acts = section(page, 'Acts')
	await acts.getByRole('button', { name: 'Turn off' }).click()
	const off = acts.getByRole('alertdialog', { name: 'Turn off Tailscale?' })
	await expect(off).toContainText("The fleet can't reach the Iti through Tailscale until it is turned on again.")
	await expect(acts.getByRole('button', { name: 'Log out' })).toBeVisible()
	await off.getByRole('button', { name: 'Cancel' }).click()
	await acts.getByRole('button', { name: 'Log out' }).click()
	const out = acts.getByRole('alertdialog', { name: 'Log out?' })
	await expect(out).toContainText("The fleet can't reach the Iti through Tailscale until it is signed in again.")
	await out.getByRole('button', { name: 'Log out' }).click()
	expect((await tailscaleSent(page)).filter((each) => each.type === 'tailscale-act')).toEqual([{ type: 'tailscale-act', act: 'log-out' }])
	await expect(acts.getByRole('button', { name: 'Turn off' })).toBeDisabled()
})

test('shows a refusal as the device wrote it', async ({ page }) => {
	await openTailscale(page, { state: 'stopped', acts: ['turn-on'] })
	await section(page, 'Acts').getByRole('button', { name: 'Turn on' }).click()
	await sayOnTailscale(page, { type: 'cannot-act', act: 'turn-on', at: "$['act']", reason: 'another act is under way' })
	await expect(section(page, 'Acts').locator('.why')).toHaveText('another act is under way')
})

test('Connect opens Connect to Tailscale, whose Back returns to the Tailscale screen', async ({ page }) => {
	await openTailscale(page, { state: 'needs-login', acts: ['sign-in', 'netcheck'] })
	await section(page, 'Acts').getByRole('button', { name: 'Connect' }).click()
	await expect(title(page)).toHaveText('Connect to Tailscale')
	await page.getByRole('button', { name: 'Back' }).click()
	await expect(title(page)).toHaveText('Tailscale')
	await page.getByRole('button', { name: 'Back' }).click()
	await expect(title(page)).toHaveText('clinic-iti-07')
})

test('runs a network check and shows it in full, relays nearest first', async ({ page }) => {
	await openTailscale(page, { acts: ['netcheck'] })
	const check = section(page, 'Network check')
	await check.getByRole('button', { name: 'Run' }).click()
	await expect(check.getByRole('status')).toHaveText('Checking…')
	await sayOnTailscale(page, { type: 'tailscale-result', result: RESULT })
	await expect(check.locator('.verdict')).toHaveText('Can connect')
	await expect(check.locator('dt')).toHaveText(['UDP', 'IPv4', 'IPv6', 'Address varies', 'Asks for sign-in'])
	await expect(check.locator('dd')).toHaveText(['Yes', 'Yes', 'No', 'Yes', 'No'])
	await expect(check.locator('.relays li')).toHaveText(['Sydney32 ms', 'Relay 1241 ms'])
	await expect(check.getByRole('button', { name: 'Run again' })).toBeEnabled()
})

test('shows a check run from the Status row without running it again', async ({ page }) => {
	await page.addInitScript(recordHandingOn, false)
	await openChannel(page, fact('tailscale-state', 'running'))
	await standingActs(page, { tailscale: ['netcheck'] })
	await sayOnTailscale(page, { type: 'tailscale-acts', acts: ['netcheck'] })
	await page.getByRole('button', { name: 'Network check' }).click()
	await sayOnTailscale(page, { type: 'tailscale-result', result: RESULT })
	await page.getByRole('button', { name: 'Details' }).click()
	await expect(section(page, 'Network check').locator('.verdict')).toHaveText('Can connect')
	expect((await tailscaleSent(page)).filter((each) => each.act === 'netcheck')).toHaveLength(1)
})

test('shares the hostname, when it ran, the result without report, then report as received', async ({ page }) => {
	await openTailscale(page, { acts: ['netcheck'], handOn: 'sheet' })
	await section(page, 'Network check').getByRole('button', { name: 'Run' }).click()
	await sayOnTailscale(page, { type: 'tailscale-result', result: RESULT })
	await section(page, 'Network check').getByRole('button', { name: 'Share' }).click()
	const [shared] = await page.evaluate(() => window.__shared)
	const { report, ...rest } = RESULT
	const [head, json, ...tail] = shared.text.split('\n\n')
	expect(head).toMatch(/^Network check of clinic-iti-07 at \d{4}-\d\d-\d\dT/)
	expect(JSON.parse(json)).toEqual(rest)
	expect(tail.join('\n\n')).toBe(report)
})

test('copies the same text where the browser has no share sheet', async ({ page }) => {
	await openTailscale(page, { acts: ['netcheck'] })
	await section(page, 'Network check').getByRole('button', { name: 'Run' }).click()
	await sayOnTailscale(page, { type: 'tailscale-result', result: RESULT })
	await section(page, 'Network check').getByRole('button', { name: 'Share' }).click()
	await expect(section(page, 'Network check').getByRole('button', { name: 'Copied' })).toBeVisible()
	const [copied] = await page.evaluate(() => window.__copied)
	expect(copied.endsWith(RESULT.report)).toBe(true)
	expect(copied).toContain('Network check of clinic-iti-07')
})

test('shows a failed check as the device wrote it', async ({ page }) => {
	await openTailscale(page, { acts: ['netcheck'] })
	await section(page, 'Network check').getByRole('button', { name: 'Run' }).click()
	await sayOnTailscale(page, { type: 'tailscale-failed', reason: 'the network check took too long' })
	await expect(section(page, 'Network check').locator('.why')).toHaveText('the network check took too long')
})

test('pings only while ping is listed, sending a machine only where one is typed', async ({ page }) => {
	await openTailscale(page, { acts: ['netcheck'] })
	await expect(section(page, 'Ping')).toHaveCount(0)
	await sayOnTailscale(page, { type: 'tailscale-acts', acts: ['netcheck', 'ping'] })
	const ping = section(page, 'Ping')
	await ping.getByRole('button', { name: 'Ping' }).click()
	await sayOnTailscale(page, {
		type: 'tailscale-result',
		result: { peer: 'tamanu-central', reachable: true, path: 'relayed', relay: { id: 5, name: 'Sydney' }, latency: 0.058 },
	})
	await expect(ping.locator('.verdict')).toHaveText('tamanu-central answered')
	await expect(ping.locator('dd')).toHaveText(['Through a relay, Sydney', '58 ms'])
	await ping.getByLabel('Machine').fill('canopy')
	await ping.getByRole('button', { name: 'Ping' }).click()
	const pings = (await tailscaleSent(page)).filter((each) => each.act === 'ping')
	expect(pings).toEqual([
		{ type: 'tailscale-act', act: 'ping' },
		{ type: 'tailscale-act', act: 'ping', peer: 'canopy' },
	])
})
