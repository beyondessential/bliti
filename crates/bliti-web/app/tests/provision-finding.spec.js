// Finding a device to provision (PWEB, "The page" and "Finding a device").

import { expect, test } from '@playwright/test'

import { devMode, openChannel, openProvision, picks, stateOf } from './fake-provision-client.js'

const find = (page) => page.getByRole('button', { name: 'Find Iti' })

test('is titled Provision an Iti, and asks for USER to be pressed, with its picture', async ({ page }) => {
	await openProvision(page)
	await expect(page.getByRole('heading', { name: 'Provision an Iti' })).toBeVisible()
	await expect(page.getByText('Press USER on the Iti')).toBeVisible()
	const picture = page.locator('img.board')
	await expect(picture).toHaveAttribute('alt', /USER button/)
	await expect.poll(() => picture.evaluate((img) => img.complete && img.naturalWidth > 0)).toBe(true)
	await expect(page.locator('.pic-caption')).toContainText('USER')
	// The primary action, the one button there is, opens the chooser.
	await expect(find(page)).toBeEnabled()
	await expect(find(page)).toHaveClass(/block/)
	await expect(find(page)).not.toHaveClass(/secondary/)
})

test('is not linked from the page that reaches a device', async ({ page }) => {
	await page.goto('/')
	await expect(page.locator('a[href*="provision"]')).toHaveCount(0)
	await expect(page.getByText('Provision an Iti', { exact: false })).toHaveCount(0)
})

test('says what an empty chooser may mean where nothing was picked, and opens it again', async ({ page }) => {
	await openProvision(page)
	await picks(page, 'nothing')
	await find(page).click()
	await expect(page.getByText("Didn't find an Iti")).toBeVisible()
	await expect(page.getByText('Only an Iti whose USER button was just pressed is listed.')).toBeVisible()
	await expect(page.locator('img.board')).toHaveAttribute('alt', /USER button/)

	await page.getByRole('button', { name: 'Try again' }).click()
	await expect(page.getByRole('button', { name: 'Disconnect' })).toBeVisible()
})

test('says it is connecting once a device is picked, and cancelling goes back to finding one', async ({ page }) => {
	await openProvision(page)
	await picks(page, 'hold')
	await find(page).click()
	await expect(page.getByText('Connecting to the Iti')).toBeVisible()
	await expect(page.getByRole('button', { name: 'Disconnect' })).toHaveCount(0)

	await page.getByRole('button', { name: 'Cancel' }).click()
	await expect(page.getByText('Press USER on the Iti')).toBeVisible()
	await expect(find(page)).toBeEnabled()
	// The attempt is abandoned: releasing it afterwards connects nothing.
	await page.evaluate(() => window.__blitiProvisionRelease?.())
	await expect(page.getByRole('button', { name: 'Disconnect' })).toHaveCount(0)
	await expect(page.getByText('Press USER on the Iti')).toBeVisible()
})

test('says something else is connected where the device has no slot free', async ({ page }) => {
	await openProvision(page)
	await picks(page, 'full')
	await find(page).click()
	await expect(page.getByText('Something else is connected to that Iti')).toBeVisible()
	await expect(page.getByRole('button', { name: 'Try again' })).toBeEnabled()
	await expect(page.getByRole('button', { name: 'Disconnect' })).toHaveCount(0)
})

test('reports any other failure to connect as itself', async ({ page }) => {
	await openProvision(page)
	await picks(page, 'That Iti speaks version 9, which this page does not read.')
	await find(page).click()
	await expect(page.getByText("Couldn't connect to that Iti")).toBeVisible()
	await expect(page.getByText('That Iti speaks version 9, which this page does not read.')).toBeVisible()
})

test('names the device by the suffix of its name for as long as it is connected, and disconnecting goes back to finding one', async ({
	page,
}) => {
	await openChannel(page, stateOf({ run: 'running' }))
	await expect(page.getByText('Code ending K7QX')).toBeVisible()
	await expect(page.getByRole('button', { name: 'Disconnect' })).toBeVisible()
	await page.getByRole('button', { name: 'Disconnect' }).click()
	await expect(page.getByText('Press USER on the Iti')).toBeVisible()
	await expect(page.getByText('Code ending K7QX')).toHaveCount(0)
	expect(await page.evaluate(() => window.__blitiProvisionDisconnects)).toBeGreaterThan(0)
	expect(await page.evaluate(() => window.__blitiProvisionStreams.at(-1).closedByPage)).toBe(true)
})

