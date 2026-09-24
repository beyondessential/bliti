// Scanning on the network screen (NSCR): networks by SSID, hidden ones on request, joining one by
// WPS, one adapter or all, and the same scan read for siting an access point.

import { expect, test } from '@playwright/test'

import { answer, message, openNetwork, say, sent } from './fake-client.js'
import { INDEPENDENT, IN_FORCE, PI, TWO_RADIOS, addWireless, ap, bar } from './network-fixtures.js'
import { spanned } from '../src/scan.js'

const USB = 'wlx00c0caa1b2c3'

// What the Pi with an adapter hears: Clinic on three access points, one heard by both radios, a
// hidden access point, and a guest network.
const HEARD = [
	ap({ bssid: 'a4:2b:b0:11:2c:40', ssid: 'Clinic', signal: -52 }),
	ap({ interface: USB, bssid: 'a4:2b:b0:11:2c:40', ssid: 'Clinic', signal: -44 }),
	ap({ bssid: 'a4:2b:b0:11:2c:41', ssid: 'Clinic', band: '2ghz', channel: 1, 'channel-width': 20, signal: -47 }),
	ap({ bssid: 'a4:2b:b0:3e:90:e0', ssid: 'Clinic', security: ['psk'], channel: 149, signal: -78 }),
	ap({ bssid: 'a4:2b:b0:11:2c:42', ssid: null, hidden: true, security: ['sae'], signal: -53 }),
	ap({ bssid: '5c:a6:e6:02:71:9b', ssid: 'Guest', security: ['open'], band: '2ghz', channel: 6, 'channel-width': 20, signal: -61 }),
]

async function scanned(page, capabilities = TWO_RADIOS, heard = HEARD) {
	await openNetwork(page, { document: IN_FORCE, capabilities })
	await answer(page, 'scan', message({ type: 'networks', 'access-points': heard }))
	await addWireless(page)
	await page.getByRole('button', { name: 'Scan', exact: true }).click()
}

const networks = (page) => page.locator('.networks > li')

test('networks are listed by SSID, with the strongest signal and how many access points', async ({ page }) => {
	await scanned(page)
	await expect(networks(page)).toHaveCount(2)
	await expect(networks(page).nth(0)).toContainText('Clinic')
	await expect(networks(page).nth(0)).toContainText('-44 dBm')
	// An access point heard by both radios is still one access point.
	await expect(networks(page).nth(0)).toContainText('3 APs')
	await expect(networks(page).nth(1)).toContainText('Guest')

	await page.getByRole('button', { name: 'Access points of Clinic' }).click()
	const points = networks(page).nth(0).locator('.points li')
	await expect(points).toHaveCount(4)
	await expect(points.nth(0)).toContainText('a4:2b:b0:11:2c:40')
	await expect(points.nth(0)).toContainText('MediaTek MT7921AU')
	// One access point advertising less than its network is what a technician needs to find.
	await expect(points.nth(3)).toContainText('ch 149, 5 GHz · WPA2 · -78 dBm')
})

// Where the scan settles whether a network is hidden, the operator is not asked (NSCR).
test('a network picked from the scan is not hidden, and cannot be marked hidden', async ({ page }) => {
	await scanned(page)
	await page.getByRole('button', { name: 'Clinic', exact: true }).click()
	const hidden = page.locator('.candidate').getByRole('checkbox', { name: 'Hidden network' })
	await expect(hidden).not.toBeChecked()
	await expect(hidden).toBeDisabled()
})

// Two access points from one vendor differ only in their last octets, so a BSSID is never cut short.
test('a BSSID is shown whole on a phone', async ({ page }) => {
	await page.setViewportSize({ width: 360, height: 800 })
	await scanned(page)
	await page.getByRole('button', { name: 'Access points of Clinic' }).click()
	const bssids = networks(page).nth(0).locator('.points .code')
	await expect(bssids).toHaveCount(4)
	for (const bssid of await bssids.all()) {
		expect(await bssid.evaluate((element) => element.scrollWidth <= element.clientWidth)).toBe(true)
	}
})

