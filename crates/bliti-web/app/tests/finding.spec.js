// Finding the device a QR code belongs to (WEB, "Finding the device").

import { expect, test } from '@playwright/test'

import { installFakeClient, typeCode as read } from './fake-client.js'

test('the name the device advertises is shown before the chooser opens', async ({ page }) => {
	await page.addInitScript(installFakeClient)
	await page.goto('/')
	await read(page, 'BLITI:AHFYTP4T6K2M9WQX')
	await expect(page.getByText('It should be the only device that shows up, named:')).toBeVisible()
	await expect(page.locator('.chip')).toHaveText('AHOW2EZUD4343RQ')
})

test('a chooser closed with nothing picked says what an empty list may mean, and can be opened again', async ({
	page,
}) => {
	await page.addInitScript(installFakeClient)
	await page.addInitScript(() => (window.__blitiNothingPicked = true))
	await page.goto('/')
	await read(page, 'BLITI:AHFYTP4T6K2M9WQX')
	await page.getByRole('button', { name: 'Find my Iti' }).click()
	await expect(page.getByText("Your Iti wasn't found")).toBeVisible()
	await expect(page.getByText("Check it's switched on and nearby, then try again.", { exact: false })).toBeVisible()
	await expect(page.locator('.chip')).toHaveText('AHOW2EZUD4343RQ')
	await expect(page.getByRole('button', { name: 'Try again' })).toBeEnabled()
})

test('any other failure to connect is reported as itself', async ({ page }) => {
	await page.addInitScript(installFakeClient)
	await page.addInitScript(() => {
		window.__blitiClient.connect = async () => {
			throw new Error('That is a different Iti. Pick the one whose code you read.')
		}
	})
	await page.goto('/')
	await read(page, 'BLITI:AHFYTP4T6K2M9WQX')
	await page.getByRole('button', { name: 'Find my Iti' }).click()
	await expect(page.getByText('That is a different Iti. Pick the one whose code you read.', { exact: true })).toBeVisible()
	await expect(page.getByText("Your Iti wasn't found")).toHaveCount(0)
})

// The real client, with its wasm module: the name is computed from the payload, and the chooser is
// filtered on exactly that name alone, with the bliti service asked for alongside.
test('the real client filters the chooser on the name it shows', async ({ page }) => {
	await page.addInitScript(() => {
		window.__requested = []
		Object.defineProperty(navigator, 'bluetooth', {
			value: {
				async requestDevice(options) {
					window.__requested.push(options)
					const error = new Error('User cancelled the requestDevice() chooser.')
					error.name = 'NotFoundError'
					throw error
				},
			},
		})
	})
	await page.goto('/')
	await read(
		page,
		'BLITI:AEAACAQDAQCQMBYIBEFAWDANBYHYBAMCQOCILBUHRCEYVC4MRWHI7EER',
	)
	await expect(page.locator('.chip')).toHaveText('AFVB2XNR5G2JGXI')
	await page.getByRole('button', { name: 'Find my Iti' }).click()
	await expect(page.getByText("Your Iti wasn't found")).toBeVisible()
	const [options] = await page.evaluate(() => window.__requested)
	expect(options.filters).toHaveLength(1)
	expect(options.filters[0].name).toBe('AFVB2XNR5G2JGXI')
	expect(options.filters[0].services).toBeUndefined()
	expect(options.optionalServices).toEqual(['63c7f3bc-0599-4a66-bdcd-f28ec571c118'])
})
