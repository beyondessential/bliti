// The Tailscale details of a run: the prompts of the step `tailscale`, the key kept in local storage
// for a week, and what is said of a sign-in that waits on approval (PWEB, "Tailscale details").

import { expect, test } from '@playwright/test'

import {
	acts,
	activity,
	answerWith,
	devMode,
	emitState,
	message,
	openChannel,
	prompt,
	say,
	stateOf,
	step,
} from './fake-provision-client.js'

const STORAGE = 'bliti.provision.tailscale-key'
const DAY = 86_400_000
const KEY = 'tskey-auth-kS3cret'

const KEY_PROMPT = prompt('auth-key', 'tailscale', {
	kind: 'secret',
	blocking: false,
	text: 'Tailscale auth key for this Iti.',
})
const NAME_PROMPT = prompt('name', 'tailscale', {
	kind: 'text',
	blocking: false,
	text: 'Name for this Iti in the tailnet.',
	default: 'tamanu-iti-k7qx',
})

const steps = (tailscale = {}) => [
	step('eeprom', { title: 'Program the EEPROM', state: 'passed' }),
	step('disk', { title: 'Write the system to the disk', state: 'running', progress: 0.4 }),
	step('tailscale', { title: 'Sign in to Tailscale', state: 'pending', ...tailscale }),
]

const asking = (prompts = [KEY_PROMPT, NAME_PROMPT]) => stateOf({ run: 'running', steps: steps(), prompts })

const row = (page) => page.locator('li[data-step="tailscale"]')
const stored = (page) => page.evaluate((name) => localStorage.getItem(name), STORAGE)

/// Put a key in local storage as saved `ago` milliseconds ago, before the page loads, and once only so
/// a reload does not put back what the page has removed.
async function saved(page, ago, key = KEY) {
	await page.addInitScript(
		([name, key, ago]) => {
			if (sessionStorage.getItem('seeded')) return
			sessionStorage.setItem('seeded', '1')
			localStorage.setItem(name, JSON.stringify({ key, saved: new Date(Date.now() - ago).toISOString() }))
		},
		[STORAGE, key, ago],
	)
}

test('shows the key and the name beneath the Tailscale row while it is pending, the name filled with its default', async ({
	page,
}) => {
	await openChannel(page, asking())
	await expect(row(page).getByText('Tailscale auth key for this Iti.')).toBeVisible()
	await expect(row(page).getByText('Name for this Iti in the tailnet.')).toBeVisible()
	await expect(row(page).getByLabel('Name for this Iti in the tailnet.')).toHaveValue('tamanu-iti-k7qx')
	// The key is hidden until revealed.
	const field = row(page).getByLabel('Tailscale auth key for this Iti.')
	await expect(field).toHaveAttribute('type', 'password')
	await row(page).getByRole('button', { name: 'Show' }).click()
	await expect(field).toHaveAttribute('type', 'text')
})

test('answers the name as it stands, or as changed', async ({ page }) => {
	await openChannel(page, asking())
	const name = row(page).getByLabel('Name for this Iti in the tailnet.')
	await name.fill('iti-front-desk')
	await name.press('Enter')
	await expect.poll(() => acts(page)).toEqual([{ act: 'answer', prompt: 'name', value: 'iti-front-desk' }])
})

test('keeps a typed key in local storage with the time it was saved, and never records it', async ({ page }) => {
	await openChannel(page, asking())
	await devMode(page)
	await row(page).getByLabel('Tailscale auth key for this Iti.').fill(KEY)
	const before = Date.now()
	await row(page).getByRole('button', { name: 'Save' }).first().click()
	await expect.poll(() => acts(page)).toEqual([{ act: 'answer', prompt: 'auth-key', value: KEY }])

	const kept = JSON.parse(await stored(page))
	expect(kept.key).toBe(KEY)
	expect(Date.parse(kept.saved)).toBeGreaterThanOrEqual(before - 1000)
	// The field is cleared as it is sent, and the record says nothing of the value.
	await expect(row(page).getByLabel('Tailscale auth key for this Iti.')).toHaveValue('')
	expect((await activity(page)).join('\n')).not.toContain(KEY)
})

test('answers the prompt from the saved key at once, without the field, and says so', async ({ page }) => {
	await saved(page, 2 * DAY)
	await openChannel(page, asking())
	await expect.poll(() => acts(page)).toEqual([{ act: 'answer', prompt: 'auth-key', value: KEY }])
	await expect(row(page).getByLabel('Tailscale auth key for this Iti.')).toHaveCount(0)
	await expect(row(page).getByText(/Using the key saved/)).toBeVisible()
	// The name is still asked.
	await expect(row(page).getByLabel('Name for this Iti in the tailnet.')).toBeVisible()
})

