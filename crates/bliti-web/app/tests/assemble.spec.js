import { expect, test } from '@playwright/test'

import { openProvision } from './fake-provision-client.js'
import { heading, numbers, panel, tickParts } from './guide.js'

// The assembly guide (ASM): a page beside the operator app and the provisioning client, at its own
// path, with the provisioning client linking to it and it linking back.

const PAGE = '/assemble'
const TITLE = 'Assemble an Iti'

const WINDOWS = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/130.0 Safari/537.36'
const MAC = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 Chrome/130.0 Safari/537.36'
const LINUX = 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/130.0 Safari/537.36'
const PHONE = 'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 Chrome/130.0 Mobile Safari/537.36'

const controlled = (page) => page.waitForFunction(() => navigator.serviceWorker.controller !== null)
test('serves the guide at its path, titled Assemble an Iti', async ({ page }) => {
	await page.goto(PAGE)
	await expect(page).toHaveTitle(TITLE)
	await expect(page.getByRole('heading', { level: 1, name: TITLE })).toBeVisible()
})

test('links to the provisioning client, and the provisioning client links back', async ({ page }) => {
	await page.goto(PAGE)
	await expect(page.getByRole('link', { name: 'provisioning page' })).toHaveAttribute('href', '/provision')

	await openProvision(page)
	await page.getByRole('link', { name: 'Follow the assembly guide' }).click()
	await expect(page).toHaveURL(PAGE)
	await expect(page.getByRole('heading', { level: 1, name: TITLE })).toBeVisible()
})

test('links to the testing and shipping pages after the Iti is ready', async ({ page }) => {
	await page.goto(PAGE)
	await expect(page.locator('.ready')).toContainText('The Iti is ready.')
	await expect(page.getByRole('link', { name: 'Test it' })).toHaveAttribute('href', '/test')
	await expect(page.getByRole('link', { name: 'Ship it' })).toHaveAttribute('href', '/ship')
})

test('puts starting over and the links to the other pages on one row', async ({ page }) => {
	await page.goto(PAGE)
	const tops = []
	for (const target of [
		page.getByRole('button', { name: 'Do a new one' }),
		page.getByRole('link', { name: 'Test it' }),
		page.getByRole('link', { name: 'Ship it' }),
	]) tops.push((await target.boundingBox()).y)
	expect(Math.max(...tops) - Math.min(...tops)).toBeLessThan(2)
})

test('no longer carries the testing and shipping sections', async ({ page }) => {
	await page.goto(PAGE)
	await expect(page.locator('#testing, #shipping')).toHaveCount(0)
	await expect(page.getByText('steps done')).toHaveCount(0)
})

test('shows photos that load, each described', async ({ page }) => {
	await page.goto(PAGE)
	await page.evaluate(() => document.querySelectorAll('.parts details').forEach((details) => (details.open = true)))
	await heading(page, 5).click()
	const photos = await page.locator('img.photo').all()
	expect(photos.length).toBeGreaterThan(0)
	for (const photo of photos) {
		await photo.scrollIntoViewIfNeeded()
		await expect.poll(() => photo.evaluate((img) => img.naturalWidth)).toBeGreaterThan(0)
		expect(await photo.getAttribute('alt')).toBeTruthy()
	}
})

test('links to the provisioning image, its digest and its manifest', async ({ page }) => {
	await page.goto(PAGE)
	for (const path of [
		'/downloads/tamanu-iti-provision.img.zst',
		'/downloads/tamanu-iti-provision.img.zst.sha256',
		'/downloads/tamanu-iti-provision.txt',
	]) {
		await expect(page.locator(`a[href="${path}"]`).first()).toBeVisible()
	}
})

test.describe('writing the image', () => {
	for (const [system, agent] of [
		['windows', WINDOWS],
		['macos', MAC],
		['linux', LINUX],
	]) {
		test.describe(system, () => {
			test.use({ userAgent: agent })

			test(`opens on the tab for ${system}`, async ({ page }) => {
				await page.goto(PAGE)
				await expect(page.locator(`#tab-${system}`)).toHaveAttribute('aria-selected', 'true')
				await expect(page.locator(`#os-${system}`)).toBeVisible()
				await expect(page.locator('.os:visible')).toHaveCount(1)
			})
		})
	}

	test('shows another system when its tab is chosen', async ({ page }) => {
		await page.goto(PAGE)
		await page.locator('#tab-macos').click()
		await expect(page.locator('#os-macos')).toBeVisible()
		await expect(page.locator('#os-windows')).toBeHidden()
	})

	test.describe('on a phone', () => {
		test.use({ userAgent: PHONE })

		test('says the stick is written from a computer', async ({ page }) => {
			await page.goto(PAGE)
			await expect(page.getByText('Write the stick from a computer.')).toBeVisible()
		})
	})

	test('does not say so on a computer', async ({ page }) => {
		await page.goto(PAGE)
		await expect(page.getByText('Write the stick from a computer.')).toBeHidden()
	})

	test('collapses the image section when it is ticked, and opens it again when it is not', async ({ page }) => {
		await page.goto(PAGE)
		const fold = page.locator('#image .fold')
		await expect(fold).toBeVisible()
		await page.locator('#image-done').check()
		await expect(fold).toBeHidden()
		await page.locator('#image-done').uncheck()
		await expect(fold).toBeVisible()
	})
})