test('says so, and offers nothing else, where the browser cannot open a channel', async ({ page }) => {
	await page.addInitScript(() => (window.__blitiProvisionUnsupported = 'Open this page in Chrome on an Android phone.'))
	await openProvision(page)
	await expect(page.getByText("This browser can't connect to an Iti")).toBeVisible()
	await expect(page.getByText('Open this page in Chrome on an Android phone.')).toBeVisible()
	await expect(page.getByRole('button')).toHaveCount(0)
	await expect(page.locator('img')).toHaveCount(0)
})

test('keeps the activity record, shown in dev mode', async ({ page }) => {
	await openProvision(page)
	await expect(page.getByRole('heading', { name: 'Activity' })).toHaveCount(0)
	await devMode(page)
	await expect(page.getByRole('heading', { name: 'Activity' })).toBeVisible()
	await find(page).click()
	await expect(page.getByRole('button', { name: 'Disconnect' })).toBeVisible()
	await page.getByRole('button', { name: 'Disconnect' }).click()
	await expect(page.locator('.log .said', { hasText: 'disconnected' })).toBeVisible()
})

// The real client, with its wasm module and a stand-in for the browser's Bluetooth: the chooser is
// filtered on the prefix of the name up to the suffix, with the service of ADV asked for alongside.
test.describe('the real client', () => {
	const bluetooth = (page, stub) =>
		page.addInitScript(`
			window.__requested = []
			Object.defineProperty(navigator, 'bluetooth', { value: { requestDevice: ${stub} } })
		`)

	test('filters the chooser on the prefix of the name of a device in provisioning mode', async ({ page }) => {
		await bluetooth(
			page,
			`async (options) => {
				window.__requested.push(options)
				const error = new Error('User cancelled the requestDevice() chooser.')
				error.name = 'NotFoundError'
				throw error
			}`,
		)
		await page.goto('/provision')
		await find(page).click()
		await expect(page.getByText("Didn't find an Iti")).toBeVisible()
		const [options] = await page.evaluate(() => window.__requested)
		expect(options.filters.length).toBeGreaterThan(0)
		expect(options.filters.map((filter) => filter.namePrefix)).toContain('ITI_PROVISION_1_')
		for (const filter of options.filters) {
			expect(filter.namePrefix).toMatch(/^ITI_PROVISION_\d+_$/)
			expect(filter.name).toBeUndefined()
			expect(filter.services).toBeUndefined()
		}
		expect(options.optionalServices).toEqual(['63c7f3bc-0599-4a66-bdcd-f28ec571c118'])
	})

	// A device with no slot free answers a read of the allocation characteristic with nothing (CHN).
	test('says something else is connected where the allocation it reads is empty', async ({ page }) => {
		await bluetooth(
			page,
			`async () => {
				const characteristic = { readValue: async () => new DataView(new ArrayBuffer(0)) }
				const service = { getCharacteristic: async () => characteristic }
				const server = { getPrimaryService: async () => service }
				return {
					name: 'ITI_PROVISION_1_K7QX',
					gatt: { connected: false, connect: async () => server, disconnect() {} },
					addEventListener() {},
					removeEventListener() {},
				}
			}`,
		)
		await page.goto('/provision')
		await find(page).click()
		await expect(page.getByText('Something else is connected to that Iti')).toBeVisible()
	})

	test('refuses a name that is not one of a device in provisioning mode, and one of a marker it cannot read', async ({
		page,
	}) => {
		await bluetooth(
			page,
			`async () => ({
				name: window.__name,
				gatt: { connected: false, connect: async () => { throw new Error('connected anyway') }, disconnect() {} },
				addEventListener() {},
				removeEventListener() {},
			})`,
		)
		await page.goto('/provision')
		await page.evaluate(() => (window.__name = 'Some Speaker'))
		await find(page).click()
		await expect(page.getByText("That isn't an Iti in provisioning mode.")).toBeVisible()

		await page.evaluate(() => (window.__name = 'ITI_PROVISION_200_K7QX'))
		await page.getByRole('button', { name: 'Try again' }).click()
		await expect(page.getByText('That Iti speaks version 200, which this page does not read.')).toBeVisible()
	})
})