test('an access point with no SSID is left out until hidden networks are shown', async ({ page }) => {
	await scanned(page)
	await expect(page.getByRole('button', { name: 'Hidden network', exact: true })).toHaveCount(0)
	await page.getByLabel('Show hidden (1)').check()
	await expect(networks(page)).toHaveCount(3)

	await page.getByRole('button', { name: 'Hidden network', exact: true }).click()
	const candidate = page.locator('.candidate')
	// Its name is not in its beacons, so it is left to type; its security is filled in.
	await expect(candidate.getByLabel('SSID')).toHaveValue('')
	await expect(candidate.getByLabel('Security')).toHaveValue('sae')
	await expect(candidate.getByRole('checkbox', { name: 'Hidden network' })).toBeChecked()
	await expect(candidate.getByRole('checkbox', { name: 'Hidden network' })).toBeDisabled()
})

test('a scan goes to one adapter where one is picked, and to all otherwise', async ({ page }) => {
	await scanned(page)
	await answer(page, 'scan', message({ type: 'networks', 'access-points': HEARD.filter((each) => each.interface === USB) }))
	await page.getByLabel('Scan with').selectOption(USB)
	await page.getByRole('button', { name: 'Scan', exact: true }).click()
	const scans = (await sent(page)).filter((each) => each.type === 'scan')
	expect(scans).toEqual([{ type: 'scan' }, { type: 'scan', interface: USB }])
	await expect(networks(page)).toHaveCount(1)
})

test('one radio offers no adapter to scan with', async ({ page }) => {
	await scanned(page, PI, HEARD.filter((each) => each.interface === 'wlan0'))
	await expect(page.getByLabel('Scan with')).toHaveCount(0)
})

test('the siting view lists every access point by signal, and the channels taken on each band', async ({ page }) => {
	await scanned(page)
	await page.getByRole('button', { name: 'Siting' }).click()
	const points = page.locator('.siting > .points li')
	await expect(points).toHaveCount(HEARD.length)
	await expect(points.nth(0)).toContainText('-44 dBm')
	await expect(points.nth(0)).toContainText('Clinic')
	await expect(points.nth(0)).toContainText('ch 36, 5 GHz · 80 MHz · MediaTek MT7921AU')
	// Hidden access points take a channel like any other.
	await expect(points.nth(3)).toContainText('Hidden network')
	await expect(points.nth(5)).toContainText('-78 dBm')

	const taken = (band) => page.getByLabel(`Channels taken on ${band}`).locator('li')
	await expect(taken('2.4 GHz')).toHaveText(['ch 11 AP, 20 MHz', 'ch 61 AP, 20 MHz'])
	// An 80 MHz access point takes the three channels bonded with its primary as well.
	await expect(taken('5 GHz')).toHaveText([
		'ch 362 APs, 80 MHz',
		'ch 402 APs, 80 MHz',
		'ch 442 APs, 80 MHz',
		'ch 482 APs, 80 MHz',
		'ch 1491 AP, 80 MHz',
		'ch 1531 AP, 80 MHz',
		'ch 1571 AP, 80 MHz',
		'ch 1611 AP, 80 MHz',
	])
	// The adapter can use 6 GHz, and nothing is heard there.
	await expect(taken('6 GHz')).toHaveText(['None heard.'])
})

// NSCR: a network the scan lists by name is joined by WPS for that network alone.
test('a network in the scan list is joined by WPS for it alone', async ({ page }) => {
	await scanned(page, PI, HEARD.filter((each) => each.interface === 'wlan0'))
	const clinic = networks(page).filter({ hasText: 'Clinic' })
	await clinic.getByRole('button', { name: 'Join Clinic by WPS' }).click()
	await clinic.getByRole('button', { name: 'WPS PIN' }).click()
	expect((await sent(page)).at(-1)).toEqual({ type: 'wps', method: 'pin', ssid: 'Clinic' })
	await expect(bar(page)).toContainText('Joining Clinic by WPS.')
})

test('a network the device cannot join is not offered WPS', async ({ page }) => {
	await scanned(page, PI, HEARD.filter((each) => each.interface === 'wlan0'))
	await expect(networks(page).filter({ hasText: 'Guest' })).toContainText('Cannot join')
	await expect(page.getByRole('button', { name: 'Join Guest by WPS' })).toHaveCount(0)
})

// CFG: credentials for another network are refused at the act's `ssid`, and the reason is the
// device's own.
test('a WPS join that hands over another network gives the reason the device wrote', async ({ page }) => {
	await scanned(page, PI, HEARD.filter((each) => each.interface === 'wlan0'))
	const clinic = networks(page).filter({ hasText: 'Clinic' })
	await clinic.getByRole('button', { name: 'Join Clinic by WPS' }).click()
	await clinic.getByRole('button', { name: 'WPS button' }).click()
	const reason = 'the access point handed over credentials for "Office", not "Clinic", and they were discarded'
	await say(page, message({ type: 'invalid', at: "$['ssid']", reason }))
	await expect(bar(page)).toHaveAttribute('data-stage', 'editing')
	await expect(page.getByText('Could not join Clinic by WPS.')).toBeVisible()
	await expect(page.locator('.network .reason')).toHaveText(reason)
})

