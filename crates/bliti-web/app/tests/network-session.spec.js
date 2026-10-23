// The session as the device speaks it (CFG, NSCR): states, acts that cannot be performed,
// capabilities carried along, and what waits on the device, through a fake device session.

import { expect, test } from '@playwright/test'

import { answer, echo, message, openChannel, openNetwork, openNetworkScreen, refusing, say, sent, stateOf } from './fake-client.js'
import { IN_FORCE, INDEPENDENT, PI, STATES, addWireless, ap, bar, row } from './network-fixtures.js'

test.describe('the session as the device speaks it', () => {
	test('the state of each candidate is read as the device sends it, checking included', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await say(page, stateOf({
			attachments: [
				{ is: 'verifying' },
				{ is: 'unavailable', reached: 'gateway', reason: '192.168.60.1 did not answer' },
				{ is: 'standby' },
				{ is: 'hibernating' },
				{ is: 'default-route' },
			],
		}))
		await expect(row(page, 'Clinic wall port').locator('.state')).toHaveText('Checking')
		await expect(row(page, 'North site').locator('.state')).toHaveText('No gateway')
		await expect(row(page, 'eth0 automatic').locator('.state')).toHaveText('Standby')
		// A state this build does not know is left unsaid, and the rest still line up.
		await expect(row(page, 'Clinic-Staff').locator('.state')).toHaveCount(0)
		await expect(row(page, 'BackupLink').locator('.state')).toHaveText('Default route')
	})

	// What went wrong before an apply is not left standing beside it.
	test('a new apply clears what the last scan failed with', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await answer(page, 'scan', message({ type: 'cannot-act', act: 'scan', at: '$', reason: 'the radio is busy with the hotspot' }))
		await page.getByRole('button', { name: 'Scan', exact: true }).click()
		const said = page.locator('.network .why', { hasText: 'the radio is busy with the hotspot' })
		await expect(said).toHaveCount(1)

		await page.getByLabel('Country').selectOption('FJ')
		await page.getByRole('button', { name: 'Apply' }).click()
		await expect(bar(page)).toHaveAttribute('data-stage', 'applying')
		await expect(said).toHaveCount(0)
	})

	// An act's failure is its own: it neither ends the stage nor reads as a proposal's failure.
	test('an act the device cannot perform is named by its act and does not touch the stage', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: INDEPENDENT })
		await answer(page, 'survey', message({ type: 'cannot-act', act: 'survey', at: "$['interface']", reason: 'wlan0 cannot survey while it scans' }))
		const hotspot = page.locator('.hotspot')
		await hotspot.getByText('Radio and addressing').click()
		await hotspot.getByRole('button', { name: 'Survey the spectrum' }).click()
		await expect(hotspot.getByText('wlan0 cannot survey while it scans')).toBeVisible()
		await expect(hotspot.getByRole('button', { name: 'Survey the spectrum' })).toBeEnabled()
		await expect(bar(page)).toHaveAttribute('data-stage', 'editing')

		// A refusal of the other act is not this one's.
		await say(page, message({ type: 'cannot-act', act: 'scan', at: '$', reason: 'not this one' }))
		await expect(page.locator('.network').getByText('not this one')).toHaveCount(0)
	})

	test('the states are not shown until the device has reported those of the configuration it saved', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI, states: STATES })
		await expect(page.locator('.order .state')).toHaveCount(5)
		await answer(page, 'configuration', echo())
		await page.getByLabel('Country').selectOption('FJ')
		await page.getByRole('button', { name: 'Apply' }).click()
		await expect(bar(page)).toHaveAttribute('data-stage', 'applying')
		await expect(page.locator('.order .state')).toHaveCount(0)

		await say(page, stateOf({ outcome: 'verifying', attachments: [{ is: 'verifying' }, { is: 'standby' }, { is: 'standby' }, { is: 'standby' }, { is: 'standby' }] }))
		await expect(row(page, 'North site').locator('.state')).toHaveText('Standby')
		await expect(row(page, 'Clinic wall port').locator('.state')).toHaveText('Checking')

		await say(page, stateOf({ attachments: [{ is: 'up' }, { is: 'default-route' }, { is: 'standby' }, { is: 'up' }, { is: 'up' }] }))
		await expect(row(page, 'North site').locator('.state')).toHaveText('Default route')
	})

	// CFG: capabilities a proposal changed come on the next `configuration` or `state`, and the next
	// proposal is checked against the latest.
	test('capabilities are refreshed from the configuration sent back, and from a later state', async ({ page }) => {
		const narrowed = structuredClone(INDEPENDENT)
		narrowed.document.hotspot.interface.wlan0.band['5ghz'].channel = [36, 40]
		await openNetwork(page, { document: IN_FORCE, capabilities: INDEPENDENT })
		await answer(page, 'configuration', [echo(narrowed), stateOf({ outcome: 'applied' })])
		const hotspot = page.locator('.hotspot')
		await hotspot.getByText('Radio and addressing').click()
		await hotspot.getByLabel('Band').selectOption('5ghz')
		await expect(hotspot.getByLabel('Channel').locator('option')).toHaveText(['Device picks', '36', '40', '44', '48'])

		await page.getByLabel('Country').selectOption('FJ')
		await page.getByRole('button', { name: 'Apply' }).click()
		await expect(bar(page)).toContainText('Applied.')
		await expect(hotspot.getByLabel('Channel').locator('option')).toHaveText(['Device picks', '36', '40'])

		await say(page, stateOf({ capabilities: INDEPENDENT }))
		await expect(hotspot.getByLabel('Channel').locator('option')).toHaveText(['Device picks', '36', '40', '44', '48'])
	})

	// CFG: each proposal is read by the configuration the device sends after it, so one saved after
	// another was refused is not taken for the refused one.
	test('a proposal is judged by its own echo, whatever came before it', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await answer(page, 'configuration', refusing("$['regulatory-domain']", 'no', IN_FORCE), [echo(), stateOf({ outcome: 'verifying' })])
		await page.getByLabel('Country').selectOption('FJ')
		await page.getByRole('button', { name: 'Apply' }).click()
		await expect(bar(page)).toHaveAttribute('data-stage', 'errored')
		await page.getByLabel('Country').selectOption('WS')
		await page.getByRole('button', { name: 'Apply' }).click()
		await expect(bar(page)).toHaveAttribute('data-stage', 'applying')
		await expect(page.locator('.notice.fault')).toHaveCount(0)
		await say(page, stateOf({ outcome: 'applied' }))
		await expect(bar(page)).toHaveAttribute('data-stage', 'editing')
		await expect(page.getByLabel('Country')).toHaveValue('WS')
		expect((await sent(page)).map((each) => each.type)).toEqual(['configure', 'configuration', 'configuration'])
	})
})

