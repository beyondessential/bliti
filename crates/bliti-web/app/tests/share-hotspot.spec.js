// The Share hotspot screen of SHARE: the switch, the name and password other devices join with, and
// a hotspot not yet set up.

import { expect, test } from '@playwright/test'

import { PI } from './network-fixtures.js'
import { answer, message, say, stateOf } from './fake-client.js'
import {
	HOTSPOT,
	ON_CABLE,
	ON_WIFI,
	SHARING,
	address,
	carrier,
	clients,
	hostname,
	joined,
	openSetup,
	openWay,
	proposals,
	title,
} from './setup-fixtures.js'

const toggle = (page) => page.getByRole('switch', { name: 'Let other devices join' })
const secret = (page) => page.locator('dd.secret span')

async function openShare(page, { document = SHARING, facts = [carrier(), address('eth0')] } = {}) {
	await openSetup(page, { document, capabilities: PI, facts })
	await openWay(page, 'Iti hotspot')
	await expect(title(page)).toHaveText('Share hotspot')
}

const applies = () => [{ echo: true }, stateOf({ outcome: 'verifying' })]

test.describe('the switch', () => {
	test('is on while the hotspot is, with the devices joined beneath it', async ({ page }) => {
		await openShare(page, { facts: [carrier(), address('eth0'), clients(3)] })
		await expect(toggle(page)).toHaveAttribute('aria-checked', 'true')
		await expect(page.getByText('3 devices connected')).toBeVisible()
	})

	test('turns the hotspot off at once, leaving the cable as it is', async ({ page }) => {
		await openShare(page)
		await answer(page, 'configuration', applies())
		await toggle(page).click()
		await expect(page.getByText('Turning off…')).toBeVisible()
		await expect(toggle(page)).toBeDisabled()
		const [proposed] = await proposals(page)
		expect(proposed.hotspot).toEqual({ ...HOTSPOT, enabled: false })
		expect(proposed.attachments).toEqual(SHARING.attachments)
		await say(page, stateOf({ outcome: 'applied' }))
		await expect(toggle(page)).toHaveAttribute('aria-checked', 'false')
		await expect(toggle(page)).toBeEnabled()
	})

	test('says turning it on leaves wifi for the cable, and does so', async ({ page }) => {
		await openShare(page, { document: ON_WIFI, facts: [carrier(), joined('Clinic-Staff'), address('wlan0')] })
		await expect(page.getByText('The Iti will leave Clinic-Staff and use its cable.')).toBeVisible()
		await answer(page, 'configuration', applies())
		await toggle(page).click()
		await expect(page.getByText('Turning on…')).toBeVisible()
		const [proposed] = await proposals(page)
		expect(proposed.attachments.map((each) => each.enabled)).toEqual([true, false])
		expect(proposed.hotspot.enabled).toBe(true)
	})

	test('says turning it on from wifi with no cable goes offline', async ({ page }) => {
		await openShare(page, { document: ON_WIFI, facts: [carrier(false), joined('Clinic-Staff'), address('wlan0')] })
		await expect(page.getByText('The Iti will leave Clinic-Staff and go offline.')).toBeVisible()
	})

	test('shows the switch as the device holds it, with the reason, where a change fails', async ({ page }) => {
		await openShare(page, { document: ON_CABLE })
		await answer(page, 'configuration', [
			message({ type: 'cannot-configure', at: "$['hotspot']", reason: 'wlan0 cannot start an access point' }),
			message({ type: 'configuration', document: ON_CABLE }),
			stateOf(),
		])
		await toggle(page).click()
		await expect(page.getByText('wlan0 cannot start an access point')).toBeVisible()
		await expect(toggle(page)).toHaveAttribute('aria-checked', 'false')
	})
})

