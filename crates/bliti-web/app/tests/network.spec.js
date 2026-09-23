// The network configuration screen of NSCR, driven through a fake device session with no channel and
// no Bluetooth in the loop; the checker a document is held to before proposing is the real one. What
// the page sends is recorded, so "nothing is proposed" is asserted on the wire rather than inferred
// from the screen.

import { expect, test } from '@playwright/test'

import { answer, message, openChannel, openNetwork, say, sent } from './fake-client.js'
import { IN_FORCE, INDEPENDENT, PI, STATES, TWO_RADIOS, WIRED_ONLY, addWireless, ap, bar, open, proposals, row } from './network-fixtures.js'

test.describe('editing is the application\'s own', () => {
	// The session sees a proposal only when the operator asks for one (NSCR).
	test('editing puts nothing on the wire until apply is pressed', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await open(page, 'North site')
		await page.getByLabel('Gateway').fill('192.168.60.254')
		await page.getByLabel('Name', { exact: true }).fill('North site, new router')
		await page.getByLabel('SSID').first().fill('Clinic-Field-05')
		await page.getByRole('button', { name: 'Move BackupLink' }).focus()
		await page.keyboard.press('ArrowUp')
		await expect(bar(page)).toContainText('3 changes not applied.')
		expect(await sent(page)).toEqual([{ type: 'configure' }])

		await page.getByRole('button', { name: 'Apply' }).click()
		const [proposal] = await proposals(page)
		expect(proposal.document.attachments[1]).toMatchObject({ label: 'North site, new router', gateway: '192.168.60.254' })
		expect(proposal.document.attachments.map((each) => each.label)).toEqual([
			'Clinic wall port',
			'North site, new router',
			'eth0 automatic',
			'BackupLink',
			'Clinic-Staff',
		])
		expect(proposal.document.hotspot.ssid).toBe('Clinic-Field-05')
	})

	test('a half-typed gateway is never proposed', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await open(page, 'North site')
		await page.getByLabel('Gateway').fill('192.168.60.')
		await page.getByRole('button', { name: 'Apply' }).click()

		expect(await proposals(page)).toEqual([])
		await expect(page.getByLabel('Gateway')).toHaveClass(/field-fault/)
		await expect(page.getByText('Enter the gateway address.')).toBeVisible()
		await expect(bar(page)).toContainText('Fix the marked field first.')

		await page.getByLabel('Gateway').fill('192.168.60.254')
		await page.getByRole('button', { name: 'Apply' }).click()
		const [proposal] = await proposals(page)
		expect(proposal.document.attachments[1].gateway).toBe('192.168.60.254')
	})

	test('reset returns the fields to the configuration in force', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await open(page, 'North site')
		await page.getByLabel('Gateway').fill('10.9.9.9')
		await page.getByLabel('Resolvers').fill('1.1.1.1, 9.9.9.9')
		await page.getByRole('button', { name: 'Turn off' }).click()
		await page.getByRole('button', { name: 'Reset' }).click()

		await expect(bar(page)).toContainText('Saved.')
		await open(page, 'North site')
		await expect(page.getByLabel('Gateway')).toHaveValue('192.168.60.1')
		await expect(page.getByLabel('Resolvers')).toHaveValue('192.168.60.1')
		await expect(page.getByLabel('SSID')).toHaveValue('Clinic-Field-04')
		expect(await proposals(page)).toEqual([])
	})

	// A field edited part way through verification belongs to neither attempt (NSCR).
	test('fields are not editable while a proposal is being verified', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await open(page, 'North site')
		await page.getByLabel('Gateway').fill('192.168.60.254')
		await page.getByRole('button', { name: 'Apply' }).click()

		await expect(bar(page)).toHaveAttribute('data-stage', 'applying')
		await expect(page.getByLabel('Gateway')).toBeDisabled()
		await expect(page.getByLabel('SSID')).toBeDisabled()
		await expect(page.getByLabel('Country')).toBeDisabled()
		await expect(page.getByRole('button', { name: 'Move BackupLink' })).toBeDisabled()
		await expect(page.getByRole('button', { name: 'Add' })).toBeDisabled()
		await expect(bar(page).getByRole('button')).toHaveText(['Cancel'])

		// Applied is read-only too, and offers confirm and cancel.
		await say(page, message({ type: 'applied' }))
		await expect(bar(page)).toHaveAttribute('data-stage', 'applied')
		await expect(page.getByLabel('Gateway')).toBeDisabled()
		await expect(bar(page).getByRole('button')).toHaveText(['Confirm', 'Cancel'])
	})

	test('confirming saves, and the configuration in force is what the device answers with', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await answer(page, 'configuration', message({ type: 'applied' }))
		const saved = { ...IN_FORCE, 'regulatory-domain': 'FJ' }
		await answer(page, 'confirm', message({ type: 'configuration', document: saved }))

		await page.getByLabel('Country').selectOption('FJ')
		await page.getByRole('button', { name: 'Apply' }).click()
		await expect(bar(page)).toContainText('Applied, not saved.')
		await expect(bar(page)).toContainText('Discarded if you leave or disconnect.')
		await page.getByRole('button', { name: 'Confirm' }).click()

		await expect(bar(page)).toContainText('Saved.')
		await expect(page.getByLabel('Country')).toHaveValue('FJ')
		expect((await sent(page)).map((each) => each.type)).toEqual(['configure', 'configuration', 'confirm'])
	})

	test('cancelling abandons the proposal and returns to the configuration in force', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await page.getByLabel('Country').selectOption('FJ')
		await page.getByRole('button', { name: 'Apply' }).click()
		await bar(page).getByRole('button', { name: 'Cancel' }).click()

		await expect(bar(page)).toHaveAttribute('data-stage', 'editing')
		await expect(page.getByLabel('Country')).toHaveValue('VU')
		expect((await sent(page)).map((each) => each.type)).toEqual(['configure', 'configuration', 'discard'])
		// A late answer to the abandoned proposal changes nothing.
		await say(page, message({ type: 'applied' }))
		await expect(bar(page)).toHaveAttribute('data-stage', 'editing')
	})

	test('leaving the screen ends the session', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await page.getByRole('button', { name: 'Back', exact: true }).click()
		expect(await page.evaluate(() => window.__blitiSessions.map((each) => each.closedByPage))).toEqual([true])
	})

	test('a device already in a session says so', async ({ page }) => {
		await openChannel(page)
		await answer(page, 'configure', message({ type: 'busy' }))
		await page.getByRole('button', { name: 'Network settings' }).click()
		await expect(page.getByText("Someone else is changing this device's network.")).toBeVisible()
	})
})

