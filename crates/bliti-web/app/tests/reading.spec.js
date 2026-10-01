// Reading a code typed in, through the real client and its wasm module (WEB, "Reading a QR code";
// QR, "Reading"). Bluetooth is only asked whether it exists: nothing here connects.

import { expect, test } from '@playwright/test'

const TEXT = 'BLITI:AEAACAQDAQCQMBYIBEFAWDANBYHYBAMCQOCILBUHRCEYVC4MRWHI7EER'

const held = (page) => page.locator('section').filter({ has: page.getByRole('heading', { name: 'QR code read' }) })

async function read(page, code) {
	await page.getByPlaceholder('BLITI:...').fill(code)
	await page.getByRole('button', { name: 'Use' }).click()
}

test.beforeEach(async ({ page }) => {
	await page.addInitScript(() => Object.defineProperty(navigator, 'bluetooth', { value: {} }))
	await page.goto('/')
})

test('the code text reads however it was typed', async ({ page }) => {
	const grouped = TEXT.split(':').pop().toLowerCase().match(/.{1,4}/g).join('-')
	for (const typed of [TEXT, TEXT.toLowerCase(), grouped]) {
		await read(page, typed)
		await expect(held(page).locator('p.code')).toHaveText(TEXT)
		await page.getByRole('button', { name: 'Use another code' }).click()
	}
})

test('a code at a payload version this build does not read is reported as that', async ({ page }) => {
	await read(page, 'BLITI:AJNFUWS2LJNFUWS2LJNFUWS2LJNFUWS2LJNFUWS2LJNFUWS2LJNFUWS2')
	await expect(
		page.getByText('That QR code is bliti payload version 2, which this app does not read.', { exact: true }),
	).toBeVisible()
})

test('text that is not a code is reported as that', async ({ page }) => {
	await read(page, 'WIFI:S:site;T:WPA;P:passphrase;;')
	await expect(page.getByText('That is not a bliti QR code.', { exact: true })).toBeVisible()
})
