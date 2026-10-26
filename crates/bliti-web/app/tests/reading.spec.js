// Reading a code typed in, through the real client and its wasm module (WEB, "Reading a QR code";
// QR, "Reading"). Bluetooth is only asked whether it exists: nothing here connects.

import { expect, test } from '@playwright/test'

import { typeCode as read } from './fake-client.js'

const TEXT = 'BLITI:AEAACAQDAQCQMBYIBEFAWDANBYHYBAMCQOCILBUHRCEYVC4MRWHI7EER'

test.beforeEach(async ({ page }) => {
	await page.addInitScript(() => Object.defineProperty(navigator, 'bluetooth', { value: {} }))
	await page.goto('/')
})

test('the code text reads however it was typed', async ({ page }) => {
	const grouped = TEXT.split(':').pop().toLowerCase().match(/.{1,4}/g).join('-')
	for (const typed of [TEXT, TEXT.toLowerCase(), grouped]) {
		await read(page, typed)
		await expect(page.locator('.chip')).toHaveText('AFVB2XNR5G2JGXI')
		await page.getByRole('button', { name: 'Scan a different code' }).click()
	}
})

test('a code at a payload version this build does not read is reported as that', async ({ page }) => {
	await read(page, 'BLITI:AJNFUWS2LJNFUWS2LJNFUWS2LJNFUWS2LJNFUWS2LJNFUWS2LJNFUWS2')
	await expect(
		page.getByText('That code is Tamanu Iti payload version 2, which this app does not read.', { exact: true }),
	).toBeVisible()
})

test('text that is not a code is reported as that', async ({ page }) => {
	await read(page, 'WIFI:S:site;T:WPA;P:passphrase;;')
	await expect(page.getByText("This isn't a Tamanu Iti code.", { exact: true })).toBeVisible()
	await expect(page.getByPlaceholder('BLITI:...')).toHaveClass(/field-fault/)
})