test.describe('rendering a failure', () => {
	const failAt = (at, reason, reached) => message({ type: 'invalid', at, reason, reached })

	test('after a failure the fields hold what was proposed, and the named field is marked', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await answer(page, 'configuration', failAt("$['attachments'][1]['gateway']", '192.168.60.254 did not answer.', 'gateway'))
		await open(page, 'North site')
		await page.getByLabel('Gateway').fill('192.168.60.254')
		await page.getByRole('button', { name: 'Apply' }).click()

		await expect(bar(page)).toHaveAttribute('data-stage', 'errored')
		await expect(bar(page)).toContainText('Could not apply.')
		await expect(page.getByLabel('Gateway')).toHaveValue('192.168.60.254')
		await expect(page.getByLabel('Gateway')).toHaveClass(/field-fault/)
		await expect(page.getByLabel('Address')).not.toHaveClass(/field-fault/)
		// Errored is writable, and offers apply and reset.
		await expect(page.getByLabel('Gateway')).toBeEditable()
		await expect(bar(page).getByRole('button')).toHaveText(['Apply', 'Reset'])
	})

	test('the verification stages show which passed and which failed', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await answer(page, 'configuration', failAt("$['attachments'][1]['gateway']", 'no answer', 'gateway'))
		await page.getByLabel('Country').selectOption('FJ')
		await page.getByRole('button', { name: 'Apply' }).click()

		const stages = page.locator('.candidate .stages')
		await expect(stages.locator('.done')).toHaveText(['Link', 'Address'])
		await expect(stages.locator('.failed')).toHaveText(['Gateway'])
		// A wired candidate has no association stage.
		await expect(stages).not.toContainText('Joined')
	})

	test('a wireless candidate that did not associate passed only the link', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await answer(page, 'configuration', failAt("$['attachments'][3]['security']['passphrase']", 'the key was refused', 'association'))
		await page.getByLabel('Country').selectOption('FJ')
		await page.getByRole('button', { name: 'Apply' }).click()

		const stages = page.locator('.candidate .stages')
		await expect(stages.locator('.done')).toHaveText(['Link'])
		await expect(stages.locator('.failed')).toHaveText(['Joined'])
		await expect(stages.locator('.untried')).toHaveText(['Address', 'Gateway'])
		await expect(page.locator('.candidate').getByLabel('Passphrase')).toHaveClass(/field-fault/)
	})

	// Absent `reached` is a fault found before anything was applied, so there are no stages to show.
	test('a failure before anything was applied shows no stages', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await answer(page, 'configuration', failAt("$['hotspot']['dhcp-range']", 'collides with the upstream subnet'))
		await page.getByText('Radio and addressing').click()
		await page.getByLabel('DHCP range').fill('10.4.2.0/24')
		await page.getByRole('button', { name: 'Apply' }).click()

		await expect(page.locator('.hotspot')).toContainText('Not accepted. Nothing was changed.')
		await expect(page.locator('.stages')).toHaveCount(0)
		await expect(page.getByLabel('DHCP range')).toHaveClass(/field-fault/)
		await expect(page.getByLabel('DHCP range')).toHaveValue('10.4.2.0/24')
	})

	test("the device's reason is rendered as the device wrote it", async ({ page }) => {
		const reason = "iwd: net.connman.iwd.Failed (reason=15) 'Clinic-Staff' <4-way handshake timeout>"
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await answer(page, 'configuration', failAt("$['attachments'][3]['security']['passphrase']", reason, 'association'))
		await page.getByLabel('Country').selectOption('FJ')
		await page.getByRole('button', { name: 'Apply' }).click()
		await expect(page.locator('.why.reason')).toHaveText(reason)
	})
})

