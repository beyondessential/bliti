// Downloading a code the application has read as SVG, for printing a replacement (WEB).

import { readFile } from 'node:fs/promises'

import { expect, test } from '@playwright/test'

import { installFakeClient } from './fake-client.js'

async function read(page, code) {
	await page.getByPlaceholder('BLITI:...').fill(code)
	await page.getByRole('button', { name: 'Use' }).click()
}

async function downloaded(page) {
	const [download] = await Promise.all([
		page.waitForEvent('download'),
		page.getByRole('button', { name: 'Download SVG' }).click(),
	])
	return { name: download.suggestedFilename(), body: await readFile(await download.path(), 'utf8') }
}

test('the file is the code the client read, named after the last four characters of its code', async ({ page }) => {
	await page.addInitScript(installFakeClient)
	await page.goto('/')
	await read(page, 'BLITI:AHFYTP4T6K2M9WQX')
	const { name, body } = await downloaded(page)
	expect(name).toBe('bliti-9WQX.svg')
	expect(body).toBe('<svg xmlns="http://www.w3.org/2000/svg"/>')
})

// The real client, with its wasm module, reading a real payload. Bluetooth is only asked whether it
// exists: nothing here connects.
test('the real client produces the image from the payload', async ({ page }) => {
	await page.addInitScript(() => Object.defineProperty(navigator, 'bluetooth', { value: {} }))
	await page.goto('/')
	await read(
		page,
		'BLITI:AEAACAQDAQCQMBYIBEFAWDANBYHYBAMCQOCILBUHRCEYVC4MRWHI7EER',
	)
	const { name, body } = await downloaded(page)
	expect(name).toBe('bliti-7EER.svg')
	expect(body).toMatch(/^<\?xml[^>]*>\s*<svg[^>]*>.*<\/svg>$/s)
	expect(body).not.toContain('<text')
})