test.describe('the steps', () => {
	test('are numbered in order, one number to each', async ({ page }) => {
		await page.goto(PAGE)
		const count = await numbers(page).count()
		expect(count).toBeGreaterThan(10)
		for (let i = 0; i < count; i++) await expect(numbers(page).nth(i)).toHaveText(String(i + 1))
	})

	test('are ticked and unticked by their number', async ({ page }) => {
		await page.goto(PAGE)
		await tickParts(page)
		const first = numbers(page).first()
		await first.click()
		await expect(first).toHaveAttribute('aria-pressed', 'true')
		await expect(first).toHaveText('✓')
		await first.click()
		await expect(first).toHaveAttribute('aria-pressed', 'false')
		await expect(first).toHaveText('1')
	})

	test('do not number a step whose number is a button twice', async ({ page }) => {
		await page.goto(PAGE)
		const before = await page.locator('.build > li').first().evaluate((li) => getComputedStyle(li, '::before').content)
		expect(before).toBe('none')
	})
})

test.describe('the order of the steps', () => {
	test('lets only the first step be ticked to begin with', async ({ page }) => {
		await page.goto(PAGE)
		await tickParts(page)
		await expect(numbers(page).nth(0)).toBeEnabled()
		await expect(numbers(page).nth(1)).toBeDisabled()
		await expect(numbers(page).nth(5)).toBeDisabled()
	})

	test('lets the next step be ticked once a step is done', async ({ page }) => {
		await page.goto(PAGE)
		await tickParts(page)
		await numbers(page).nth(0).click()
		await expect(numbers(page).nth(1)).toBeEnabled()
		await expect(numbers(page).nth(2)).toBeDisabled()
	})

	test('lets only the last step done be unticked', async ({ page }) => {
		await page.goto(PAGE)
		await tickParts(page)
		await numbers(page).nth(0).click()
		await numbers(page).nth(1).click()
		await expect(numbers(page).nth(0)).toBeDisabled()
		await expect(numbers(page).nth(1)).toBeEnabled()
		await numbers(page).nth(1).click()
		await expect(numbers(page).nth(0)).toBeEnabled()
		await expect(numbers(page).nth(1)).toBeEnabled()
	})

	test('still opens a step that cannot be ticked yet, to read it', async ({ page }) => {
		await page.goto(PAGE)
		await heading(page, 8).click()
		await expect(panel(page, 8)).toHaveJSProperty('open', true)
	})

	test('begins again at the first step after starting over', async ({ page }) => {
		await page.goto(PAGE)
		await tickParts(page)
		const count = await numbers(page).count()
		for (let i = 0; i < count; i++) await numbers(page).nth(i).click()
		await page.getByRole('button', { name: 'Do a new one' }).click()
		await expect(numbers(page).nth(0)).toBeDisabled()
		await page.locator('#p-case').check()
		await page.locator('#p-driver').check()
		await expect(numbers(page).nth(0)).toBeEnabled()
		await expect(numbers(page).nth(1)).toBeDisabled()
	})
})

