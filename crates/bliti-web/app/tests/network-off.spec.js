// Turning a connection or the hotspot off and on again (NSCR): an edit like any other, proposed on
// apply, with its fields kept.

import { expect, test } from '@playwright/test'

import { emit, message, openNetwork, sent } from './fake-client.js'
import { IN_FORCE, PI, STATES, bar, open, proposals, row } from './network-fixtures.js'

const toggle = (page, name) => page.locator('.candidate').getByRole('button', { name, exact: true })
const hotspotButton = (page, name) => page.locator('section.hotspot').getByRole('button', { name, exact: true })

test('a connection turned off keeps its fields, and is proposed off on apply', async ({ page }) => {
	await openNetwork(page, { document: IN_FORCE, capabilities: PI })
	await open(page, 'Clinic-Staff')
	await toggle(page, 'Turn off').click()

	await expect(row(page, 'Clinic-Staff')).toContainText('Wireless, WPA3, off')
	await expect(page.locator('.candidate')).toContainText('Off. Kept, but not used until turned on.')
	await expect(page.locator('.candidate').getByLabel('SSID')).toBeEditable()
	await expect(bar(page)).toContainText('1 change not applied.')
	expect(await sent(page)).toEqual([{ type: 'configure' }])

	await page.getByRole('button', { name: 'Apply' }).click()
	const [proposal] = await proposals(page)
	expect(proposal.document.attachments[3]).toEqual({ ...IN_FORCE.attachments[3], enabled: false })
	expect(proposal.document.attachments.map((each) => each.enabled)).toEqual([true, true, true, false, true])
})

test('a connection turned off is turned on again', async ({ page }) => {
	const document = structuredClone(IN_FORCE)
	document.attachments[4].enabled = false
	await openNetwork(page, { document, capabilities: PI })
	await expect(row(page, 'BackupLink')).toContainText(', off')
	await open(page, 'BackupLink')
	await toggle(page, 'Turn on').click()

	await expect(toggle(page, 'Turn off')).toBeVisible()
	await expect(row(page, 'BackupLink')).not.toContainText(', off')
	await expect(page.locator('.candidate')).not.toContainText('Off.')
	await page.getByRole('button', { name: 'Apply' }).click()
	const [proposal] = await proposals(page)
	expect(proposal.document.attachments[4]).toEqual({ ...IN_FORCE.attachments[4], enabled: true })
})

test('turning a connection off and on again leaves nothing to apply', async ({ page }) => {
	await openNetwork(page, { document: IN_FORCE, capabilities: PI })
	await open(page, 'North site')
	await toggle(page, 'Turn off').click()
	await toggle(page, 'Turn on').click()
	await expect(bar(page)).toContainText('Saved.')
})

test('a connection the device reports off is shown off', async ({ page }) => {
	const document = structuredClone(IN_FORCE)
	document.attachments[4].enabled = false
	const states = structuredClone(STATES)
	states[4] = { is: 'off' }
	await openNetwork(page, { document, capabilities: PI, states })
	await expect(page.locator('.order .state')).toHaveText(['Default route', 'No gateway', 'No lease', 'Up', 'Off'])
})

test('turning off the connection a hotspot cannot run beside takes the notice away', async ({ page }) => {
	await openNetwork(page, { document: IN_FORCE, capabilities: PI })
	await emit(
		page,
		message({
			type: 'fact',
			at: 1,
			fact: 'wireless-network',
			kind: 'text',
			value: 'Clinic-Staff',
			traits: { status: { is: 'passed' }, interface: { name: 'wlan0' }, security: 'sae', channel: { band: '5ghz', number: 136 } },
		}),
	)
	const notice = page.locator('section.hotspot .notice')
	await expect(notice).toHaveText("Can't run beside Clinic-Staff on 5 GHz channel 136; turning that connection off lets it run.")
	await open(page, 'Clinic-Staff')
	await toggle(page, 'Turn off').click()
	await expect(notice).toHaveCount(0)
})

test('the hotspot turned off keeps its settings, and is proposed off on apply', async ({ page }) => {
	await openNetwork(page, { document: IN_FORCE, capabilities: PI })
	await hotspotButton(page, 'Turn off').click()

	const hotspot = page.locator('section.hotspot')
	await expect(hotspot).toContainText('Off. Kept, but not used until turned on.')
	await expect(hotspot.getByLabel('SSID')).toHaveValue('Clinic-Field-04')
	await expect(hotspot.getByLabel('SSID')).toBeEditable()
	await expect(bar(page)).toContainText('1 change not applied.')

	await page.getByRole('button', { name: 'Apply' }).click()
	const [proposal] = await proposals(page)
	expect(proposal.document.hotspot).toEqual({ ...IN_FORCE.hotspot, enabled: false })
})

test('the hotspot turned off is turned on again with its settings', async ({ page }) => {
	const document = structuredClone(IN_FORCE)
	document.hotspot.enabled = false
	await openNetwork(page, { document, capabilities: PI })
	await expect(page.locator('section.hotspot')).toContainText('Off. Kept, but not used until turned on.')
	await hotspotButton(page, 'Turn on').click()

	await expect(hotspotButton(page, 'Turn off')).toBeVisible()
	await expect(page.locator('section.hotspot')).not.toContainText('Off.')
	await page.getByRole('button', { name: 'Apply' }).click()
	const [proposal] = await proposals(page)
	expect(proposal.document.hotspot).toEqual(IN_FORCE.hotspot)
})

test('turning the hotspot off and on again leaves nothing to apply', async ({ page }) => {
	await openNetwork(page, { document: IN_FORCE, capabilities: PI })
	await hotspotButton(page, 'Turn off').click()
	await hotspotButton(page, 'Turn on').click()
	await expect(bar(page)).toContainText('Saved.')
})

test('the hotspot removed is proposed with none', async ({ page }) => {
	await openNetwork(page, { document: IN_FORCE, capabilities: PI })
	await hotspotButton(page, 'Remove').click()

	await expect(page.locator('section.hotspot')).toContainText('Off.')
	await expect(hotspotButton(page, 'Remove')).toHaveCount(0)
	await page.getByRole('button', { name: 'Apply' }).click()
	const [proposal] = await proposals(page)
	expect(proposal.document).not.toHaveProperty('hotspot')
})

test('a hotspot turned off says nothing about the connection it cannot run beside', async ({ page }) => {
	const document = structuredClone(IN_FORCE)
	document.hotspot.enabled = false
	await openNetwork(page, { document, capabilities: PI })
	await emit(
		page,
		message({
			type: 'fact',
			at: 1,
			fact: 'wireless-network',
			kind: 'text',
			value: 'Clinic-Staff',
			traits: { status: { is: 'passed' }, interface: { name: 'wlan0' }, security: 'sae', channel: { band: '5ghz', number: 136 } },
		}),
	)
	const notice = page.locator('section.hotspot .notice')
	await expect(notice).toHaveCount(0)
	await hotspotButton(page, 'Turn on').click()
	await expect(notice).toHaveText("Can't run beside Clinic-Staff on 5 GHz channel 136; turning that connection off lets it run.")
})