test('WPS is offered from the scan only where the device joins a named network that way', async ({ page }) => {
	await scanned(page, INDEPENDENT, HEARD.filter((each) => each.interface === 'wlan0'))
	await expect(networks(page).first()).toContainText('Clinic')
	await expect(page.getByRole('button', { name: 'Join Clinic by WPS' })).toHaveCount(0)
})

// The adapter the candidate is pinned to is the one joined on, and one that cannot join a named
// network by WPS is not offered it.
test('WPS from the scan joins on the adapter the candidate is pinned to', async ({ page }) => {
	await scanned(page)
	const candidate = page.locator('.candidate')
	await candidate.getByLabel('Adapter').selectOption(USB)
	await expect(page.getByRole('button', { name: 'Join Clinic by WPS' })).toHaveCount(0)

	await candidate.getByLabel('Adapter').selectOption('wlan0')
	const clinic = networks(page).filter({ hasText: 'Clinic' })
	await clinic.getByRole('button', { name: 'Join Clinic by WPS' }).click()
	await clinic.getByRole('button', { name: 'WPS button' }).click()
	expect((await sent(page)).at(-1)).toEqual({ type: 'wps', method: 'push-button', interface: 'wlan0', ssid: 'Clinic' })
})

// NSCR: the channels taken count every channel a wide access point spans, not its primary alone.
test('the siting view counts every channel a wide access point spans', async ({ page }) => {
	const heard = [
		ap({ bssid: 'a4:2b:b0:11:2c:40', ssid: 'Clinic', band: '2ghz', channel: 6, 'channel-width': 40, 'secondary-channel': 2, signal: -50 }),
		ap({ bssid: 'a4:2b:b0:11:2c:41', ssid: 'Clinic', band: '2ghz', channel: 1, 'channel-width': 20, signal: -60 }),
		ap({ bssid: 'a4:2b:b0:11:2c:42', ssid: 'Clinic', channel: 60, 'channel-width': 40, signal: -55 }),
		ap({ bssid: 'a4:2b:b0:11:2c:43', ssid: 'Clinic', channel: 64, 'channel-width': 20, signal: -65 }),
	]
	await scanned(page, PI, heard)
	await page.getByRole('button', { name: 'Siting' }).click()
	const taken = (band) => page.getByLabel(`Channels taken on ${band}`).locator('li')
	await expect(taken('2.4 GHz')).toHaveText(['ch 11 AP, 20 MHz', 'ch 21 AP, 40 MHz', 'ch 61 AP, 40 MHz'])
	await expect(taken('5 GHz')).toHaveText(['ch 601 AP, 40 MHz', 'ch 642 APs, 40 MHz'])
})

test.describe('the channels an access point spans', () => {
	const at = (band, channel, width, more = {}) => spanned({ band, channel, 'channel-width': width, ...more })

	test('a 20 MHz access point takes its primary alone', () => {
		expect(at('5ghz', 44, 20)).toEqual([44])
		expect(at('2ghz', 6, 20)).toEqual([6])
	})

	test('on 2.4 GHz a wide one takes the secondary channel it names', () => {
		expect(at('2ghz', 6, 40, { 'secondary-channel': 2 })).toEqual([2, 6])
		expect(at('2ghz', 1, 40, { 'secondary-channel': 5 })).toEqual([1, 5])
		// Where it does not say which side, only its primary is known to be taken.
		expect(at('2ghz', 6, 40)).toEqual([6])
	})

	test('on 5 GHz a wide one takes its bonded group', () => {
		expect(at('5ghz', 40, 40)).toEqual([36, 40])
		expect(at('5ghz', 144, 40)).toEqual([140, 144])
		expect(at('5ghz', 157, 40)).toEqual([157, 161])
		expect(at('5ghz', 44, 80)).toEqual([36, 40, 44, 48])
		expect(at('5ghz', 60, 80)).toEqual([52, 56, 60, 64])
		expect(at('5ghz', 132, 80)).toEqual([132, 136, 140, 144])
		expect(at('5ghz', 161, 80)).toEqual([149, 153, 157, 161])
		expect(at('5ghz', 52, 160)).toEqual([36, 40, 44, 48, 52, 56, 60, 64])
		expect(at('5ghz', 116, 160)).toEqual([100, 104, 108, 112, 116, 120, 124, 128])
	})

	test('on 6 GHz a wide one takes its group counted from channel 1', () => {
		expect(at('6ghz', 5, 40)).toEqual([1, 5])
		expect(at('6ghz', 37, 80)).toEqual([33, 37, 41, 45])
		expect(at('6ghz', 37, 160)).toEqual([33, 37, 41, 45, 49, 53, 57, 61])
	})
})

