import { expect, test } from '@playwright/test'

import { emit, openChannel } from './fake-client.js'

const hello = { kind: 'message', message: { type: 'device-hello', name: 'bliti', version: '0.4.2' } }
const identity = {
	kind: 'message',
	message: {
		type: 'identity',
		hostname: 'tamanu-iti',
		addresses: [{ address: '192.0.2.10', interface: 'end0', family: 'ipv4' }],
	},
}

test.describe('the device view', () => {
	test('shows the name and version the device reported', async ({ page }) => {
		await openChannel(page)
		await emit(page, hello)
		await expect(page.getByText('bliti 0.4.2')).toBeVisible()
	})

	test('shows what the device reports about itself', async ({ page }) => {
		await openChannel(page)
		await emit(page, hello)
		await emit(page, identity)
		await expect(page.getByText('tamanu-iti')).toBeVisible()
		await expect(page.getByText('192.0.2.10')).toBeVisible()
	})
})

test.describe('a device newer than this build', () => {
	// An ignorable unknown is passed over in silence, and costs the view nothing (BLI-MSG).
	test('skipping leaves the view whole and says nothing about it', async ({ page }) => {
		await openChannel(page)
		await emit(page, hello)
		await emit(page, { kind: 'skipped', detail: 'message type "readings" is not known to this build' })
		await emit(page, identity)

		await expect(page.getByText('tamanu-iti')).toBeVisible()
		await expect(page.locator('.notice')).toHaveCount(0)
	})

	// A critical unknown is refused: said plainly, but not as a fault, and never at the cost of what
	// the application did understand.
	test('refusing is shown without blanking the view', async ({ page }) => {
		await openChannel(page)
		await emit(page, hello)
		await emit(page, identity)
		await emit(page, { kind: 'refused', detail: 'critical members not known to this build: redact' })

		const notice = page.locator('.notice')
		await expect(notice).toHaveCount(1)
		await expect(notice).toContainText('too old to act on')
		await expect(notice).not.toHaveClass(/fault/)

		// Everything else is still on screen: most of a view beats none of it.
		await expect(page.getByText('tamanu-iti')).toBeVisible()
		await expect(page.getByText('bliti 0.4.2')).toBeVisible()
	})

	// A hello the client cannot process leaves it without the name and version, saying so, and the
	// session carries on rather than being refused.
	test('a refused device-hello leaves the session alive and says what is missing', async ({ page }) => {
		await openChannel(page)
		await emit(page, { kind: 'refused', detail: 'critical members not known to this build: capability' })
		await emit(page, identity)

		await expect(page.getByText('not reported')).toBeVisible()
		await expect(page.getByText('tamanu-iti')).toBeVisible()
	})
})

test.describe('a device not speaking the protocol', () => {
	test('is surfaced to the operator rather than leaving the view blank', async ({ page }) => {
		await openChannel(page)
		await emit(page, hello)
		await emit(page, identity)
		await emit(page, { kind: 'fault', detail: 'message is not valid JSON: expected value' })

		const notice = page.locator('.notice.fault')
		await expect(notice).toHaveCount(1)
		await expect(notice).toContainText('not speaking the protocol')
		await expect(page.getByText('tamanu-iti')).toBeVisible()
	})
})

test.describe('the subscription lifecycle', () => {
	const subscriptions = (page) => page.evaluate(() => window.__blitiSubscriptions)

	test('subscribes once the channel is open', async ({ page }) => {
		await openChannel(page)
		await expect.poll(() => subscriptions(page)).toHaveLength(1)
		const [first] = await subscriptions(page)
		expect(first.topic).toBe('system')
		expect(first.open).toBe(true)
	})

	// Hiding the page closes the stream, which is the unsubscribe; showing it opens a fresh one.
	test('hiding the page unsubscribes and showing it subscribes again', async ({ page }) => {
		await openChannel(page)
		await expect.poll(() => subscriptions(page)).toHaveLength(1)

		await page.evaluate(() => {
			Object.defineProperty(document, 'hidden', { value: true, configurable: true })
			document.dispatchEvent(new Event('visibilitychange'))
		})
		await expect.poll(async () => (await subscriptions(page))[0].open).toBe(false)

		await page.evaluate(() => {
			Object.defineProperty(document, 'hidden', { value: false, configurable: true })
			document.dispatchEvent(new Event('visibilitychange'))
		})
		await expect.poll(() => subscriptions(page)).toHaveLength(2)
		await expect.poll(async () => (await subscriptions(page))[1].open).toBe(true)
	})

	// Samples already queued when the stream ends may still arrive, and are discarded rather than
	// treated as a fault (BLI-MSG, "Subscribing").
	test('data in flight after unsubscribing is discarded rather than faulting', async ({ page }) => {
		await openChannel(page)
		await emit(page, hello)
		await expect.poll(() => subscriptions(page)).toHaveLength(1)

		await page.evaluate(() => {
			Object.defineProperty(document, 'hidden', { value: true, configurable: true })
			document.dispatchEvent(new Event('visibilitychange'))
		})
		await expect.poll(async () => (await subscriptions(page))[0].open).toBe(false)

		// The device had already sent this when the stream closed.
		await page.evaluate(() =>
			window.__blitiSubscriptions[0].events({
				kind: 'message',
				message: { type: 'identity', hostname: 'late', addresses: [] },
			}),
		)

		await expect(page.locator('.notice')).toHaveCount(0)
		await expect(page.getByText('bliti 0.4.2')).toBeVisible()
	})
})