test('answers the saved key once however often the device repeats the state', async ({ page }) => {
	await saved(page, DAY)
	await openChannel(page, asking())
	await expect.poll(() => acts(page)).toHaveLength(1)
	await emitState(page, asking())
	await emitState(page, asking([KEY_PROMPT]))
	await page.waitForTimeout(100)
	expect(await acts(page)).toHaveLength(1)
})

test('does not put the saved key in the activity record', async ({ page }) => {
	await saved(page, DAY)
	await openChannel(page, asking())
	await devMode(page)
	await expect.poll(() => acts(page)).toHaveLength(1)
	expect((await activity(page)).join('\n')).not.toContain(KEY)
})

test('asks for the key where the saved one is a week old, and removes it', async ({ page }) => {
	await saved(page, 7 * DAY + 1000)
	await openChannel(page, asking())
	await expect(row(page).getByLabel('Tailscale auth key for this Iti.')).toBeVisible()
	expect(await acts(page)).toEqual([])
	expect(await stored(page)).toBeNull()
})

test('uses a saved key until it is a week old', async ({ page }) => {
	await saved(page, 7 * DAY - 60_000)
	await openChannel(page, asking())
	await expect.poll(() => acts(page)).toEqual([{ act: 'answer', prompt: 'auth-key', value: KEY }])
})

test('asks for the key where what is saved cannot be read', async ({ page }) => {
	await page.addInitScript((name) => localStorage.setItem(name, '{"key": 4'), STORAGE)
	await openChannel(page, asking())
	await expect(row(page).getByLabel('Tailscale auth key for this Iti.')).toBeVisible()
	expect(await acts(page)).toEqual([])
})

test('forgets the saved key on request, from local storage, and says the next Iti asks', async ({ page }) => {
	await saved(page, DAY)
	await openChannel(page, asking())
	await expect.poll(() => acts(page)).toHaveLength(1)
	await row(page).getByRole('button', { name: 'Forget' }).click()
	expect(await stored(page)).toBeNull()
	await expect(row(page).getByText('The saved key was forgotten. The next Iti asks for one.')).toBeVisible()
})

test('keeps a typed key for the next Iti, across a reload', async ({ page }) => {
	await openChannel(page, asking())
	await row(page).getByLabel('Tailscale auth key for this Iti.').fill(KEY)
	await row(page).getByRole('button', { name: 'Save' }).first().click()
	await expect.poll(() => acts(page)).toHaveLength(1)

	await page.reload()
	await page.getByRole('button', { name: 'Find Iti' }).click()
	await page.getByRole('button', { name: 'Disconnect' }).waitFor()
	await emitState(page, asking())
	await expect.poll(() => acts(page)).toEqual([{ act: 'answer', prompt: 'auth-key', value: KEY }])
})

test('discards the saved key where the device refuses the answer, and shows the field', async ({ page }) => {
	await saved(page, DAY)
	await openChannel(page, asking([]))
	await answerWith(page, 'answer', message({ type: 'cannot-act', act: 'provision-act', at: "$['value']", reason: 'No.' }))
	await emitState(page, asking())
	await expect(row(page).getByLabel('Tailscale auth key for this Iti.')).toBeVisible()
	expect(await stored(page)).toBeNull()
})

test('discards the saved key where the step fails, so the next attempt asks again', async ({ page }) => {
	await saved(page, DAY)
	await openChannel(page, asking())
	await expect.poll(() => acts(page)).toHaveLength(1)
	await emitState(page, {
		run: 'failed',
		steps: steps({ state: 'failed', reason: 'Tailscale rejected the key' }),
		prompts: [],
	})
	await expect(row(page).getByText('Tailscale rejected the key')).toBeVisible()
	expect(await stored(page)).toBeNull()
})

test('says a sign-in that waits on approval needs a tailnet administrator, while the run goes on', async ({ page }) => {
	await openChannel(
		page,
		stateOf({
			run: 'running',
			steps: [
				step('tailscale', {
					title: 'Sign in to Tailscale',
					state: 'passed',
					result: { 'needs-approval': true },
				}),
				step('bootloader', { title: 'Configure the bootloader', state: 'running' }),
			],
		}),
	)
	await expect(row(page).getByText('A tailnet administrator has to approve this Iti before it joins the tailnet.')).toBeVisible()
})

test('says it again once the run has finished, and not where nothing waits on approval', async ({ page }) => {
	const done = (approval) =>
		stateOf({
			run: 'finished',
			steps: [
				step('tailscale', {
					title: 'Sign in to Tailscale',
					state: 'passed',
					result: { 'needs-approval': approval },
				}),
			],
		})
	await openChannel(page, done(true))
	await expect(page.getByText('A tailnet administrator has to approve this Iti')).toBeVisible()
	await say(page, message(done(false)))
	await expect(page.getByText('A tailnet administrator has to approve this Iti')).toHaveCount(0)
})
