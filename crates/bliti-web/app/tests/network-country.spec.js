// Suggesting the country from what a scan heard access points name (NSCR).

import { expect, test } from '@playwright/test'

import { answer, message, openNetwork, sent } from './fake-client.js'
import { IN_FORCE, PI, TWO_RADIOS, addWireless, ap, bar } from './network-fixtures.js'
import { suggestedCountries } from '../src/countries.js'

const { 'regulatory-domain': _, ...UNSET } = IN_FORCE

const country = (page) => page.locator('section').filter({ has: page.getByRole('heading', { name: 'Country' }) })

const named = (bssid, code) => ap({ bssid, ssid: 'Clinic', country: code })

// Three access points naming Vanuatu, one the United States, and one naming none.
const HEARD = [
	named('a4:2b:b0:11:2c:40', 'VU'),
	named('a4:2b:b0:11:2c:41', 'VU'),
	named('a4:2b:b0:11:2c:42', 'VU'),
	named('5c:a6:e6:02:71:9b', 'US'),
	ap({ bssid: '5c:a6:e6:02:71:9c', ssid: 'Guest' }),
]

async function scanned(page, { document = UNSET, capabilities = PI, heard = HEARD } = {}) {
	await openNetwork(page, { document, capabilities })
	await answer(page, 'scan', message({ type: 'networks', 'access-points': heard }))
	await country(page).getByRole('button', { name: 'Scan', exact: true }).click()
}

test('the country scans every adapter', async ({ page }) => {
	await scanned(page, { capabilities: TWO_RADIOS })
	expect((await sent(page)).filter((each) => each.type === 'scan')).toEqual([{ type: 'scan' }])
})

test('unset, the country most access points name is the primary action, and sets the field only', async ({ page }) => {
	await scanned(page)
	const section = country(page)
	await expect(section).toContainText('Nearby access points say Vanuatu.')
	const use = section.getByRole('button', { name: 'Use Vanuatu' })
	await expect(use).not.toHaveClass(/secondary|link/)

	await use.click()
	await expect(page.getByLabel('Country')).toHaveValue('VU')
	await expect(bar(page)).toContainText('1 change not applied.')
	expect((await sent(page)).map((each) => each.type)).toEqual(['configure', 'scan'])
})

test('every other country heard is offered beside the suggestion', async ({ page }) => {
	await scanned(page)
	const section = country(page)
	await expect(section.locator('.also')).toHaveText('Also heard: United States')
	await section.getByRole('button', { name: 'United States', exact: true }).click()
	await expect(page.getByLabel('Country')).toHaveValue('US')
})

test('a tie suggests each tied country, none above the others', async ({ page }) => {
	await scanned(page, { heard: [named('02:00:00:00:00:01', 'FJ'), named('02:00:00:00:00:02', 'TO')] })
	const section = country(page)
	await expect(section).toContainText('Nearby access points disagree.')
	const buttons = section.locator('.suggest .row button')
	await expect(buttons).toHaveText(['Use Fiji', 'Use Tonga'])
	for (const button of await buttons.all()) await expect(button).not.toHaveClass(/secondary|link/)
})

test('set to another country, the suggestion is a line beneath the field', async ({ page }) => {
	await scanned(page, { document: { ...IN_FORCE, 'regulatory-domain': 'NZ' } })
	const section = country(page)
	await expect(section.locator('.suggest')).toHaveCount(0)
	await expect(section.locator('.aside')).toContainText('Nearby access points say Vanuatu.')
	await section.locator('.aside').getByRole('button', { name: 'Use Vanuatu' }).click()
	await expect(page.getByLabel('Country')).toHaveValue('VU')
})

test('set to a country heard but not suggested, it is not offered again', async ({ page }) => {
	await scanned(page, { document: { ...IN_FORCE, 'regulatory-domain': 'US' } })
	const section = country(page)
	await expect(section.locator('.aside')).toContainText('Nearby access points say Vanuatu.')
	await expect(section.locator('.also')).toHaveCount(0)
})

test('set to the country suggested, nothing is said of it', async ({ page }) => {
	await scanned(page, { document: IN_FORCE })
	const section = country(page)
	await expect(section.locator('.suggest, .aside, .also')).toHaveCount(0)
	await expect(section.getByRole('button', { name: /^Use / })).toHaveCount(0)
})

test('a scan hearing no country the device offers says so', async ({ page }) => {
	const fijiOnly = { ...PI, document: { ...PI.document, 'regulatory-domain': ['FJ'] } }
	await scanned(page, { capabilities: fijiOnly })
	await expect(country(page).locator('.aside')).toHaveText('No nearby access point names a country.')
})

test('a scan from a wireless connection suggests the country too', async ({ page }) => {
	await openNetwork(page, { document: UNSET, capabilities: PI })
	await answer(page, 'scan', message({ type: 'networks', 'access-points': HEARD }))
	await addWireless(page)
	await page.locator('.candidate').getByRole('button', { name: 'Scan', exact: true }).click()
	await expect(country(page).getByRole('button', { name: 'Use Vanuatu' })).toBeVisible()
})

test('nothing is suggested before a scan', async ({ page }) => {
	await openNetwork(page, { document: UNSET, capabilities: PI })
	const section = country(page)
	await expect(section.getByRole('button', { name: 'Scan', exact: true })).toBeVisible()
	await expect(section.locator('.suggest, .aside')).toHaveCount(0)
})

test('a device that cannot scan offers no scan beside the country', async ({ page }) => {
	await openNetwork(page, { document: UNSET, capabilities: { ...PI, acts: {} } })
	await expect(country(page).getByRole('button', { name: 'Scan' })).toHaveCount(0)
})

test.describe('the countries a scan heard named', () => {
	test('an access point heard by two radios counts once', () => {
		const heard = [
			named('02:00:00:00:00:01', 'VU'),
			{ ...named('02:00:00:00:00:01', 'VU'), interface: 'wlx00c0caa1b2c3' },
			named('02:00:00:00:00:02', 'FJ'),
			named('02:00:00:00:00:03', 'FJ'),
		]
		expect(suggestedCountries(heard, 'any')).toEqual({ suggested: ['FJ'], others: ['VU'] })
	})

	test('only countries the device offers are counted', () => {
		const heard = [named('02:00:00:00:00:01', 'XX'), named('02:00:00:00:00:02', 'EU'), named('02:00:00:00:00:03', 'vu')]
		expect(suggestedCountries(heard, 'any')).toEqual({ suggested: ['VU'], others: [] })
		expect(suggestedCountries(heard, ['FJ'])).toEqual({ suggested: [], others: [] })
	})

	test('others come most named first', () => {
		const heard = [
			named('02:00:00:00:00:01', 'VU'),
			named('02:00:00:00:00:02', 'VU'),
			named('02:00:00:00:00:03', 'VU'),
			named('02:00:00:00:00:04', 'US'),
			named('02:00:00:00:00:05', 'FJ'),
			named('02:00:00:00:00:06', 'FJ'),
		]
		expect(suggestedCountries(heard, 'any')).toEqual({ suggested: ['VU'], others: ['FJ', 'US'] })
	})
})