test.describe('the parts a step is held for', () => {
	test('hold the first step until its parts are ticked', async ({ page }) => {
		await page.goto(PAGE)
		await expect(numbers(page).nth(0)).toBeDisabled()
		await expect(numbers(page).nth(0)).toHaveAttribute('title', 'Tick the parts it needs first')
		await page.locator('#p-case').check()
		await expect(numbers(page).nth(0)).toBeDisabled()
		await page.locator('#p-driver').check()
		await expect(numbers(page).nth(0)).toBeEnabled()
	})

	test('hold a step whose part is missing, while the ones before it can be done', async ({ page }) => {
		await page.goto(PAGE)
		await tickParts(page)
		await page.locator('#p-hat').uncheck()
		for (let i = 0; i < 6; i++) await numbers(page).nth(i).click()
		await expect(numbers(page).nth(6)).toBeDisabled()
		await page.locator('#p-hat').check()
		await expect(numbers(page).nth(6)).toBeEnabled()
	})

	test('leave a step that needs no part to the order alone', async ({ page }) => {
		await page.goto(PAGE)
		await tickParts(page)
		for (let i = 0; i < 9; i++) await numbers(page).nth(i).click()
		await page.locator('#p-pi').uncheck()
		await expect(numbers(page).nth(9)).toBeEnabled()
	})

	test('still let the last step done be unticked if a part is unticked after', async ({ page }) => {
		await page.goto(PAGE)
		await tickParts(page)
		await numbers(page).nth(0).click()
		await page.locator('#p-case').uncheck()
		await expect(numbers(page).nth(0)).toBeEnabled()
	})
})

test.describe('opening a step', () => {
	test('starts with the first step open and the rest closed', async ({ page }) => {
		await page.goto(PAGE)
		await expect(panel(page, 0)).toHaveJSProperty('open', true)
		const count = await page.locator('.build > li').count()
		for (let i = 1; i < count; i++) await expect(panel(page, i)).toHaveJSProperty('open', false)
	})

	test('opens a step when it is clicked, and closes the one that was open', async ({ page }) => {
		await page.goto(PAGE)
		await heading(page, 2).click()
		await expect(panel(page, 2)).toHaveJSProperty('open', true)
		await expect(panel(page, 0)).toHaveJSProperty('open', false)
	})

	test('shows a photo or video where a step is open', async ({ page }) => {
		await page.goto(PAGE)
		await expect(panel(page, 0).locator('.photo, video').first()).toBeVisible()
	})

	test('closes a step when it is ticked', async ({ page }) => {
		await page.goto(PAGE)
		await tickParts(page)
		await numbers(page).first().click()
		await expect(panel(page, 0)).toHaveJSProperty('open', false)
		await expect(page.locator('.build > li').first()).toHaveClass(/done/)
	})

	test('opens the next step when one is ticked', async ({ page }) => {
		await page.goto(PAGE)
		await tickParts(page)
		await numbers(page).nth(0).click()
		await expect(panel(page, 1)).toHaveJSProperty('open', true)
		await expect(panel(page, 0)).toHaveJSProperty('open', false)
	})

	test('opens nothing after the last step is ticked', async ({ page }) => {
		await page.goto(PAGE)
		await tickParts(page)
		const count = await numbers(page).count()
		for (let i = 0; i < count; i++) await numbers(page).nth(i).click()
		for (let i = 0; i < count; i++) await expect(panel(page, i)).toHaveJSProperty('open', false)
	})

	test('does not open the next step when a step is unticked', async ({ page }) => {
		await page.goto(PAGE)
		await tickParts(page)
		await numbers(page).nth(0).click()
		await heading(page, 4).click()
		await numbers(page).nth(0).click()
		await expect(panel(page, 4)).toHaveJSProperty('open', true)
		await expect(panel(page, 1)).toHaveJSProperty('open', false)
	})

	test('plays a step\'s video as it opens, and stops it as it closes', async ({ page }) => {
		await page.addInitScript(() => {
			window.__played = []
			HTMLMediaElement.prototype.play = function () {
				window.__played.push(`play ${this.id}`)
				return Promise.resolve()
			}
			HTMLMediaElement.prototype.pause = function () {
				window.__played.push(`pause ${this.id}`)
			}
		})
		await page.goto(PAGE)
		await page.evaluate(() => {
			const video = document.createElement('video')
			video.id = 'two'
			video.loop = true
			video.muted = true
			document.querySelectorAll('.build > li')[1].querySelector('.detail').append(video)
		})
		await heading(page, 1).click()
		await expect.poll(() => page.evaluate(() => window.__played)).toContain('play two')
		await heading(page, 2).click()
		await expect.poll(() => page.evaluate(() => window.__played)).toContain('pause two')
	})
})