// NSCR: whatever waits on the device says so, with the spinner, until the device answers.
test.describe('waiting on the device', () => {
	const spinning = (locator) => locator.locator('.spinner')

	test('opening a session shows it is waiting until the device answers', async ({ page }) => {
		await openChannel(page)
		await openNetworkScreen(page)
		const asking = page.getByText('Asking the device for its network settings.')
		await expect(asking).toBeVisible()
		await expect(spinning(page.locator('.network'))).toHaveCount(1)
		await say(page, message({ type: 'configuration', document: IN_FORCE, capabilities: PI }))
		await expect(asking).toBeVisible()
		await say(page, stateOf())
		await expect(page.getByRole('heading', { name: 'Connections' })).toBeVisible()
		await expect(asking).toHaveCount(0)
	})

	test('applying shows it is waiting until the device answers', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await answer(page, 'configuration', echo())
		await page.getByLabel('Country').selectOption('FJ')
		await page.getByRole('button', { name: 'Apply' }).click()
		await expect(bar(page)).toContainText('Applying…')
		await expect(spinning(bar(page))).toHaveCount(1)
		await say(page, stateOf({ outcome: 'verifying' }))
		await expect(spinning(bar(page))).toHaveCount(1)
		await say(page, stateOf({ outcome: 'applied' }))
		await expect(spinning(bar(page))).toHaveCount(0)
	})

	test('scanning shows it is waiting until the device answers', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await addWireless(page)
		await page.locator('.candidate').getByRole('button', { name: 'Scan', exact: true }).click()
		const scanning = page.locator('.candidate').getByRole('button', { name: 'Scanning' })
		await expect(scanning).toBeDisabled()
		await expect(spinning(scanning)).toHaveCount(1)
		await say(page, message({ type: 'networks', 'access-points': [ap({ bssid: 'a4:2b:b0:11:2c:40', ssid: 'Clinic', signal: -52 })] }))
		await expect(page.locator('.candidate').getByRole('button', { name: 'Scan', exact: true })).toBeEnabled()
		await expect(page.getByRole('button', { name: 'Clinic', exact: true })).toBeVisible()
	})

	test('surveying shows it is waiting until the device answers', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: INDEPENDENT })
		const hotspot = page.locator('.hotspot')
		await hotspot.getByText('Radio and addressing').click()
		await hotspot.getByRole('button', { name: 'Survey the spectrum' }).click()
		const surveying = hotspot.getByRole('button', { name: 'Surveying' })
		await expect(surveying).toBeDisabled()
		await expect(spinning(surveying)).toHaveCount(1)
		await say(page, message({ type: 'spectrum', spectrum: { channels: [{ interface: 'wlan0', band: '5ghz', channel: 36, networks: 2, busy: 0.4 }] } }))
		await expect(hotspot.getByRole('button', { name: 'Survey the spectrum' })).toBeEnabled()
		await expect(hotspot.locator('.spectrum li')).toHaveText(['36 · 5 GHz2 networks, 40% busy'])
	})
})
