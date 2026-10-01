// Finding the device a QR code belongs to (WEB, "Finding the device").

import { expect, test } from '@playwright/test'

import { installFakeClient } from './fake-client.js'

async function read(page, code) {
	await page.getByPlaceholder('BLITI:...').fill(code)
	await page.getByRole('button', { name: 'Use' }).click()
}

test('the name the device advertises is shown before the chooser opens', async ({ page }) => {
	await page.addInitScript(installFakeClient)
	await page.goto('/')
	await read(page, 'BLITI:AHFYTP4T6K2M9WQX')
	await expect(page.getByText('A device named AHOW2EZUD4343RQ will show up.')).toBeVisible()
	await expect(page.getByText('It should be the only one in the list.')).toBeVisible()
})

test('a chooser closed with nothing picked says what an empty list may mean, and can be opened again', async ({
	page,
}) => {
	await page.addInitScript(installFakeClient)
	await page.addInitScript(() => (window.__blitiNothingPicked = true))
	await page.goto('/')
	await read(page, 'BLITI:AHFYTP4T6K2M9WQX')
	await page.getByRole('button', { name: 'Find the device' }).click()
	await expect(page.getByText('If your device was not in the list, check it is on and nearby.')).toBeVisible()
	await expect(page.getByRole('button', { name: 'Find the device' })).toBeEnabled()
})

test('any other failure to connect is reported as itself', async ({ page }) => {
	await page.addInitScript(installFakeClient)
	await page.addInitScript(() => {
		window.__blitiClient.connect = async () => {
			throw new Error('That is a different bliti device. Pick the one whose code you read.')
		}
	})
	await page.goto('/')
	await read(page, 'BLITI:AHFYTP4T6K2M9WQX')
	await page.getByRole('button', { name: 'Find the device' }).click()
	await expect(
		page.getByText('That is a different bliti device. Pick the one whose code you read.', { exact: true }),
	).toBeVisible()
	await expect(page.getByText('If your device was not in the list')).toHaveCount(0)
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
	await expect(page.getByText('A device named AFVB2XNR5G2JGXI will show up.')).toBeVisible()
	await page.getByRole('button', { name: 'Find the device' }).click()
	await expect(page.getByText('If your device was not in the list, check it is on and nearby.')).toBeVisible()
	const [options] = await page.evaluate(() => window.__requested)
	expect(options.filters).toHaveLength(1)
	expect(options.filters[0].name).toBe('AFVB2XNR5G2JGXI')
	expect(options.filters[0].services).toBeUndefined()
	expect(options.optionalServices).toEqual(['63c7f3bc-0599-4a66-bdcd-f28ec571c118'])
})