test.describe('the parts a step needs', () => {
	const pill = (page, index) => page.locator('.build > li').nth(index).locator('.pill')

	test('are counted from the start, with every part still needed', async ({ page }) => {
		await page.goto(PAGE)
		await expect(pill(page, 0)).toHaveText('0 of 2 parts')
		await expect(page.locator('.build > li').first()).toHaveClass(/short/)
		await expect(page.locator('.build > li').first().locator('.missing')).toContainText('X1207-C1 metal case for the X1208')
	})

	test('are counted as parts are ticked, and named in the step', async ({ page }) => {
		await page.goto(PAGE)
		await page.locator('#p-case').check()
		await expect(pill(page, 0)).toHaveText('1 of 2 parts')
		await expect(page.locator('.build > li').first().locator('.missing')).toContainText('Phillips screwdriver')
	})

	test('are met when every part of a step is ticked', async ({ page }) => {
		await page.goto(PAGE)
		await page.locator('#p-case').check()
		await page.locator('#p-driver').check()
		await expect(pill(page, 0)).toBeHidden()
		await expect(page.locator('.build > li').first()).not.toHaveClass(/short/)
	})

	test('leave a step that needs none alone', async ({ page }) => {
		await page.goto(PAGE)
		await page.locator('#p-case').check()
		await expect(page.locator('.build > li').nth(9)).not.toHaveClass(/short/)
	})

	test('are counted again once a part is unticked', async ({ page }) => {
		await page.goto(PAGE)
		await page.locator('#p-case').check()
		await page.locator('#p-case').uncheck()
		await expect(pill(page, 0)).toHaveText('0 of 2 parts')
	})

	test('name only parts the page lists', async ({ page }) => {
		await page.goto(PAGE)
		const known = await page.locator('.parts input').evaluateAll((boxes) => boxes.map((box) => box.id))
		const needed = await page.locator('.build > li[data-needs]').evaluateAll((steps) => steps.flatMap((step) => step.dataset.needs.split(' ')))
		for (const id of needed) expect(known).toContain(id)
	})
})

test.describe('starting over', () => {
	test('is offered only once every step is done', async ({ page }) => {
		await page.goto(PAGE)
		await tickParts(page)
		const again = page.getByRole('button', { name: 'Do a new one' })
		await expect(again).toBeDisabled()
		const count = await numbers(page).count()
		for (let i = 0; i < count - 1; i++) await numbers(page).nth(i).click()
		await expect(again).toBeDisabled()
		await numbers(page).nth(count - 1).click()
		await expect(again).toBeEnabled()
	})

	test('clears the steps and the parts of one Iti, and keeps the image and the parts of provisioning', async ({ page }) => {
		await page.goto(PAGE)
		await tickParts(page)
		await page.locator('#image-done').check()
		const count = await numbers(page).count()
		for (let i = 0; i < count; i++) await numbers(page).nth(i).click()

		await page.getByRole('button', { name: 'Do a new one' }).click()

		await expect(page.locator('.build > li.done')).toHaveCount(0)
		await expect(page.locator('#p-pi')).not.toBeChecked()
		await expect(page.locator('#p-stick')).toBeChecked()
		await expect(page.locator('#image-done')).toBeChecked()
		await expect(page.getByRole('button', { name: 'Do a new one' })).toBeDisabled()
		await expect(numbers(page).first()).toHaveText('1')
	})

	test('opens the first step again, and returns to the parts', async ({ page }) => {
		await page.goto(PAGE)
		await tickParts(page)
		const count = await numbers(page).count()
		for (let i = 0; i < count; i++) await numbers(page).nth(i).click()
		await expect(panel(page, 0)).toHaveJSProperty('open', false)
		await page.getByRole('button', { name: 'Do a new one' }).click()
		await expect(panel(page, 0)).toHaveJSProperty('open', true)
		await expect(page.locator('#parts')).toBeInViewport()
	})
})

test('opens a part to show its sources, closed until then', async ({ page }) => {
	await page.goto(PAGE)
	const pi = page.locator('#p-pi').locator('xpath=ancestor::li[1]')
	const sources = pi.locator('.sources')
	await expect(sources).toBeHidden()
	await pi.locator('summary').click()
	await expect(sources).toBeVisible()
})

test.describe('offline after one load of the operator app', () => {
	test('opens the guide at its path', async ({ page, context }) => {
		await page.goto('/')
		await controlled(page)

		await context.setOffline(true)
		await page.goto(PAGE)
		await expect(page.getByRole('heading', { level: 1, name: TITLE })).toBeVisible()
	})

	test('opens the guide at its path with a trailing slash', async ({ page, context }) => {
		await page.goto('/')
		await controlled(page)

		await context.setOffline(true)
		await page.goto(`${PAGE}/`)
		await expect(page.getByRole('heading', { level: 1, name: TITLE })).toBeVisible()
	})

	test('still ticks steps offline', async ({ page, context }) => {
		await page.goto('/')
		await controlled(page)

		await context.setOffline(true)
		await page.goto(PAGE)
		await tickParts(page)
		await numbers(page).first().click()
		await expect(numbers(page).first()).toHaveAttribute('aria-pressed', 'true')
	})
})