// NSCR: every candidate is checked, except one the operator applies without checking after its
// checking failed a proposal.
test.describe('applying without checking', () => {
	const failure = message({ type: 'invalid', at: "$['attachments'][1]['gateway']", reason: '192.168.60.254 did not answer.', reached: 'gateway' })
	const unchecked = (page) => page.locator('.candidate').getByRole('button', { name: 'Apply without checking' })
	const checkedAgain = (page) => page.locator('.candidate').getByRole('button', { name: 'Turn checking on' })

	async function failGateway(page) {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await answer(page, 'configuration', failure)
		await open(page, 'North site')
		await page.getByLabel('Gateway').fill('192.168.60.254')
		await page.getByRole('button', { name: 'Apply' }).click()
		await expect(bar(page)).toHaveAttribute('data-stage', 'errored')
	}

	test('every candidate is proposed checked, and one added is too', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await page.getByRole('button', { name: 'Add' }).click()
		await page.getByRole('button', { name: 'Wired, static' }).click()
		await page.getByLabel('Name', { exact: true }).fill('South site')
		await page.getByLabel('Address').fill('172.16.4.20/24')
		await page.getByLabel('Gateway').fill('172.16.4.1')
		await page.getByRole('button', { name: 'Apply' }).click()
		const [proposal] = await proposals(page)
		expect(proposal.verify).toBeUndefined()
		expect(proposal.document.attachments.map((each) => each.verify)).toEqual([true, true, true, true, true, true])
		await expect(page.locator('.order')).not.toContainText('not checked')
	})

	test('it is offered only on the candidate whose checking failed', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await open(page, 'North site')
		await expect(unchecked(page)).toHaveCount(0)

		await answer(page, 'configuration', failure)
		await page.getByLabel('Gateway').fill('192.168.60.254')
		await page.getByRole('button', { name: 'Apply' }).click()
		await expect(unchecked(page)).toBeEnabled()
		await expect(page.locator('.candidate')).toContainText('The rest are still checked.')
		await expect(bar(page).getByRole('button')).toHaveText(['Apply', 'Reset'])
		await open(page, 'Clinic wall port')
		await expect(unchecked(page)).toHaveCount(0)

		// Nor once the failure has been reset away.
		await bar(page).getByRole('button', { name: 'Reset' }).click()
		await open(page, 'North site')
		await expect(unchecked(page)).toHaveCount(0)
	})

	test('it is not offered where nothing was checked', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await answer(page, 'configuration', message({ type: 'invalid', at: "$['attachments'][1]['interface']", reason: 'eth0 is not free.' }))
		await open(page, 'North site')
		await page.getByLabel('Gateway').fill('192.168.60.254')
		await page.getByRole('button', { name: 'Apply' }).click()
		await expect(bar(page)).toHaveAttribute('data-stage', 'errored')
		await expect(page.locator('.candidate')).toContainText('eth0 is not free.')
		await expect(unchecked(page)).toHaveCount(0)
	})

	test('the document that failed is proposed again with that candidate not checked', async ({ page }) => {
		await failGateway(page)
		await unchecked(page).click()

		await expect(bar(page)).toHaveAttribute('data-stage', 'applying')
		await expect(page.getByLabel('Gateway')).toBeDisabled()
		const [failed, again] = await proposals(page)
		expect(failed.document.attachments[1].verify).toBe(true)
		expect(again.document.attachments[1]).toEqual({ ...failed.document.attachments[1], verify: false })
		expect(again.document.attachments.filter((_, index) => index !== 1)).toEqual(failed.document.attachments.filter((_, index) => index !== 1))
		expect({ ...again.document, attachments: null }).toEqual({ ...failed.document, attachments: null })
		await expect(row(page, 'North site')).toContainText('not checked')
		await expect(row(page, 'Clinic wall port')).not.toContainText('not checked')

		await say(page, message({ type: 'applied' }))
		await expect(bar(page)).toHaveAttribute('data-stage', 'applied')
		await expect(bar(page)).toContainText('Applied, not saved.')
		await expect(bar(page).getByRole('button')).toHaveText(['Confirm', 'Cancel'])
		await expect(page.locator('.candidate .notice.fault')).toHaveCount(0)
		await expect(page.locator('.candidate')).toContainText('Not checked.')
		await expect(checkedAgain(page)).toBeDisabled()
	})

	test('an edit after the failure is applied with checking', async ({ page }) => {
		await failGateway(page)
		await page.getByLabel('Gateway').fill('192.168.60.1')
		await expect(unchecked(page)).toBeDisabled()
		await expect(page.locator('.candidate')).toContainText('Undo your changes')
		// Back to what failed, and it is offered again.
		await page.getByLabel('Gateway').fill('192.168.60.254')
		await expect(unchecked(page)).toBeEnabled()

		await page.getByLabel('Gateway').fill('192.168.60.1')
		await bar(page).getByRole('button', { name: 'Apply', exact: true }).click()
		const [, edited] = await proposals(page)
		expect(edited.document.attachments[1]).toMatchObject({ gateway: '192.168.60.1', verify: true })
	})

	test('a candidate applied without checking is confirmed like any other, and saved not checked', async ({ page }) => {
		await failGateway(page)
		const [failed] = await proposals(page)
		const saved = structuredClone(failed.document)
		saved.attachments[1].verify = false
		await answer(page, 'configuration', message({ type: 'applied' }))
		await answer(page, 'confirm', message({ type: 'configuration', document: saved }))
		await unchecked(page).click()
		await bar(page).getByRole('button', { name: 'Confirm' }).click()

		await expect(bar(page)).toContainText('Saved.')
		await expect(row(page, 'North site')).toContainText('not checked')
		await open(page, 'North site')
		await expect(page.getByLabel('Gateway')).toHaveValue('192.168.60.254')
		expect((await sent(page)).map((each) => each.type)).toEqual(['configure', 'configuration', 'configuration', 'confirm'])
	})

	// CFG: a candidate not checked is still brought up, and reported through `state`.
	test('a candidate not checked is shown by what the device observed', async ({ page }) => {
		await failGateway(page)
		await answer(page, 'configuration', message({ type: 'applied' }))
		await unchecked(page).click()
		await expect(bar(page)).toHaveAttribute('data-stage', 'applied')
		await say(page, message({
			type: 'state',
			attachments: [
				{ is: 'default-route' },
				{ is: 'unavailable', reached: 'gateway', reason: '192.168.60.254 did not answer' },
				{ is: 'standby' },
				{ is: 'unavailable', reached: 'carrier', reason: 'Clinic-Staff is not in range' },
				{ is: 'up' },
			],
		}))
		await expect(row(page, 'North site').locator('.state')).toHaveText('No gateway')
		await expect(row(page, 'Clinic-Staff').locator('.state')).toHaveText('Out of range')
		await expect(page.locator('.candidate')).toContainText('192.168.60.254 did not answer')
		await expect(page.locator('.candidate .notice.fault')).toHaveCount(0)
	})

	test('checking is turned back on while editing', async ({ page }) => {
		const document = structuredClone(IN_FORCE)
		document.attachments[3].verify = false
		await openNetwork(page, { document, capabilities: PI })
		await expect(row(page, 'Clinic-Staff')).toContainText('Wireless, WPA3, not checked')
		await open(page, 'Clinic-Staff')
		await expect(page.locator('.candidate')).toContainText('Not checked. Applying goes ahead even if this cannot connect.')

		await checkedAgain(page).click()
		await expect(checkedAgain(page)).toHaveCount(0)
		await expect(row(page, 'Clinic-Staff')).not.toContainText('not checked')
		await expect(bar(page)).toContainText('1 change not applied.')
		await page.getByRole('button', { name: 'Apply' }).click()
		const [proposal] = await proposals(page)
		expect(proposal.document.attachments[3].verify).toBe(true)
	})
})