// Two radios able to survey, so the survey can go to one.
const SURVEYING = structuredClone(TWO_RADIOS)
SURVEYING.acts.survey = { interface: { wlan0: {}, [USB]: {} } }

// NSCR: a survey goes to one adapter where one is picked, among those able to, and to all otherwise.
test('a survey goes to one adapter where one is picked, and to all otherwise', async ({ page }) => {
	await openNetwork(page, { document: IN_FORCE, capabilities: SURVEYING })
	const hotspot = page.locator('.hotspot')
	await hotspot.getByText('Radio and addressing').click()
	await expect(hotspot.getByLabel('Survey with').locator('option')).toHaveText(['All adapters', 'Cypress CYW43455', 'MediaTek MT7921AU'])
	const spectrum = message({ type: 'spectrum', spectrum: { channels: [] } })
	await answer(page, 'survey', spectrum, spectrum)
	await hotspot.getByRole('button', { name: 'Survey the spectrum' }).click()
	await expect(hotspot.getByRole('button', { name: 'Survey the spectrum' })).toBeEnabled()
	await hotspot.getByLabel('Survey with').selectOption(USB)
	await hotspot.getByRole('button', { name: 'Survey the spectrum' }).click()
	const surveys = (await sent(page)).filter((each) => each.type === 'survey')
	expect(surveys).toEqual([{ type: 'survey' }, { type: 'survey', interface: USB }])
})

// Only adapters the device surveys with are offered; one alone is no choice.
test('one adapter able to survey offers no choice', async ({ page }) => {
	await openNetwork(page, { document: IN_FORCE, capabilities: TWO_RADIOS })
	const hotspot = page.locator('.hotspot')
	await hotspot.getByText('Radio and addressing').click()
	await expect(hotspot.getByRole('button', { name: 'Survey the spectrum' })).toBeVisible()
	await expect(hotspot.getByLabel('Survey with')).toHaveCount(0)
	await hotspot.getByRole('button', { name: 'Survey the spectrum' }).click()
	expect((await sent(page)).at(-1)).toEqual({ type: 'survey' })
})

// NSCR: WPS joins on a chosen adapter or on one the device picks, offering what that adapter does.
test('WPS joins on a chosen adapter, or on one the device picks', async ({ page }) => {
	await openNetwork(page, { document: IN_FORCE, capabilities: TWO_RADIOS })
	await page.getByRole('button', { name: 'Add' }).click()
	const adding = page.locator('.adding')
	await expect(page.getByLabel('WPS with').locator('option')).toHaveText(['Device chooses', 'Cypress CYW43455', 'MediaTek MT7921AU'])
	await expect(adding.getByRole('button', { name: /^WPS / })).toHaveText(['WPS button', 'WPS PIN'])

	// The adapter joins by push-button alone.
	await page.getByLabel('WPS with').selectOption(USB)
	await expect(adding.getByRole('button', { name: /^WPS / })).toHaveText(['WPS button'])
	await page.getByRole('button', { name: 'WPS button' }).click()
	expect((await sent(page)).at(-1)).toEqual({ type: 'wps', method: 'push-button', interface: USB })
	await bar(page).getByRole('button', { name: 'Cancel' }).click()

	await page.getByRole('button', { name: 'Add' }).click()
	await page.getByLabel('WPS with').selectOption('')
	await page.getByRole('button', { name: 'WPS PIN' }).click()
	expect((await sent(page)).at(-1)).toEqual({ type: 'wps', method: 'pin' })
})

test('one radio offers no adapter to join by WPS with', async ({ page }) => {
	await openNetwork(page, { document: IN_FORCE, capabilities: PI })
	await page.getByRole('button', { name: 'Add' }).click()
	await expect(page.getByRole('button', { name: 'WPS button' })).toBeVisible()
	await expect(page.getByLabel('WPS with')).toHaveCount(0)
})
