// Reading a code with the camera while the application is open (WEB, "Reading a QR code").
//
// The camera is a canvas showing the code, and the browser has no `BarcodeDetector`, as most
// desktops do not: the frames are decoded by the wasm module.

import { readFile } from 'node:fs/promises'

import { expect, test } from '@playwright/test'

import { installFakeClient, typeCode as readTyped } from './fake-client.js'

const PAYLOAD =
	'BLITI:AEAACAQDAQCQMBYIBEFAWDANBYHYBAMCQOCILBUHRCEYVC4MRWHI7EER'

const noDetector = () => delete window.BarcodeDetector

// A camera pointed at `svg`, a code as the application exports it, held on a white page.
function cameraShowing(svg) {
	navigator.mediaDevices.getUserMedia = async () => {
		const image = new Image()
		image.src = `data:image/svg+xml,${encodeURIComponent(svg)}`
		await image.decode()
		const canvas = Object.assign(document.createElement('canvas'), { width: 640, height: 480 })
		const context = canvas.getContext('2d')
		const draw = () => {
			context.fillStyle = '#fff'
			context.fillRect(0, 0, canvas.width, canvas.height)
			context.drawImage(image, 140, 60, 360, 360)
		}
		draw()
		setInterval(draw, 100)
		return canvas.captureStream()
	}
}

test('the camera is offered where the browser has no QR detector of its own', async ({ page }) => {
	await page.addInitScript(installFakeClient)
	await page.addInitScript(noDetector)
	await page.goto('/')
	await expect(page.getByRole('button', { name: 'Open camera' })).toBeVisible()
})

test('a code held up to the camera is read without a QR detector in the browser', async ({ browser }) => {
	const bluetooth = () => Object.defineProperty(navigator, 'bluetooth', { value: {} })

	// The code as the real client renders it, taken from its own export.
	const exporting = await browser.newPage()
	await exporting.addInitScript(bluetooth)
	await exporting.goto('/')
	await readTyped(exporting, PAYLOAD)
	const name = await exporting.locator('.chip').textContent()
	const [download] = await Promise.all([
		exporting.waitForEvent('download'),
		exporting.getByRole('button', { name: 'Save code for reprinting' }).click(),
	])
	const svg = await readFile(await download.path(), 'utf8')
	await exporting.close()

	const page = await browser.newPage()
	await page.addInitScript(bluetooth)
	await page.addInitScript(noDetector)
	await page.addInitScript(cameraShowing, svg)
	await page.goto('/')
	await page.getByRole('button', { name: 'Open camera' }).click()
	await expect(page.getByText('Code scanned')).toBeVisible()
	await expect(page.locator('.chip')).toHaveText(name)
	await page.close()
})

test('the camera is asked for a stream larger than a phone browser opens by default', async ({ page }) => {
	// A phone browser left to itself opens the camera at 640 by 480, too coarse to read a code on an
	// enclosure from where an operator holds the phone.
	await page.addInitScript(() => Object.defineProperty(navigator, 'bluetooth', { value: {} }))
	await page.addInitScript(noDetector)
	await page.addInitScript(() => {
		navigator.mediaDevices.getUserMedia = async (constraints) => {
			window.cameraConstraints = constraints
			return document.createElement('canvas').captureStream()
		}
	})
	await page.goto('/')
	await page.getByRole('button', { name: 'Open camera' }).click()
	await expect.poll(() => page.evaluate(() => window.cameraConstraints)).toBeTruthy()
	const { video } = await page.evaluate(() => window.cameraConstraints)
	expect(video.facingMode).toBe('environment')
	expect(video.width.ideal).toBeGreaterThanOrEqual(1920)
	expect(video.height.ideal).toBeGreaterThanOrEqual(1080)
})

test('a camera that cannot be opened is said so, with a way to try again', async ({ page }) => {
	await page.addInitScript(installFakeClient)
	await page.addInitScript(noDetector)
	await page.addInitScript(() => {
		navigator.mediaDevices.getUserMedia = async () => {
			throw new DOMException('Permission denied', 'NotAllowedError')
		}
	})
	await page.goto('/')
	await page.getByRole('button', { name: 'Open camera' }).click()
	await expect(page.getByText('Camera not available')).toBeVisible()
	await expect(page.getByRole('button', { name: 'Try again' })).toBeVisible()
	// Typing the code still works.
	await readTyped(page, 'BLITI:AHFYTP4T6K2M9WQX')
	await expect(page.getByText('Code scanned')).toBeVisible()
})

test('scanning says how, and can be cancelled', async ({ page }) => {
	await page.addInitScript(installFakeClient)
	await page.addInitScript(noDetector)
	await page.addInitScript(cameraShowing, '<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"/>')
	await page.goto('/')
	await page.getByRole('button', { name: 'Open camera' }).click()
	await expect(page.getByText('Point your camera at the code')).toBeVisible()
	await expect(page.getByText('Advanced', { exact: true })).toHaveCount(0)
	await page.getByRole('button', { name: 'Cancel' }).click()
	await expect(page.getByRole('button', { name: 'Open camera' })).toBeVisible()
})