test.describe('validating before proposing', () => {
	// The shared-channel constraint removes band, channel and width, and a sentence stands in their
	// place (HOT, NSCR).
	test('a setting the device did not report is not offered, and the screen says why', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await page.getByText('Radio and addressing').click()
		await expect(page.getByLabel('Band')).toHaveCount(0)
		await expect(page.getByLabel('Channel')).toHaveCount(0)
		await expect(page.getByLabel('Width')).toHaveCount(0)
		await expect(page.getByText('The radio runs the hotspot on the same channel as its wireless connection.')).toBeVisible()
		await expect(page.getByLabel('DHCP range')).toBeVisible()
	})

	test('a radio with independent channels offers them, keyed by band', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: INDEPENDENT })
		await page.getByText('Radio and addressing').click()
		await page.getByLabel('Band').selectOption('5ghz')
		await expect(page.getByLabel('Channel').locator('option')).toHaveText(['Device picks', '36', '40', '44', '48'])
		await page.getByLabel('Channel').selectOption('40')
		await page.getByLabel('Width').selectOption('80')
		// This device did not report a DHCP range, so it is not offered and the screen says so.
		await expect(page.getByLabel('DHCP range')).toHaveCount(0)
		await expect(page.getByText('DHCP range: not offered by this device.')).toBeVisible()

		await page.getByRole('button', { name: 'Apply' }).click()
		const [proposal] = await proposals(page)
		expect(proposal.document.hotspot).toMatchObject({ band: '5ghz', channel: 40, 'channel-width': 80 })
	})

	test('a device with no radio offers no wireless, hotspot or country, and says why', async ({ page }) => {
		const wired = { attachments: [{ kind: 'wired-dynamic', label: 'eth0 automatic', verify: true, interface: 'eth0' }] }
		await openNetwork(page, { document: wired, capabilities: WIRED_ONLY })
		await expect(page.locator('.hotspot')).toContainText('This device has no wireless radio.')
		await expect(page.getByRole('button', { name: 'Turn on' })).toHaveCount(0)
		await expect(page.getByRole('heading', { name: 'Country' })).toHaveCount(0)
		await page.getByRole('button', { name: 'Add' }).click()
		await expect(page.locator('.adding button')).toHaveText(['Wired, DHCP', 'Wired, static'])
	})

	test('a document outside the capabilities is not proposed', async ({ page }) => {
		const withEth1 = {
			attachments: [{ kind: 'wired-dynamic', label: 'eth1 automatic', verify: true, interface: 'eth1' }],
		}
		await openNetwork(page, { document: withEth1, capabilities: PI })
		await open(page, 'eth1 automatic')
		await page.getByLabel('Name', { exact: true }).fill('Second port')
		await page.getByRole('button', { name: 'Apply' }).click()
		expect(await proposals(page)).toEqual([])
		await expect(page.getByLabel('Interface')).toHaveClass(/field-fault/)
		await expect(page.getByText('This device does not support eth1.')).toBeVisible()
	})
})

