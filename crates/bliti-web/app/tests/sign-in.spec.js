// The sign-in screen of TSCR, Connect to Tailscale: by an auth key typed or scanned, or by a login
// link, over a Tailscale stream of its own, and back to where it was opened from once signed in.

import { readFileSync } from 'node:fs'

import { expect, test } from '@playwright/test'

import {
	actStreams,
	devMode,
	feed,
	message,
	openAdvanced,
	openChannel,
	sayOnTailscale,
	standingActs,
	tailscaleSent,
} from './fake-client.js'
import { initSync, text_qr as textQr } from '../src/wasm/bliti_web.js'

initSync({ module: readFileSync(new URL('../src/wasm/bliti_web_bg.wasm', import.meta.url)) })

const fact = (name, value) => message({ type: 'fact', at: 1, fact: name, kind: 'text', value, traits: { status: { is: 'passed' } } })

const title = (page) => page.getByRole('heading', { level: 1 })
const LINK = 'https://login.tailscale.com/a/1f2e3d4c5b6a'

function recordHandingOn() {
	window.__copied = []
	window.__shared = []
	window.__opened = []
	Object.defineProperty(navigator, 'clipboard', { value: { writeText: async (text) => window.__copied.push(text) } })
	Object.defineProperty(navigator, 'share', { value: async (data) => window.__shared.push(data) })
	window.open = (url) => window.__opened.push(url)
}

// Connect to Tailscale, opened from the Status row of a device that is not signed in.
async function openSignIn(page) {
	await page.addInitScript(recordHandingOn)
	await openChannel(page, fact('hostname', 'clinic-iti-07'), fact('tailscale-state', 'needs-login'))
	await standingActs(page, { tailscale: ['sign-in', 'netcheck'] })
	await sayOnTailscale(page, { type: 'tailscale-acts', acts: ['sign-in', 'netcheck'] })
	await page.getByRole('button', { name: 'Connect', exact: true }).click()
	await expect(title(page)).toHaveText('Connect to Tailscale')
}

const signIns = async (page) => (await tailscaleSent(page)).filter((each) => each.type === 'tailscale-act')
const connect = (page) => page.locator('.form .go').getByRole('button')

test.describe('by auth key', () => {
	test('opens on the key, hidden until Show, with the device name filled from the hostname', async ({ page }) => {
		await openSignIn(page)
		const key = page.getByLabel('Auth key')
		await expect(key).toHaveAttribute('type', 'password')
		await page.getByRole('button', { name: 'Show' }).click()
		await expect(key).toHaveAttribute('type', 'text')
		await expect(page.getByLabel('Device name')).toHaveValue('clinic-iti-07')
		await expect(page.getByRole('button', { name: 'Use a login link' })).toBeVisible()
	})

	test('sends the key typed, and no name where it was left as the hostname', async ({ page }) => {
		await openSignIn(page)
		await expect(connect(page)).toBeDisabled()
		await page.getByLabel('Auth key').fill('tskey-auth-kQ9')
		await connect(page).click()
		expect(await signIns(page)).toEqual([{ type: 'tailscale-act', act: 'sign-in', method: 'auth-key', key: 'tskey-auth-kQ9' }])
	})

	test('sends the name where the operator changed it', async ({ page }) => {
		await openSignIn(page)
		await page.getByLabel('Auth key').fill('tskey-auth-kQ9')
		await page.getByLabel('Device name').fill('ward-3-iti')
		await connect(page).click()
		expect((await signIns(page))[0].name).toBe('ward-3-iti')
	})

	test('shows a rejected key in the device words, leaving the form to try again', async ({ page }) => {
		await openSignIn(page)
		await page.getByLabel('Auth key').fill('tskey-auth-old')
		await connect(page).click()
		await sayOnTailscale(page, { type: 'tailscale-accepted' })
		await sayOnTailscale(page, { type: 'tailscale-failed', reason: 'Invalid auth key: key has expired.' })
		await expect(page.locator('.why.reason')).toHaveText('Invalid auth key: key has expired.')
		await expect(page.getByLabel('Auth key')).toHaveValue('tskey-auth-old')
		await expect(connect(page)).toBeEnabled()
	})

	test('starts empty again once left and returned to', async ({ page }) => {
		await openSignIn(page)
		await page.getByLabel('Auth key').fill('tskey-auth-kQ9')
		await page.getByRole('button', { name: 'Back' }).click()
		await page.getByRole('button', { name: 'Connect', exact: true }).click()
		await expect(page.getByLabel('Auth key')).toHaveValue('')
	})
})

test('a key held up to the camera fills the field', async ({ browser }) => {
	const key = 'tskey-auth-kScanned0123456789'
	const svg = `<svg xmlns="http://www.w3.org/2000/svg" ${textQr(key).trim().slice(4)}`
	const page = await browser.newPage()
	await page.addInitScript(() => delete window.BarcodeDetector)
	await page.addInitScript((svg) => {
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
	}, svg)
	await openSignIn(page)
	await page.getByRole('button', { name: 'Scan a QR code' }).click()
	await expect(page.getByLabel('Auth key')).toHaveValue(key)
	await page.close()
})

