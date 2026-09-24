// Scanning on the network screen (NSCR): networks by SSID, hidden ones on request, joining one by
// WPS, one adapter or all, and the same scan read for siting an access point.

import { expect, test } from '@playwright/test'

import { answer, message, openNetwork, say, sent } from './fake-client.js'
import { INDEPENDENT, IN_FORCE, PI, TWO_RADIOS, addWireless, ap, bar } from './network-fixtures.js'

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
	await expect(taken('5 GHz')).toHaveText(['ch 362 APs, 80 MHz', 'ch 1491 AP, 80 MHz'])
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