test.describe('the ordering', () => {
	test("each candidate's state is shown, by what the device observed", async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI, states: STATES })
		await expect(page.locator('.order .state')).toHaveText(['Default route', 'No gateway', 'No lease', 'Up', 'Out of range'])
		await open(page, 'North site')
		await expect(page.locator('.candidate')).toContainText('192.168.60.1 did not answer')

		// A state follows its candidate through reordering, and changes when the device reports it has.
		await page.getByRole('button', { name: 'Move BackupLink' }).focus()
		await page.keyboard.press('ArrowUp')
		await expect(row(page, 'BackupLink').locator('.state')).toHaveText('Out of range')
		await say(page, message({ type: 'state', attachments: [{ is: 'up' }, { is: 'default-route' }, { is: 'standby' }, { is: 'up' }, { is: 'up' }] }))
		await expect(row(page, 'North site').locator('.state')).toHaveText('Default route')
		await expect(row(page, 'BackupLink').locator('.state')).toHaveText('Up')
	})

	test('a candidate is added, edited and proposed with the others', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await page.getByRole('button', { name: 'Add' }).click()
		await page.getByRole('button', { name: 'Wired, static' }).click()
		await page.getByLabel('Name', { exact: true }).fill('South site')
		await page.getByLabel('Address').fill('172.16.4.20/24')
		await page.getByLabel('Gateway').fill('172.16.4.1')
		await page.getByRole('button', { name: 'Apply' }).click()
		const [proposal] = await proposals(page)
		expect(proposal.document.attachments[5]).toEqual({
			kind: 'wired-static',
			label: 'South site',
			verify: true,
			interface: 'eth0',
			addresses: ['172.16.4.20/24'],
			gateway: '172.16.4.1',
		})
	})

	test('a scanned network fills the SSID, and one the device will not join cannot be picked', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await answer(page, 'scan', message({
			type: 'networks',
			'access-points': [
				ap({ bssid: 'a4:2b:b0:11:2c:40', ssid: 'Clinic', security: ['psk', 'sae'], signal: -52 }),
				ap({ bssid: '5c:a6:e6:02:71:9b', ssid: 'Guest', security: ['open'], signal: -61, band: '2.4ghz', channel: 6 }),
			],
		}))
		await addWireless(page)
		await page.getByRole('button', { name: 'Scan' }).click()
		await expect(page.getByRole('button', { name: 'Guest', exact: true })).toBeDisabled()
		await page.getByRole('button', { name: 'Clinic', exact: true }).click()
		await expect(page.locator('.candidate').getByLabel('SSID')).toHaveValue('Clinic')
		await expect(page.getByLabel('Security')).toHaveValue('psk-sae')
	})
})

