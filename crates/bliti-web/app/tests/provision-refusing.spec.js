// Refusing a medium (PWEB, "Refusing a medium"): a revision outside the range, or an image built 30
// days or more before the phone's own date. Neither shows the steps or sends an act.

import { expect, test } from '@playwright/test'

import { acts, daysAgo, openChannel, stateOf } from './fake-provision-client.js'

// What a refusal never shows or sends: the steps, a way to start, and any act at all.
async function offersNothing(page) {
	await expect(page.getByRole('checkbox')).toHaveCount(0)
	await expect(page.getByRole('button', { name: 'Start' })).toHaveCount(0)
	await expect(page.getByText('Check the hardware')).toHaveCount(0)
	expect(await acts(page)).toEqual([])
}

const WRITE_AGAIN = /Write the latest provisioning image to it/

test('says a revision below the range needs the medium writing again, with no date', async ({ page }) => {
	await page.addInitScript(() => (window.__blitiProvisionLimits = { revisions: [2, 3], lifetime: 30 }))
	await openChannel(page, stateOf({ revision: 1 }))
	await expect(page.getByText('This USB stick is out of date')).toBeVisible()
	await expect(page.getByText(WRITE_AGAIN)).toBeVisible()
	await expect(page.getByText(/Its image was built on/)).toHaveCount(0)
	await expect(page.getByRole('button', { name: 'Reload' })).toHaveCount(0)
	await offersNothing(page)
})

test('says a revision above the range means the page is out of date, with Reload, whatever the date', async ({
	page,
}) => {
	await page.addInitScript(() => (window.__blitiProvisionLimits = { revisions: [1, 1], lifetime: 30 }))
	await openChannel(page, stateOf({ revision: 2, built: daysAgo(90) }))
	await expect(page.getByText('This page is out of date')).toBeVisible()
	await expect(page.getByText('This USB stick is out of date')).toHaveCount(0)
	await expect(page.getByRole('button', { name: 'Reload' })).toBeVisible()
	await offersNothing(page)

	const navigated = page.waitForEvent('load')
	await page.getByRole('button', { name: 'Reload' }).click()
	await navigated
})

test.describe('the image date, by the phone clock', () => {
	// Fixed before the page loads, so the boundary is exact and the day cannot change under a test.
	test.beforeEach(async ({ page }) => {
		await page.clock.setFixedTime(new Date(2026, 9, 29, 23, 59))
	})

	test('refuses an image built 30 days before today', async ({ page }) => {
		await openChannel(page, stateOf({ built: daysAgo(30, new Date(2026, 9, 29)) }))
		await expect(page.getByText('Its image was built on 29 Sep 2026.')).toBeVisible()
		await offersNothing(page)
	})

	test('accepts an image built 29 days before today, and lists the steps', async ({ page }) => {
		await openChannel(page, stateOf({ built: daysAgo(29, new Date(2026, 9, 29)) }))
		await expect(page.getByText('This USB stick is out of date')).toHaveCount(0)
		await expect(page.getByText('Check the hardware')).toBeVisible()
	})

	test('says the date the image was built, in the words of the phone', async ({ page }) => {
		await page.clock.setFixedTime(new Date(2026, 9, 12, 12))
		await openChannel(page, stateOf({ built: '2026-09-12' }))
		await expect(page.getByText('Its image was built on 12 Sep 2026.')).toBeVisible()
		await expect(page.getByText(WRITE_AGAIN)).toBeVisible()
		await offersNothing(page)
	})

	test('refuses an image from a long time ago', async ({ page }) => {
		await openChannel(page, stateOf({ built: '2025-01-01' }))
		await expect(page.getByText('Its image was built on 1 Jan 2025.')).toBeVisible()
	})
})
