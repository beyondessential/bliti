import { expect, test } from '@playwright/test'

import { closeChannel, emit, openChannel } from './fake-client.js'

const hello = (extra = {}) => ({
	kind: 'message',
	message: { type: 'hello', name: 'bliti', version: '0.4.2', ...extra },
})

const fact = (name, body) => ({
	kind: 'message',
	message: { type: 'fact', at: 1, fact: name, traits: { status: { is: 'passed' } }, ...body },
})

const hostname = fact('hostname', { kind: 'text', value: 'tamanu-iti' })
const address = fact('network-address', {
	kind: 'ipv4',
	value: '192.0.2.10',
	traits: { status: { is: 'passed' }, interface: { name: 'end0', route: 'default' } },
})

test.describe('the device view', () => {
	// The hello is a fact about the software rather than a reading, so it goes in the record and the
	// device line rather than taking a tile. MSG asks the client display it (VIEW).
	test('records the name and version the device reported', async ({ page }) => {
		await openChannel(page)
		await emit(page, hello())
		await expect(page.locator('.log')).toContainText('name bliti')
		await expect(page.locator('.log')).toContainText('version 0.4.2')
		await expect(page.locator('.software')).toContainText('bliti 0.4.2')
	})

	// A device that grows its hello must not have the new member silently dropped in the record.
	test('every member of the hello is recorded, including ones this build predates', async ({ page }) => {
		await openChannel(page)
		await emit(page, hello({ built: '2026-09-17' }))
		await expect(page.locator('.log')).toContainText('built 2026-09-17')
	})

	// The header names the device from the facts that say which device it is; they get no tiles (VIEW).
	test('shows what the device reports about itself, in the header and as tiles', async ({ page }) => {
		await openChannel(page)
		await emit(page, hostname)
		await emit(page, address)
		await expect(page.locator('.device .name')).toHaveText('tamanu-iti')
		await expect(page.getByText('192.0.2.10')).toBeVisible()
		// hostname is in the header, not a tile; the only tile is the address.
		await expect(page.locator('.tile')).toHaveCount(1)
	})
})

test.describe('a device newer than this build', () => {
	// An ignorable unknown is passed over in silence, and costs the view nothing (MSG).
	test('skipping leaves the view whole and says nothing about it', async ({ page }) => {
		await openChannel(page)
		await emit(page, hostname)
		await emit(page, { kind: 'skipped', detail: 'message type "telemetry" is not known to this build' })
		await expect(page.locator('.device .name')).toHaveText('tamanu-iti')
		await expect(page.locator('.notice')).toHaveCount(0)
	})

	// A critical unknown is refused: said plainly, not as a fault, and never at the cost of what the
	// application did understand.
	test('refusing is shown without blanking the view', async ({ page }) => {
		await openChannel(page)
		await emit(page, hostname)
		await emit(page, { kind: 'refused', detail: 'critical members not known to this build: redact' })

		const notice = page.locator('.notice')
		await expect(notice).toHaveCount(1)
		await expect(notice).toContainText('too old to act on')
		await expect(notice).not.toHaveClass(/fault/)
		await expect(page.locator('.device .name')).toHaveText('tamanu-iti')
	})
})

test.describe('a device not speaking the protocol', () => {
	test('is surfaced to the operator rather than leaving the view blank', async ({ page }) => {
		await openChannel(page)
		await emit(page, hostname)
		await emit(page, { kind: 'fault', detail: 'message is not valid JSON: expected value' })

		const notice = page.locator('.notice.fault')
		await expect(notice).toHaveCount(1)
		await expect(notice).toContainText('not speaking the protocol')
		await expect(page.locator('.device .name')).toHaveText('tamanu-iti')
	})
})

