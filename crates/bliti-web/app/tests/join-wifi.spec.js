// The Join wifi screen of WIFI: choosing a network from a scan or by name, its password, and
// connecting, with what each failure says.

import { expect, test } from '@playwright/test'

import { PI, ap } from './network-fixtures.js'
import { UNNAMED, answer, message, say, sent, stateOf } from './fake-client.js'
import { ON_CABLE, ON_WIFI, SHARING, address, carrier, joined, openSetup, openWay, proposals, title } from './setup-fixtures.js'

const HEARD = [
	ap({ ssid: 'Clinic-Staff', signal: -48, security: ['sae'] }),
	ap({ ssid: 'Clinic-Guest', signal: -62, security: ['psk'] }),
	ap({ ssid: 'HealthPost-4G', signal: -82, security: ['psk', 'sae'] }),
	ap({ ssid: 'Clinic-Corp', signal: -70, security: ['enterprise'] }),
	ap({ ssid: 'Cafe', signal: -75, security: ['open'] }),
	ap({ ssid: null, hidden: true, signal: -40 }),
]
const networks = (points = HEARD) => message({ type: 'networks', 'access-points': points })
const choice = (page, name) => page.locator('.choices button').filter({ has: page.locator('.name', { hasText: name }) })
const step = (page) => page.locator('.steps li.current .name')
const password = (page) => page.getByLabel('Password', { exact: true })

/// The Join wifi screen of a device holding `document`, its scan answered with `points`.
async function openJoin(page, { document = ON_CABLE, points = HEARD, facts = [carrier(), address('eth0')] } = {}) {
	await openSetup(page, { document, capabilities: PI, facts })
	if (points) await answer(page, 'scan', networks(points))
	await openWay(page, 'Wifi')
	await expect(title(page)).toHaveText('Join wifi')
}

test.describe('choosing a network', () => {
	test('scans on opening, and lists each network once by name with its signal, strongest first', async ({ page }) => {
		await openJoin(page)
		await expect(step(page)).toHaveText('Network')
		await expect(page.locator('.choices .name')).toHaveText(['Clinic-Staff', 'Clinic-Guest', 'Clinic-Corp', 'Cafe', 'HealthPost-4G'])
		await expect(choice(page, 'Clinic-Staff').locator('.signal')).toHaveText('Strong')
		await expect(choice(page, 'HealthPost-4G').locator('.signal')).toHaveText('Weak')
		expect((await sent(page)).filter((each) => each.type === 'scan')).toEqual([{ type: 'scan' }])
	})

	test('says it is looking until the device answers, and where it heard nothing', async ({ page }) => {
		await openJoin(page, { points: null })
		await expect(page.getByText('Looking for networks…')).toBeVisible()
		await say(page, networks([]))
		await expect(page.getByText('No wifi networks found nearby.')).toBeVisible()
		await answer(page, 'scan', networks())
		await page.getByRole('button', { name: 'Refresh' }).click()
		await expect(page.locator('.choices .name').first()).toHaveText('Clinic-Staff')
	})

	test('says joining turns the hotspot off', async ({ page }) => {
		await openJoin(page, { document: SHARING })
		await expect(page.getByText('Joining turns the hotspot off.')).toBeVisible()
	})

	test('marks the network the device is joined to', async ({ page }) => {
		await openJoin(page, { document: ON_WIFI, facts: [joined('Clinic-Staff'), address('wlan0')] })
		await expect(page.locator('.choices button.joined .name')).toHaveText(['Clinic-Staff'])
		await expect(choice(page, 'Clinic-Staff')).toContainText('Connected')
		await expect(page.getByText('Joining turns the hotspot off.')).toHaveCount(0)
	})

	test('shows a network the device cannot join, and does not let it be picked', async ({ page }) => {
		await openJoin(page)
		await expect(choice(page, 'Cafe')).toBeDisabled()
		await expect(choice(page, 'Cafe')).toContainText("The Iti can't join this network")
	})

	test('sends an enterprise network to the network settings, or back to the list', async ({ page }) => {
		await openJoin(page)
		await choice(page, 'Clinic-Corp').click()
		await expect(page.getByText('Clinic-Corp needs more settings')).toBeVisible()
		await page.getByRole('button', { name: 'Choose another network' }).click()
		await expect(step(page)).toHaveText('Network')
		await choice(page, 'Clinic-Corp').click()
		await page.getByRole('button', { name: 'Open network settings' }).click()
		await expect(title(page)).toHaveText('Network settings')
		expect((await page.evaluate(() => window.__blitiSessions.map((each) => each.closedByPage)))[0]).toBe(true)
	})
})

