// The label sheet in the page (PWEB, PSHT): labels added from a finished run, the list, and the
// sheet downloaded from it.

import { readFile } from 'node:fs/promises'

import { expect, test } from '@playwright/test'

import { emitState, openChannel, openProvision, stateOf, step } from './fake-provision-client.js'

// Four payloads of real QR codes, so the sheet can be drawn from them.
const PAYLOADS = [
	'BLITI:AF7O2MIUBIUGCVVXQ7AOWQ7UZMZRTDRQQXBJWX4VODYKEKPUVUJEHZVU',
	'BLITI:AFTYS4WBYGHYQXSUDI22XLS7IH3UOSOPZZ3LV6GN5AROIB2PHYFZXBAG',
	'BLITI:AF4DYHAHWPDVY3HT2CHMKU65JMKW5IL6MGNBJPILAUCBD27KCYEPALUL',
	'BLITI:AH3QD4IHUVGW35UWWJBCRKLLT52FFGXISJU7RWBBUI3YP6JUUICZAXBT',
]
const [ONE, TWO] = PAYLOADS

const STORAGE = 'bliti.provision.labels'

/// A finished run whose step `label` made `payload` at `at`.
const finished = (payload, at = '2026-10-31T01:00:00Z', version = '4.0') =>
	stateOf({
		run: 'finished',
		steps: [step('label', { title: 'Make the label', state: 'passed', result: { payload, version, at } })],
	})