test.describe('the state of the session', () => {
	test('whether the running configuration is durable stays in view while the operator scrolls', async ({ page }) => {
		await page.setViewportSize({ width: 390, height: 640 })
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await open(page, 'North site')
		await page.getByText('Radio and addressing').click()
		await page.getByLabel('Country').selectOption('FJ')
		await page.getByRole('button', { name: 'Apply' }).click()
		await say(page, message({ type: 'applied' }))

		for (const y of [0, 400, 800]) {
			await page.evaluate((y) => window.scrollTo(0, y), y)
			await expect(bar(page)).toBeInViewport({ ratio: 1 })
			await expect(bar(page)).toContainText('Applied, not saved.')
		}
		expect(await page.evaluate(() => document.documentElement.scrollHeight > window.innerHeight)).toBe(true)
	})

	test('the vocabulary of the wire does not appear on screen', async ({ page }) => {
		const document = structuredClone(IN_FORCE)
		document.attachments[4].verify = false
		await openNetwork(page, { document, capabilities: PI, states: STATES })
		await answer(page, 'configuration', message({ type: 'invalid', at: "$['attachments'][1]['gateway']", reason: 'no answer', reached: 'gateway' }))
		await page.getByLabel('Country').selectOption('FJ')
		await page.getByRole('button', { name: 'Apply' }).click()
		await page.getByText('Radio and addressing').click()
		const shown = await page.locator('.network').innerText()
		for (const word of ['invalid', 'configure', 'discard', 'wired-static', 'wired-dynamic', 'psk', 'sae', 'regulatory', 'default-route', 'unavailable', 'verif', '$[']) {
			expect(shown.toLowerCase()).not.toContain(word)
		}
	})
})