test.describe('the password and connecting', () => {
	test('asks for the password, shows it on request, and checks its length', async ({ page }) => {
		await openJoin(page)
		await choice(page, 'Clinic-Guest').click()
		await expect(step(page)).toHaveText('Password')
		await expect(page.getByText('Enter the password for Clinic-Guest')).toBeVisible()
		await expect(password(page)).toHaveAttribute('type', 'password')
		await page.getByRole('button', { name: 'Show' }).click()
		await expect(password(page)).toHaveAttribute('type', 'text')
		await password(page).fill('short')
		await page.getByRole('button', { name: 'Connect' }).click()
		await expect(page.getByText('Use 8 to 63 characters.')).toBeVisible()
		expect(await proposals(page)).toEqual([])
	})

	test('fills in the password wifi holds for the network it names', async ({ page }) => {
		await openJoin(page)
		await choice(page, 'Clinic-Staff').click()
		await expect(password(page)).toHaveValue('correct horse battery')
		await page.getByRole('button', { name: 'Choose another network' }).click()
		await choice(page, 'Clinic-Guest').click()
		await expect(password(page)).toHaveValue('')
	})

	test('replaces wifi in place, turning the hotspot off and leaving the cable on, and returns to the Status screen', async ({ page }) => {
		await openJoin(page, { document: SHARING })
		await answer(page, 'configuration', [{ echo: true }, stateOf({ outcome: 'verifying' })])
		await choice(page, 'Clinic-Guest').click()
		await password(page).fill('guest-pass-1')
		await page.getByRole('button', { name: 'Connect' }).click()
		await expect(step(page)).toHaveText('Connect')
		await expect(page.getByText('Connecting to Clinic-Guest')).toBeVisible()
		await expect(page.getByRole('button', { name: 'Cancel' })).toHaveCount(0)
		const [proposed] = await proposals(page)
		expect(proposed.attachments).toEqual([
			SHARING.attachments[0],
			{
				kind: 'wireless',
				label: 'Clinic-Guest',
				enabled: true,
				verify: true,
				ssid: 'Clinic-Guest',
				security: { kind: 'psk', passphrase: 'guest-pass-1' },
				hidden: false,
			},
		])
		expect(proposed.hotspot.enabled).toBe(false)
		await say(page, stateOf({ outcome: 'applied' }))
		await expect(title(page)).toHaveText(UNNAMED)
	})

	test('joins a network the operator names, hidden and by the strongest kind offered', async ({ page }) => {
		await openJoin(page)
		await answer(page, 'configuration', [{ echo: true }, stateOf({ outcome: 'verifying' })])
		await page.getByRole('button', { name: "My network isn't listed" }).click()
		await page.getByLabel('Network name').fill('Clinic-Private')
		await page.getByRole('button', { name: 'Continue' }).click()
		await password(page).fill('private-pass')
		await page.getByRole('button', { name: 'Connect' }).click()
		const [proposed] = await proposals(page)
		expect(proposed.attachments[1]).toMatchObject({ ssid: 'Clinic-Private', hidden: true, security: { kind: 'psk-sae' } })
	})

	test('goes back to the password, marked, where it did not work', async ({ page }) => {
		await openJoin(page)
		await answer(page, 'configuration', [
			{ echo: true },
			stateOf({ outcome: 'verifying' }),
			stateOf({ outcome: 'failed', failure: { at: "$['attachments'][1]['security']['passphrase']", reason: 'auth rejected', reached: 'association' } }),
		])
		await choice(page, 'Clinic-Guest').click()
		await password(page).fill('guest-pass-2')
		await page.getByRole('button', { name: 'Connect' }).click()
		await expect(step(page)).toHaveText('Password')
		await expect(page.getByText("That password didn't work. Check it and try again.")).toBeVisible()
		await expect(password(page)).toHaveClass(/field-fault/)
		await expect(password(page)).toHaveValue('guest-pass-2')
	})

	test('waits for a network out of range, and returns once the device joins it', async ({ page }) => {
		await openJoin(page)
		await answer(page, 'configuration', [
			{ echo: true },
			stateOf({ outcome: 'verifying' }),
			stateOf({
				outcome: 'failed',
				failure: { at: "$['attachments'][1]", reason: 'not heard', reached: 'carrier' },
				attachments: [{ is: 'off' }, { is: 'unavailable', reached: 'carrier', reason: 'not heard' }],
			}),
		])
		await choice(page, 'Clinic-Guest').click()
		await password(page).fill('guest-pass-1')
		await page.getByRole('button', { name: 'Connect' }).click()
		await expect(page.getByText("Couldn't reach Clinic-Guest")).toBeVisible()
		await expect(page.getByText('Waiting for Clinic-Guest…')).toBeVisible()
		await expect(page.getByRole('button', { name: 'Try again' })).toHaveCount(0)
		await say(page, stateOf({ outcome: 'failed', failure: { at: "$['attachments'][1]", reason: 'not heard', reached: 'carrier' }, attachments: [{ is: 'off' }, { is: 'default-route' }] }))
		await expect(title(page)).toHaveText(UNNAMED)
	})

	test('says why joining failed otherwise, offering another network and another password', async ({ page }) => {
		await openJoin(page)
		await answer(page, 'configuration', [
			{ echo: true },
			stateOf({ outcome: 'verifying' }),
			stateOf({ outcome: 'failed', failure: { at: "$['attachments'][1]", reason: 'no DHCP offer on wlan0', reached: 'addressing' } }),
		])
		await choice(page, 'Clinic-Guest').click()
		await password(page).fill('guest-pass-1')
		await page.getByRole('button', { name: 'Connect' }).click()
		await expect(page.getByText("Couldn't join Clinic-Guest")).toBeVisible()
		await expect(page.getByText('no DHCP offer on wlan0')).toBeVisible()
		// The device kept the configuration that failed, so wifi names this network now.
		await page.getByRole('button', { name: 'Change password' }).click()
		await expect(password(page)).toHaveValue('guest-pass-1')
		await page.getByRole('button', { name: 'Choose another network' }).click()
		await expect(step(page)).toHaveText('Network')
	})

	test('shows a refusal as the device wrote it', async ({ page }) => {
		await openJoin(page)
		await answer(page, 'configuration', [
			message({ type: 'cannot-configure', at: '$', reason: 'radio busy' }),
			message({ type: 'configuration', document: ON_CABLE }),
			stateOf(),
		])
		await choice(page, 'Clinic-Guest').click()
		await password(page).fill('guest-pass-1')
		await page.getByRole('button', { name: 'Connect' }).click()
		await expect(page.getByText('radio busy')).toBeVisible()
	})
})

test.describe('leaving with a password not proposed', () => {
	test('asks first, from the screen’s Back and the browser’s', async ({ page }) => {
		await openJoin(page)
		await choice(page, 'Clinic-Guest').click()
		await password(page).fill('half-typ')
		await page.getByRole('button', { name: 'Back', exact: true }).click()
		await expect(page.getByRole('alertdialog', { name: 'Leave without connecting?' })).toBeVisible()
		await page.getByRole('button', { name: 'Stay' }).click()
		await page.goBack()
		await expect(page.getByRole('alertdialog')).toBeVisible()
		await expect(title(page)).toHaveText('Join wifi')
		await page.getByRole('alertdialog').getByRole('button', { name: 'Leave' }).click()
		await expect(title(page)).toHaveText('Network')
	})

	test('does not ask for a password filled in and not typed', async ({ page }) => {
		await openJoin(page)
		await choice(page, 'Clinic-Staff').click()
		await page.getByRole('button', { name: 'Back', exact: true }).click()
		await expect(title(page)).toHaveText('Network')
	})
})