const sheet = (page) => page.getByRole('button', { name: /^Download sheet/ })
const link = (page) => page.getByRole('button', { name: /^Labels \(/ })
const kept = (page) => page.evaluate((key) => JSON.parse(localStorage.getItem(key)), STORAGE)

/// The labels a browser holds, seeded before the page loads where it holds none.
async function seed(page, labels) {
	await page.addInitScript(
		([key, labels]) => {
			if (localStorage.getItem(key) === null) {
				const seen = Object.fromEntries(labels.map((label) => [label.payload.slice(-4), { at: label.at, until: null }]))
				localStorage.setItem(key, JSON.stringify({ labels, seen }))
			}
		},
		[STORAGE, labels],
	)
}

const held = (payload, extra = {}) => ({ payload, version: '4.0', at: '2026-10-30T01:00:00Z', printed: null, ...extra })

test('a finished run adds its label to the list and says so, without showing its payload', async ({ page }) => {
	await openChannel(page, finished(ONE))
	await expect(page.getByText('Its label is in your list.')).toBeVisible()
	await expect(link(page)).toHaveText('Labels (1 not printed)')
	await expect(page.getByText(ONE)).toHaveCount(0)
	await expect(page.getByText('HZVU')).toHaveCount(0)
	expect((await kept(page)).labels.map((label) => label.payload)).toEqual([ONE])
})

test('the sheet is the primary action of the finished screen while a label is unprinted', async ({ page }) => {
	await openChannel(page, finished(ONE))
	await expect(sheet(page)).toHaveText('Download sheet (4% filled)')
	await expect(sheet(page)).not.toHaveClass(/secondary/)
	await expect(page.getByRole('button', { name: 'Provision another Iti' })).not.toHaveClass(/go/)
})

test('downloading from the finished screen gives a PDF and marks the label printed', async ({ page }) => {
	await openChannel(page, finished(ONE))
	const [download] = await Promise.all([page.waitForEvent('download'), sheet(page).click()])
	expect(download.suggestedFilename()).toMatch(/^iti-labels-\d{4}-\d{2}-\d{2}\.pdf$/)
	const bytes = await readFile(await download.path())
	expect(bytes.subarray(0, 5).toString()).toBe('%PDF-')
	expect(bytes.toString('latin1').match(/\/Type \/Page\n/g)).toHaveLength(1)
	await expect(link(page)).toHaveText('Labels (0 not printed)')
	await expect(sheet(page)).toHaveCount(0)
	await expect(page.getByRole('button', { name: 'Provision another Iti' })).toHaveClass(/go/)
	expect((await kept(page)).labels[0].printed).not.toBeNull()
})

test('seeing a result again adds nothing, whether the label is unprinted, printed or removed', async ({ page }) => {
	await openChannel(page, finished(ONE))
	await emitState(page, finished(ONE))
	await expect(link(page)).toHaveText('Labels (1 not printed)')
	expect((await kept(page)).labels).toHaveLength(1)

	await Promise.all([page.waitForEvent('download'), sheet(page).click()])
	await emitState(page, finished(ONE))
	await expect(link(page)).toHaveText('Labels (0 not printed)')

	await link(page).click()
	await page.getByRole('button', { name: 'Remove' }).click()
	await expect(page.getByText('No labels yet.')).toBeVisible()
	await page.getByRole('button', { name: 'Back' }).click()
	await expect(link(page)).toHaveCount(0)
	await emitState(page, finished(ONE))
	await expect(link(page)).toHaveCount(0)
	expect((await kept(page)).labels).toEqual([])
})

test('a later result for the same board replaces its label and leaves it unprinted', async ({ page }) => {
	await seed(page, [held(ONE, { at: '2026-10-30T01:00:00Z', printed: '2026-10-30T02:00:00.000Z' })])
	await openChannel(page, finished(ONE, '2026-10-31T01:00:00Z'))
	await expect(link(page)).toHaveText('Labels (1 not printed)')
	const labels = (await kept(page)).labels
	expect(labels).toHaveLength(1)
	expect(labels[0]).toMatchObject({ at: '2026-10-31T01:00:00Z', printed: null })
})

test('the list is offered while finding a device, with how many are unprinted', async ({ page }) => {
	await seed(page, [held(ONE), held(TWO, { printed: '2026-10-30T02:00:00.000Z' })])
	await openProvision(page)
	await expect(link(page)).toHaveText('Labels (1 not printed)')
	await link(page).click()
	await expect(page.getByRole('heading', { name: 'Labels' })).toBeVisible()
	await page.getByRole('button', { name: 'Back' }).click()
	await expect(page.getByRole('button', { name: 'Find Iti' })).toBeVisible()
})

test('nothing is offered while finding a device and the list is empty', async ({ page }) => {
	await openProvision(page)
	await expect(page.getByRole('button', { name: 'Find Iti' })).toBeVisible()
	await expect(link(page)).toHaveCount(0)
})

test('the list shows each label, ticks the unprinted and leaves the printed unticked', async ({ page }) => {
	await seed(page, [
		held(ONE, { at: '2026-10-31T01:00:00Z' }),
		held(TWO, { at: '2026-10-30T01:00:00Z', printed: '2026-10-30T02:00:00.000Z' }),
	])
	await openProvision(page)
	await link(page).click()
	const rows = page.locator('.labels li')
	await expect(rows).toHaveCount(2)
	await expect(rows.nth(0)).toContainText('HZVU')
	await expect(rows.nth(0)).toContainText('Tamanu Iti v4.0')
	await expect(rows.nth(0)).toContainText('Added')
	await expect(rows.nth(1)).toContainText('XBAG')
	await expect(rows.nth(1)).toContainText('Printed')
	await expect(page.getByRole('checkbox', { name: 'Print HZVU' })).toBeChecked()
	await expect(page.getByRole('checkbox', { name: 'Print XBAG' })).not.toBeChecked()
	await expect(sheet(page)).toHaveText('Download sheet (4% filled)')
	await page.getByRole('checkbox', { name: 'Print XBAG' }).check()
	await expect(sheet(page)).toHaveText('Download sheet (7% filled)')
})

test('a printed label is printed again by ticking it, and no download is offered with none ticked', async ({ page }) => {
	await seed(page, [held(ONE, { printed: '2026-10-30T02:00:00.000Z' })])
	await openProvision(page)
	await link(page).click()
	await expect(sheet(page)).toBeDisabled()
	await page.getByRole('checkbox', { name: 'Print HZVU' }).check()
	await expect(sheet(page)).toBeEnabled()
	const [download] = await Promise.all([page.waitForEvent('download'), sheet(page).click()])
	expect((await readFile(await download.path())).subarray(0, 5).toString()).toBe('%PDF-')
	expect((await kept(page)).labels[0].printed).not.toBe('2026-10-30T02:00:00.000Z')
})

test('says how full the sheet is for one page, several pages, and pages with none to spare', async ({ page }) => {
	const many = (count) =>
		Array.from({ length: count }, (_, index) =>
			held(`BLITI:${String(index).padStart(52, '0')}${String(index).padStart(4, 'A').slice(-4)}`, {
				at: `2026-10-30T01:${String(index % 60).padStart(2, '0')}:00Z`,
			}),
		)
	await seed(page, many(46))
	await openProvision(page)
	await link(page).click()
	await expect(sheet(page)).toHaveText('Download sheet (2 sheets, last 70% filled)')
	const boxes = page.getByRole('checkbox')
	for (let at = 0; at < 19; at++) await boxes.nth(at).uncheck()
	await expect(sheet(page)).toHaveText('Download sheet (1 sheet)')
	await boxes.nth(0).check()
	await expect(sheet(page)).toHaveText('Download sheet (2 sheets, last 4% filled)')
})

test('removing an unprinted label asks first, and a printed one goes at once', async ({ page }) => {
	await seed(page, [held(ONE), held(TWO, { printed: '2026-10-30T02:00:00.000Z' })])
	await openProvision(page)
	await link(page).click()
	const rows = page.locator('.labels li')

	await rows.filter({ hasText: 'XBAG' }).getByRole('button', { name: 'Remove' }).click()
	await expect(rows).toHaveCount(1)

	await page.getByRole('button', { name: 'Remove' }).click()
	await expect(page.getByText('Remove this label before it has been printed?')).toBeVisible()
	await page.getByRole('button', { name: 'Keep' }).click()
	await expect(rows).toHaveCount(1)
	await page.getByRole('button', { name: 'Remove' }).click()
	await page.getByRole('button', { name: 'Remove' }).click()
	await expect(page.getByText('No labels yet.')).toBeVisible()
	expect((await kept(page)).labels).toEqual([])
})

test('every tab of the browser holds the same list', async ({ browser }) => {
	const context = await browser.newContext()
	const first = await context.newPage()
	const second = await context.newPage()
	await openChannel(first, finished(ONE))
	await expect(link(first)).toHaveText('Labels (1 not printed)')

	await openChannel(second, finished(TWO, '2026-10-31T02:00:00Z'))
	await expect(link(second)).toHaveText('Labels (2 not printed)')
	// The first tab learns of the second's label without a reload.
	await expect(link(first)).toHaveText('Labels (2 not printed)')
	expect((await kept(second)).labels.map((label) => label.payload).sort()).toEqual([ONE, TWO].sort())
	await context.close()
})

test('says the labels are lost with the page where storage cannot keep them', async ({ page }) => {
	await page.addInitScript(() => {
		Object.defineProperty(window, 'localStorage', {
			get() {
				throw new DOMException('denied', 'SecurityError')
			},
		})
	})
	await openChannel(page, finished(ONE))
	await expect(link(page)).toHaveText('Labels (1 not printed)')
	await link(page).click()
	await expect(page.getByText("This browser can't keep the list. The labels are lost when the page is closed.")).toBeVisible()
})