test.describe('the details', () => {
	test('show the name, hide the password until asked, and draw the QR code', async ({ page }) => {
		await openShare(page)
		await expect(page.locator('.details dd strong')).toHaveText('clinic-iti-07')
		await expect(secret(page)).toHaveText('••••••••')
		await page.getByRole('button', { name: 'Show' }).click()
		await expect(secret(page)).toHaveText(HOTSPOT.passphrase)
		await page.getByRole('button', { name: 'Hide' }).click()
		await expect(secret(page)).toHaveText('••••••••')
		await expect(page.getByRole('img', { name: 'QR code to join clinic-iti-07' })).toBeVisible()
	})

	test('change the password in place, saying joined devices must rejoin', async ({ page }) => {
		await openShare(page)
		await answer(page, 'configuration', [...applies(), stateOf({ outcome: 'applied' })])
		await page.getByRole('button', { name: 'Change password' }).click()
		await expect(page.getByText('Devices already connected will need to reconnect with the new password.')).toBeVisible()
		const field = page.getByLabel('Password')
		await field.fill('short')
		await page.getByRole('button', { name: 'Save' }).click()
		await expect(page.getByText('Use 8 to 63 characters.')).toBeVisible()
		expect(await proposals(page)).toEqual([])
		await field.fill('harbour-light-22')
		await page.getByRole('button', { name: 'Save' }).click()
		await expect(page.getByLabel('Password')).toHaveCount(0)
		const [proposed] = await proposals(page)
		expect(proposed.hotspot).toEqual({ ...HOTSPOT, passphrase: 'harbour-light-22' })
		await page.getByRole('button', { name: 'Show' }).click()
		await expect(secret(page)).toHaveText('harbour-light-22')
	})

	test('cancel a change to the password, keeping the one held', async ({ page }) => {
		await openShare(page)
		await page.getByRole('button', { name: 'Change password' }).click()
		await page.getByLabel('Password').fill('something-else')
		await page.getByRole('button', { name: 'Cancel' }).click()
		await page.getByRole('button', { name: 'Show' }).click()
		await expect(secret(page)).toHaveText(HOTSPOT.passphrase)
		expect(await proposals(page)).toEqual([])
	})

	test('ask before leaving with a password not saved', async ({ page }) => {
		await openShare(page)
		await page.getByRole('button', { name: 'Change password' }).click()
		await page.getByLabel('Password').fill('something-else')
		await page.goBack()
		await expect(page.getByRole('alertdialog', { name: 'Leave without saving the password?' })).toBeVisible()
		await page.getByRole('alertdialog').getByRole('button', { name: 'Leave' }).click()
		await expect(title(page)).toHaveText('Network')
	})
})

test.describe('a hotspot not yet set up', () => {
	test('is named after the hostname with a passphrase drawn for the session, proposed on turning it on', async ({ page }) => {
		await openShare(page, { document: ON_CABLE, facts: [hostname('clinic-iti-09'), carrier(), address('eth0')] })
		await expect(page.locator('.details dd strong')).toHaveText('clinic-iti-09')
		await page.getByRole('button', { name: 'Show' }).click()
		const drawn = await secret(page).textContent()
		expect(drawn).toMatch(/^[a-z]+-[a-z]+-[a-z]+-[a-z]+-\d{2}$/)

		// The same passphrase for as long as the session is open.
		await page.getByRole('button', { name: 'Back', exact: true }).click()
		await openWay(page, 'Iti hotspot')
		await page.getByRole('button', { name: 'Show' }).click()
		await expect(secret(page)).toHaveText(drawn)

		await answer(page, 'configuration', applies())
		await toggle(page).click()
		const [proposed] = await proposals(page)
		expect(proposed.hotspot).toEqual({
			enabled: true,
			ssid: 'clinic-iti-09',
			passphrase: drawn,
			'share-upstream': true,
			'isolate-clients': true,
		})
	})

	test('is named as the Status screen titles a device reporting no hostname', async ({ page }) => {
		await openShare(page, { document: ON_CABLE })
		await expect(page.locator('.details dd strong')).toHaveText('9WQX')
	})

	test('is set up off where the operator saves a passphrase first', async ({ page }) => {
		await openShare(page, { document: ON_CABLE })
		await page.getByRole('button', { name: 'Change password' }).click()
		await page.getByLabel('Password').fill('harbour-light-22')
		await page.getByRole('button', { name: 'Save' }).click()
		const [proposed] = await proposals(page)
		expect(proposed.hotspot).toMatchObject({ enabled: false, ssid: '9WQX', passphrase: 'harbour-light-22' })
	})
})