test.describe('the feed lifecycle', () => {
	const feeds = (page) => page.evaluate(() => window.__blitiFeeds)

	// The device pushes the default feed unprompted, so a feed is running the moment the channel is
	// open, with no subscribe round trip (MSG).
	test('the pushed feed runs once the channel is open', async ({ page }) => {
		await openChannel(page)
		await expect.poll(() => feeds(page)).toHaveLength(1)
		const [first] = await feeds(page)
		expect(first.source).toBe('push')
		expect(first.open).toBe(true)
	})

	// Hiding the page declines the feed by closing it; showing it resumes with a subscribe for
	// `default` (VIEW, MSG).
	test('hiding declines the feed and showing resumes with a subscribe', async ({ page }) => {
		await openChannel(page)
		await expect.poll(() => feeds(page)).toHaveLength(1)

		await page.evaluate(() => {
			Object.defineProperty(document, 'hidden', { value: true, configurable: true })
			document.dispatchEvent(new Event('visibilitychange'))
		})
		await expect.poll(async () => (await feeds(page))[0].open).toBe(false)

		await page.evaluate(() => {
			Object.defineProperty(document, 'hidden', { value: false, configurable: true })
			document.dispatchEvent(new Event('visibilitychange'))
		})
		await expect.poll(() => feeds(page)).toHaveLength(2)
		const all = await feeds(page)
		expect(all[1].source).toBe('subscribe')
		expect(all[1].topic).toBe('default')
		expect(all[1].open).toBe(true)
	})

	// A resume opened while the page is being hidden is declined anyway: tracking the resolved handle
	// rather than the in-flight resume would leave the device pushing to a page nobody is looking at.
	test('a resume opened while the page is being hidden is declined anyway', async ({ page }) => {
		await page.addInitScript('window.__blitiResumeDelay = 300')
		await openChannel(page)
		await expect.poll(() => feeds(page)).toHaveLength(1)

		// Hide, which declines the pushed feed.
		await page.evaluate(() => {
			Object.defineProperty(document, 'hidden', { value: true, configurable: true })
			document.dispatchEvent(new Event('visibilitychange'))
		})
		await expect.poll(async () => (await feeds(page))[0].open).toBe(false)

		// Show, starting a resume, then hide again before it resolves.
		await page.evaluate(() => {
			Object.defineProperty(document, 'hidden', { value: false, configurable: true })
			document.dispatchEvent(new Event('visibilitychange'))
		})
		await page.evaluate(() => {
			Object.defineProperty(document, 'hidden', { value: true, configurable: true })
			document.dispatchEvent(new Event('visibilitychange'))
		})

		// The subscribe resolves, and is closed straight away rather than left running.
		await expect.poll(() => feeds(page)).toHaveLength(2)
		await expect.poll(async () => (await feeds(page))[1].open).toBe(false)
	})

	// Readings already in flight when the feed is declined are discarded rather than faulting (MSG).
	test('data in flight after declining is discarded rather than faulting', async ({ page }) => {
		await openChannel(page)
		await emit(page, hello())
		await expect.poll(() => feeds(page)).toHaveLength(1)

		await page.evaluate(() => {
			Object.defineProperty(document, 'hidden', { value: true, configurable: true })
			document.dispatchEvent(new Event('visibilitychange'))
		})
		await expect.poll(async () => (await feeds(page))[0].open).toBe(false)

		// The device had already sent this when the feed closed; the closed feed does not deliver it.
		await page.evaluate(() =>
			window.__blitiFeeds[0].deliver({
				kind: 'message',
				message: { type: 'fact', at: 2, fact: 'hostname', traits: { status: { is: 'passed' } }, kind: 'text', value: 'late' },
			}),
		)

		await expect(page.locator('.notice')).toHaveCount(0)
		await expect(page.getByText('late')).toBeHidden()
	})
})

// A channel closes with nothing having gone wrong as readily as it closes on a fault; either way the
// operator is told, and offered the channel again without rereading the code (CHN).
test.describe('when the channel closes', () => {
	test('the operator is told, and is offered the channel again', async ({ page }) => {
		await openChannel(page)
		await emit(page, hostname)
		await expect(page.locator('.device .name')).toHaveText('tamanu-iti')

		await closeChannel(page)

		await expect(page.getByText('The device disconnected.')).toBeVisible()
		await expect(page.getByRole('button', { name: 'Find the device' })).toBeVisible()
	})

	test('a fault that ends the connection is reported with its reason', async ({ page }) => {
		await openChannel(page)
		await emit(page, hello())

		await closeChannel(page, 'decompression failed: corrupt deflate stream')

		await expect(page.locator('p.muted').filter({ hasText: /decompression failed/ })).toBeVisible()
		await expect(page.locator('.log')).toContainText('the connection to the device closed: decompression failed')
		await expect(page.getByRole('button', { name: 'Find the device' })).toBeVisible()
	})
})