test.describe('adapters', () => {
	const options = (locator) => locator.locator('option')

	// NET: a candidate naming no interface is within capabilities where any radio admits it, and one
	// naming an interface is held to that radio's own.
	test('a wireless candidate is offered only what its adapter supports, and everything where unset', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: TWO_RADIOS })
		await addWireless(page)
		const candidate = page.locator('.candidate')
		await expect(options(candidate.getByLabel('Adapter'))).toHaveText(['Device chooses', 'Cypress CYW43455', 'MediaTek MT7921AU'])
		await expect(candidate.getByLabel('Adapter')).toHaveValue('')
		await expect(options(candidate.getByLabel('Security'))).toHaveText(['WPA2/WPA3', 'WPA3', 'WPA2', 'Enterprise'])
		await expect(candidate.getByLabel('Hidden network')).toBeVisible()

		await candidate.getByLabel('Security').selectOption('enterprise')
		await candidate.getByLabel('Adapter').selectOption('wlan0')
		await expect(options(candidate.getByLabel('Security'))).toHaveText(['WPA2/WPA3', 'WPA3', 'WPA2'])
		// Moving to an adapter that cannot join enterprise networks moves the candidate off it.
		await expect(candidate.getByLabel('Security')).toHaveValue('psk-sae')
		await expect(candidate.getByLabel('Hidden network')).toHaveCount(0)

		await candidate.getByLabel('SSID').fill('Clinic')
		await candidate.getByLabel('Passphrase').fill('correct horse battery')
		await page.getByRole('button', { name: 'Apply' }).click()
		const [proposal] = await proposals(page)
		expect(proposal.document.attachments[5]).toEqual({
			kind: 'wireless',
			label: 'Clinic',
			verify: true,
			ssid: 'Clinic',
			interface: 'wlan0',
			security: { kind: 'psk-sae', passphrase: 'correct horse battery' },
		})
	})

	test('the hotspot is offered the bands and channels of its adapter, and says why one has none', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: TWO_RADIOS })
		const hotspot = page.locator('.hotspot')
		await hotspot.getByText('Radio and addressing').click()
		await expect(options(hotspot.getByLabel('Band'))).toHaveText(['Device picks', '2.4 GHz', '5 GHz'])

		await hotspot.getByLabel('Band').selectOption('5ghz')
		await hotspot.getByLabel('Adapter').selectOption('wlan0')
		await expect(hotspot.getByLabel('Band')).toHaveCount(0)
		await expect(hotspot.getByLabel('Channel')).toHaveCount(0)
		await expect(hotspot).toContainText('The Cypress CYW43455 runs the hotspot on the same channel as its wireless connection.')

		await hotspot.getByLabel('Adapter').selectOption('wlx00c0caa1b2c3')
		await hotspot.getByLabel('Band').selectOption('2.4ghz')
		await expect(options(hotspot.getByLabel('Channel'))).toHaveText(['Device picks', '1', '6', '11'])
		await hotspot.getByLabel('Channel').selectOption('6')
		await page.getByRole('button', { name: 'Apply' }).click()
		const [proposal] = await proposals(page)
		expect(proposal.document.hotspot).toMatchObject({ interface: 'wlx00c0caa1b2c3', band: '2.4ghz', channel: 6 })
	})

	test('a single radio offers no adapter to choose', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await addWireless(page)
		await expect(page.getByLabel('Adapter')).toHaveCount(0)
	})
})