test.describe('by login link', () => {
	async function askForLink(page) {
		await openSignIn(page)
		await page.getByRole('button', { name: 'Use a login link' }).click()
		await page.getByRole('button', { name: 'Get a login link' }).click()
	}

	test('waits for the first link, then shows it as a QR code with Open, Share and Copy', async ({ page }) => {
		await askForLink(page)
		expect(await signIns(page)).toEqual([{ type: 'tailscale-act', act: 'sign-in', method: 'login-link' }])
		await expect(page.getByRole('status')).toHaveText('Getting a link…')
		await sayOnTailscale(page, { type: 'tailscale-accepted' })
		await sayOnTailscale(page, { type: 'tailscale-login-link', url: LINK })
		await expect(page.locator('.join.link .qr svg')).toBeVisible()
		await expect(page.getByText('The Iti connects once someone approves this link.')).toBeVisible()
		await expect(page.locator('.form .code')).toHaveText(LINK)
		await page.getByRole('button', { name: 'Open' }).click()
		await page.getByRole('button', { name: 'Share' }).click()
		await page.getByRole('button', { name: 'Copy' }).click()
		await expect(page.getByRole('button', { name: 'Copied' })).toBeVisible()
		expect(await page.evaluate(() => [window.__opened, window.__shared, window.__copied])).toEqual([[LINK], [{ url: LINK }], [LINK]])
	})

	test('replaces the link with the latest the device sends, everywhere', async ({ page }) => {
		await askForLink(page)
		await sayOnTailscale(page, { type: 'tailscale-login-link', url: LINK })
		const next = 'https://login.tailscale.com/a/99aa88bb77cc'
		await sayOnTailscale(page, { type: 'tailscale-login-link', url: next })
		await expect(page.locator('.form .code')).toHaveText(next)
		await page.getByRole('button', { name: 'Open' }).click()
		await page.getByRole('button', { name: 'Copy' }).click()
		expect(await page.evaluate(() => [window.__opened, window.__copied])).toEqual([[next], [next]])
	})

	test('says a lapsed link in the device words, offering a new one', async ({ page }) => {
		await askForLink(page)
		await sayOnTailscale(page, { type: 'tailscale-login-link', url: LINK })
		await sayOnTailscale(page, { type: 'tailscale-failed', reason: 'The login link was not approved in time.' })
		await expect(page.locator('.form .code')).toHaveCount(0)
		await expect(page.locator('.why.reason')).toHaveText('The login link was not approved in time.')
		await page.getByRole('button', { name: 'Get a new link' }).click()
		expect((await signIns(page)).map((each) => each.method)).toEqual(['login-link', 'login-link'])
	})

	test('drops the link once the stream closes, offering a new one', async ({ page }) => {
		await askForLink(page)
		await sayOnTailscale(page, { type: 'tailscale-login-link', url: LINK })
		await page.evaluate(() => window.__blitiTailscaleStream.close())
		await expect(page.locator('.form .code')).toHaveCount(0)
		await expect(page.getByRole('button', { name: 'Get a new link' })).toBeVisible()
	})

	test('never records the link in the activity record', async ({ page }) => {
		await askForLink(page)
		await devMode(page)
		await sayOnTailscale(page, { type: 'tailscale-login-link', url: LINK })
		await expect(page.locator('.log')).toContainText('tailscale-login-link')
		await expect(page.locator('.log')).not.toContainText('1f2e3d4c5b6a')
	})
})

test.describe('once signed in', () => {
	for (const state of ['running', 'needs-approval']) {
		test(`returns to the Status screen on reaching ${state}, and the browser's back does not land on it again`, async ({ page }) => {
			await openSignIn(page)
			await feed(page, fact('tailscale-state', state))
			await expect(title(page)).toHaveText('clinic-iti-07')
			await openAdvanced(page)
			await page.getByRole('button', { name: 'Tailscale', exact: true }).click()
			await page.goBack()
			await expect(title(page)).toHaveText('clinic-iti-07')
		})
	}

	test('returns to the Tailscale screen where it was opened from there', async ({ page }) => {
		await page.addInitScript(recordHandingOn)
		await openChannel(page, fact('hostname', 'clinic-iti-07'), fact('tailscale-state', 'needs-login'))
		await standingActs(page, { tailscale: ['sign-in', 'netcheck'] })
		await openAdvanced(page)
		await page.getByRole('button', { name: 'Tailscale', exact: true }).click()
		await page.locator('section').filter({ hasText: 'Acts' }).getByRole('button', { name: 'Connect' }).click()
		await expect(title(page)).toHaveText('Connect to Tailscale')
		await feed(page, fact('tailscale-state', 'running'))
		await expect(title(page)).toHaveText('Tailscale')
		await page.goBack()
		await expect(title(page)).toHaveText('clinic-iti-07')
	})

	test('closes its stream on leaving', async ({ page }) => {
		await openSignIn(page)
		await feed(page, fact('tailscale-state', 'running'))
		await expect(title(page)).toHaveText('clinic-iti-07')
		const streams = (await actStreams(page)).tailscale
		expect(streams.filter((each) => each.open)).toHaveLength(1)
	})
})