test.describe('the session as the device speaks it', () => {
	test('the state of each candidate is read as the device sends it, checking included', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await say(page, message({
			type: 'state',
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

	test('joining by PIN shows the PIN the device generated', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await page.getByRole('button', { name: 'Add' }).click()
		await page.getByRole('button', { name: 'WPS PIN' }).click()
		expect((await sent(page)).at(-1)).toEqual({ type: 'wps', method: 'pin' })
		await expect(bar(page)).toContainText('Waiting for the PIN.')
		await say(page, message({ type: 'pin', pin: '12345670' }))
		await expect(bar(page).locator('.pin')).toHaveText('12345670')
		await expect(bar(page)).toContainText('Enter 12345670 on the access point.')
	})

	// CFG: capabilities a proposal changed come on `applied`, those a return to the recorded
	// configuration changed back on the next `state`, and the next proposal is checked against the
	// latest.
	test('capabilities are refreshed from applied, and from the state after a revert', async ({ page }) => {
		const narrowed = structuredClone(INDEPENDENT)
		narrowed.document.hotspot.interface.wlan0.band['5ghz'].channel = [36, 40]
		await openNetwork(page, { document: IN_FORCE, capabilities: INDEPENDENT })
		await answer(page, 'configuration', message({ type: 'applied', capabilities: narrowed }))
		const hotspot = page.locator('.hotspot')
		await hotspot.getByText('Radio and addressing').click()
		await hotspot.getByLabel('Band').selectOption('5ghz')
		await expect(hotspot.getByLabel('Channel').locator('option')).toHaveText(['Device picks', '36', '40', '44', '48'])

		await page.getByLabel('Country').selectOption('FJ')
		await page.getByRole('button', { name: 'Apply' }).click()
		await expect(bar(page)).toHaveAttribute('data-stage', 'applied')
		await expect(hotspot.getByLabel('Channel').locator('option')).toHaveText(['Device picks', '36', '40'])
		await bar(page).getByRole('button', { name: 'Cancel' }).click()
		// The device said nothing of them since, so the narrowed set still holds.
		await hotspot.getByLabel('Band').selectOption('5ghz')
		await expect(hotspot.getByLabel('Channel').locator('option')).toHaveText(['Device picks', '36', '40'])

		await say(page, message({ type: 'state', attachments: [], capabilities: INDEPENDENT }))
		await hotspot.getByLabel('Band').selectOption('5ghz')
		await expect(hotspot.getByLabel('Channel').locator('option')).toHaveText(['Device picks', '36', '40', '44', '48'])
	})

	// CFG: every proposal is answered once, an interrupted one with `invalid` at `$`, so the answer to
	// an abandoned proposal is not read as the answer to the next.
	test('the answer to an abandoned proposal is not taken for the next one', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await page.getByLabel('Country').selectOption('FJ')
		await page.getByRole('button', { name: 'Apply' }).click()
		await bar(page).getByRole('button', { name: 'Cancel' }).click()
		await page.getByLabel('Country').selectOption('WS')
		await page.getByRole('button', { name: 'Apply' }).click()
		await expect(bar(page)).toHaveAttribute('data-stage', 'applying')

		await say(page, message({ type: 'invalid', at: '$', reason: 'discarded before it was verified' }))
		await expect(bar(page)).toHaveAttribute('data-stage', 'applying')
		await say(page, message({ type: 'applied' }))
		await expect(bar(page)).toHaveAttribute('data-stage', 'applied')
		expect((await sent(page)).map((each) => each.type)).toEqual(['configure', 'configuration', 'discard', 'configuration'])
	})
})
