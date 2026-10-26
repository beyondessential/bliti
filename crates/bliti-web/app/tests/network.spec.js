// The network configuration screen of NSCR, driven through a fake device session with no channel and
// no Bluetooth in the loop; the checker a document is held to before proposing is the real one. What
// the page sends is recorded, so "nothing is proposed" is asserted on the wire rather than inferred
// from the screen.

import { expect, test } from '@playwright/test'

import { answer, echo, failsAt, feed, message, openNetwork, refusing, say, sent, stateOf } from './fake-client.js'
import { IN_FORCE, INDEPENDENT, PI, STATES, TWO_RADIOS, UNTUNED, WIRED_ONLY, addWireless, ap, bar, open, proposals, row } from './network-fixtures.js'

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
		await page.locator('section.hotspot').getByRole('button', { name: 'Turn off' }).click()
		await page.getByRole('button', { name: 'Reset' }).click()

		await expect(bar(page)).toContainText('Applied.')
		await open(page, 'North site')
		await expect(page.getByLabel('Gateway')).toHaveValue('192.168.60.1')
		await expect(page.getByLabel('Resolvers')).toHaveValue('192.168.60.1')
		await expect(page.getByLabel('SSID')).toHaveValue('Clinic-Field-04')
		expect(await proposals(page)).toEqual([])
	})

	// A field edited part way through verification belongs to neither attempt (NSCR).
	test('fields are not editable while a proposal is being verified, and are again once it is applied', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await answer(page, 'configuration', [echo(), stateOf({ outcome: 'verifying' })])
		await open(page, 'North site')
		await page.getByLabel('Gateway').fill('192.168.60.254')
		await page.getByRole('button', { name: 'Apply' }).click()

		await expect(bar(page)).toHaveAttribute('data-stage', 'applying')
		await expect(page.getByLabel('Gateway')).toBeDisabled()
		await expect(page.getByLabel('SSID')).toBeDisabled()
		await expect(page.getByLabel('Country')).toBeDisabled()
		await expect(page.getByRole('button', { name: 'Move BackupLink' })).toBeDisabled()
		await expect(page.getByRole('button', { name: 'Add' })).toBeDisabled()
		await expect(bar(page).getByRole('button')).toHaveCount(0)

		await say(page, stateOf({ outcome: 'applied' }))
		await expect(bar(page)).toHaveAttribute('data-stage', 'editing')
		await expect(page.getByLabel('Gateway')).toBeEditable()
		await expect(bar(page).getByRole('button')).toHaveText(['Apply', 'Reset'])
	})

	// The device saves a proposal before it applies it, and says so by sending it back (CFG).
	test('a proposal is saved when the device sends it back, then verified, then applied', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await answer(page, 'configuration', echo())
		await page.getByLabel('Country').selectOption('FJ')
		await page.getByRole('button', { name: 'Apply' }).click()

		// Entered on apply, before anything is answered, and held through the echo until the state after it.
		await expect(bar(page)).toHaveAttribute('data-stage', 'applying')
		await expect(bar(page)).toContainText('Applying…')
		await expect(bar(page)).not.toContainText('onfirm')
		await expect(page.getByLabel('Country')).toHaveValue('FJ')
		await say(page, stateOf({ outcome: 'verifying' }))
		await expect(bar(page)).toHaveAttribute('data-stage', 'applying')
		await expect(bar(page).locator('.spinner')).toHaveCount(1)

		await say(page, stateOf({ outcome: 'applied' }))
		await expect(bar(page)).toHaveAttribute('data-stage', 'editing')
		await expect(bar(page)).toContainText('Applied.')
		await expect(bar(page)).not.toContainText('change')
		await expect(page.getByLabel('Country')).toHaveValue('FJ')
		expect((await sent(page)).map((each) => each.type)).toEqual(['configure', 'configuration'])
	})

	test('there is nothing to confirm or cancel anywhere', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await answer(page, 'configuration', [echo(), stateOf({ outcome: 'verifying' })])
		await page.getByLabel('Country').selectOption('FJ')
		await page.getByRole('button', { name: 'Apply' }).click()
		await expect(bar(page)).toHaveAttribute('data-stage', 'applying')
		for (const name of ['Confirm', 'Cancel', 'Discard', 'Review']) {
			await expect(page.getByRole('button', { name })).toHaveCount(0)
		}
	})

	test('leaving the screen with nothing changed ends the session', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await page.getByRole('button', { name: 'Back', exact: true }).click()
		await expect(page.getByRole('alertdialog')).toHaveCount(0)
		await expect(page.getByRole('button', { name: 'Network settings' })).toBeVisible()
		expect(await page.evaluate(() => window.__blitiSessions.map((each) => each.closedByPage))).toEqual([true])
	})

	// Edits not applied are lost on leaving, so leaving asks first (NSCR).
	test('leaving with edits not applied asks first, and leaves them behind', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await page.getByLabel('Country').selectOption('FJ')
		await page.getByRole('button', { name: 'Back', exact: true }).click()

		const asking = page.getByRole('alertdialog', { name: 'Leave the network settings?' })
		await expect(asking).toContainText('will be lost')
		expect(await page.evaluate(() => window.__blitiSessions.map((each) => each.closedByPage))).toEqual([false])
		await asking.getByRole('button', { name: 'Stay' }).click()
		await expect(asking).toHaveCount(0)
		await expect(page.getByLabel('Country')).toHaveValue('FJ')
		expect(await page.evaluate(() => window.__blitiSessions.map((each) => each.closedByPage))).toEqual([false])

		await page.getByRole('button', { name: 'Back', exact: true }).click()
		await page.getByRole('alertdialog').getByRole('button', { name: 'Leave' }).click()
		await expect(page.getByRole('button', { name: 'Network settings' })).toBeVisible()
		expect(await page.evaluate(() => window.__blitiSessions.map((each) => each.closedByPage))).toEqual([true])
		expect((await sent(page)).map((each) => each.type)).toEqual(['configure'])

		// What was left behind is gone: the screen opens on the configuration the device holds.
		await answer(page, 'configure', [message({ type: 'configuration', document: IN_FORCE, capabilities: PI }), stateOf()])
		await page.getByRole('button', { name: 'Network settings' }).click()
		await expect(page.getByLabel('Country')).toHaveValue('VU')
		await expect(bar(page)).toContainText('Applied.')
	})

	// A failed proposal the device saved is the configuration it holds: nothing is lost by leaving (NSCR).
	test('leaving after a failure the device saved asks nothing, and reopening shows the failure', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await answer(page, 'configuration', failsAt("$['attachments'][1]['gateway']", '192.168.60.254 did not answer', 'gateway'))
		await open(page, 'North site')
		await page.getByLabel('Gateway').fill('192.168.60.254')
		await page.getByRole('button', { name: 'Apply' }).click()
		await expect(bar(page)).toHaveAttribute('data-stage', 'errored')
		await expect(bar(page)).toContainText('Could not apply.')
		await expect(bar(page)).toContainText('The device is on these settings.')
		await expect(bar(page)).not.toContainText('back on')

		await page.getByRole('button', { name: 'Back', exact: true }).click()
		await expect(page.getByRole('alertdialog')).toHaveCount(0)
		await expect(page.getByRole('button', { name: 'Network settings' })).toBeVisible()
		expect(await page.evaluate(() => window.__blitiSessions.map((each) => each.closedByPage))).toEqual([true])

		// The next session opens on what the device holds and how it fared.
		const [proposal] = await proposals(page)
		await answer(page, 'configure', [
			message({ type: 'configuration', document: proposal.document, capabilities: PI }),
			stateOf({ outcome: 'failed', failure: { at: "$['attachments'][1]['gateway']", reason: '192.168.60.254 did not answer', reached: 'gateway' } }),
		])
		await page.getByRole('button', { name: 'Network settings' }).click()
		await expect(bar(page)).toHaveAttribute('data-stage', 'errored')
		await expect(page.locator('.candidate')).toHaveAttribute('aria-label', 'North site')
		await expect(page.getByLabel('Gateway')).toHaveValue('192.168.60.254')
		await expect(page.getByLabel('Gateway')).toHaveClass(/field-fault/)
		await expect(page.locator('.candidate .reason')).toHaveText('192.168.60.254 did not answer')
		await expect(page.locator('.candidate .stages .failed')).toHaveText(['Gateway'])
	})

	test('a refused proposal is not held by the device, so leaving asks', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await answer(page, 'configuration', refusing("$['regulatory-domain']", 'not a country this radio knows', IN_FORCE))
		await page.getByLabel('Country').selectOption('FJ')
		await page.getByRole('button', { name: 'Apply' }).click()
		await expect(bar(page)).toHaveAttribute('data-stage', 'errored')
		await page.getByRole('button', { name: 'Back', exact: true }).click()
		await expect(page.getByRole('alertdialog', { name: 'Leave the network settings?' })).toBeVisible()
	})

	// NSCR: the browser warns before the page goes only while edits would be lost.
	test('the browser warns before the page goes only while edits are not applied', async ({ page }) => {
		const warns = () =>
			page.evaluate(() => {
				const event = new Event('beforeunload', { cancelable: true })
				window.dispatchEvent(event)
				return event.defaultPrevented
			})
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		expect(await warns()).toBe(false)

		await page.getByLabel('Country').selectOption('FJ')
		expect(await warns()).toBe(true)

		await answer(page, 'configuration', [echo(), stateOf({ outcome: 'verifying' })])
		await page.getByRole('button', { name: 'Apply' }).click()
		await expect(bar(page)).toHaveAttribute('data-stage', 'applying')
		expect(await warns()).toBe(false)

		await say(page, stateOf({ outcome: 'applied' }))
		await expect(bar(page)).toHaveAttribute('data-stage', 'editing')
		expect(await warns()).toBe(false)

		await page.getByLabel('Country').selectOption('WS')
		expect(await warns()).toBe(true)
		await page.getByRole('button', { name: 'Reset' }).click()
		expect(await warns()).toBe(false)

		// A screen left behind has no listener to leave standing.
		await page.getByLabel('Country').selectOption('WS')
		expect(await warns()).toBe(true)
		await page.getByRole('button', { name: 'Back', exact: true }).click()
		await page.getByRole('alertdialog').getByRole('button', { name: 'Leave' }).click()
		expect(await warns()).toBe(false)
	})

	// NSCR: a session opens on how the device is faring, not on a stage of its own.
	test('opening while the device verifies shows applying, read-only', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI, outcome: 'verifying' })
		await expect(bar(page)).toHaveAttribute('data-stage', 'applying')
		await expect(bar(page).locator('.spinner')).toHaveCount(1)
		await expect(page.getByLabel('Country')).toBeDisabled()
		await expect(page.getByRole('button', { name: 'Add' })).toBeDisabled()
		await expect(bar(page).getByRole('button')).toHaveCount(0)

		// And it follows the outcome as the device settles it.
		await say(page, stateOf({ outcome: 'applied' }))
		await expect(bar(page)).toHaveAttribute('data-stage', 'editing')
		await expect(page.getByLabel('Country')).toBeEditable()
	})

	test('opening on a configuration that failed shows the failure', async ({ page }) => {
		await openNetwork(page, {
			document: IN_FORCE,
			capabilities: PI,
			outcome: 'failed',
			failure: { at: "$['attachments'][3]['security']['passphrase']", reason: 'the key was refused', reached: 'association' },
		})
		await expect(bar(page)).toHaveAttribute('data-stage', 'errored')
		await expect(page.locator('.candidate')).toHaveAttribute('aria-label', 'Clinic-Staff')
		await expect(page.locator('.candidate').getByLabel('Passphrase')).toHaveClass(/field-fault/)
		await expect(page.locator('.candidate .reason')).toHaveText('the key was refused')
		await expect(page.getByLabel('Country')).toBeEditable()
		await expect(bar(page).getByRole('button')).toHaveText(['Apply', 'Reset'])
	})

	// NSCR: another operator's session is watched, never edited.
	test('a session the device reports busy is watched, read-only, and offers no scan or survey', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: INDEPENDENT, busy: true })
		await expect(bar(page)).toHaveAttribute('data-stage', 'watching')
		await expect(bar(page)).toContainText("Someone else is changing this device's network.")
		await expect(page.getByRole('heading', { name: 'Connections' })).toBeVisible()
		await expect(bar(page).getByRole('button')).toHaveCount(0)
		await expect(page.getByLabel('Country')).toBeDisabled()
		await expect(page.getByRole('button', { name: 'Add' })).toBeDisabled()
		await expect(page.getByRole('button', { name: 'Move BackupLink' })).toBeDisabled()
		await open(page, 'North site')
		await expect(page.getByLabel('Gateway')).toBeDisabled()
		await page.locator('.hotspot').getByText('Radio and addressing').click()
		await expect(page.getByRole('button', { name: /^Scan/ })).toHaveCount(0)
		await expect(page.getByRole('button', { name: 'Survey the spectrum' })).toHaveCount(0)
		expect((await sent(page)).map((each) => each.type)).toEqual(['configure'])
	})

	test('a watching session follows what the holder applies, and becomes editable once it is done', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI, busy: true, states: STATES })
		await expect(page.getByLabel('Country')).toHaveValue('VU')

		// The holder applies a change: the configuration, then the outcome, as the device reports them.
		const changed = { ...IN_FORCE, 'regulatory-domain': 'FJ' }
		await say(page, message({ type: 'configuration', document: changed }))
		await say(page, stateOf({ busy: true, outcome: 'verifying' }))
		await expect(page.getByLabel('Country')).toHaveValue('FJ')
		await expect(bar(page)).toHaveAttribute('data-stage', 'watching')
		await say(page, stateOf({ busy: true, outcome: 'failed', failure: { at: "$['regulatory-domain']", reason: 'no' } }))
		await expect(bar(page)).toHaveAttribute('data-stage', 'watching')
		await expect(page.getByLabel('Country')).toBeDisabled()

		// The holder closes: this one holds, on the configuration the device holds and how it fared.
		await say(page, stateOf({ busy: false, outcome: 'failed', failure: { at: "$['regulatory-domain']", reason: 'no' } }))
		await expect(bar(page)).toHaveAttribute('data-stage', 'errored')
		await expect(page.getByLabel('Country')).toBeEditable()
		await expect(page.getByLabel('Country')).toHaveValue('FJ')
		await expect(page.locator('.hotspot')).toBeVisible()
		await expect(page.locator('.why.reason')).toHaveText('no')
		await expect(page.getByRole('button', { name: 'Add' })).toBeEnabled()
	})

	test('a watching session that becomes the holder is editable and offers a scan', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI, busy: true })
		await say(page, stateOf({ busy: false }))
		await expect(bar(page)).toHaveAttribute('data-stage', 'editing')
		await expect(page.getByRole('button', { name: 'Scan' })).toBeVisible()
		await page.getByLabel('Country').selectOption('FJ')
		await expect(bar(page)).toContainText('1 change not applied.')
	})

	// A session that ends with the channel still up opens again from the screen.
	test('a session that stops answering starts again', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await page.evaluate(() => window.__blitiSession.close('the stream was reset'))
		await expect(page.getByText('The device stopped answering: the stream was reset')).toBeVisible()
		await answer(page, 'configure', [message({ type: 'configuration', document: IN_FORCE, capabilities: PI }), stateOf()])
		await page.getByRole('button', { name: 'Start again' }).click()
		await page.getByRole('heading', { name: 'Connections' }).waitFor()
		expect(await page.evaluate(() => window.__blitiSessions.length)).toBe(2)
	})

	// Found on the prototype: where the channel under the session is gone, starting again fails at
	// once and the screen looked unchanged. It says so, and offers to disconnect so the operator can
	// connect again.
	test('a session that cannot be opened again offers to disconnect', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await page.evaluate(() => window.__blitiSession.close('the stream was reset'))
		await page.evaluate(() => {
			window.__blitiConfigureFails = 'this channel is not connected'
		})
		await page.getByRole('button', { name: 'Start again' }).click()
		await expect(
			page.getByText("The device's network settings could not be opened again: this channel is not connected"),
		).toBeVisible()
		await expect(page.getByRole('button', { name: 'Start again' })).toHaveCount(0)
		await page.getByRole('button', { name: 'Disconnect' }).click()
		await expect(page.getByRole('button', { name: 'Find my Iti' })).toBeVisible()
	})
})

test.describe('rendering a failure', () => {
	test('after a failure the fields hold what was proposed, and the named field is marked', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await answer(page, 'configuration', failsAt("$['attachments'][1]['gateway']", '192.168.60.254 did not answer.', 'gateway'))
		await open(page, 'North site')
		await page.getByLabel('Gateway').fill('192.168.60.254')
		await page.getByRole('button', { name: 'Apply' }).click()

		await expect(bar(page)).toHaveAttribute('data-stage', 'errored')
		await expect(bar(page)).toContainText('Could not apply.')
		await expect(page.locator('.candidate .notice.fault')).toHaveText('Applied, then could not be reached.')
		await expect(page.getByLabel('Gateway')).toHaveValue('192.168.60.254')
		await expect(page.getByLabel('Gateway')).toHaveClass(/field-fault/)
		await expect(page.getByLabel('Address')).not.toHaveClass(/field-fault/)
		// Errored is writable, and offers apply and reset.
		await expect(page.getByLabel('Gateway')).toBeEditable()
		await expect(bar(page).getByRole('button')).toHaveText(['Apply', 'Reset'])
	})

	test('the verification stages show which passed and which failed', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await answer(page, 'configuration', failsAt("$['attachments'][1]['gateway']", 'no answer', 'gateway'))
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
		await answer(page, 'configuration', failsAt("$['attachments'][3]['security']['passphrase']", 'the key was refused', 'association'))
		await page.getByLabel('Country').selectOption('FJ')
		await page.getByRole('button', { name: 'Apply' }).click()

		const stages = page.locator('.candidate .stages')
		await expect(stages.locator('.done')).toHaveText(['Link'])
		await expect(stages.locator('.failed')).toHaveText(['Joined'])
		await expect(stages.locator('.untried')).toHaveText(['Address', 'Gateway'])
		await expect(page.locator('.candidate').getByLabel('Passphrase')).toHaveClass(/field-fault/)
	})

	// A refusal carries no `reached`: nothing was saved, so there are no stages to show.
	test('a refusal keeps the proposal, marks the field, and says nothing was changed', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await answer(page, 'configuration', refusing("$['hotspot']['dhcp-range']", 'collides with the upstream subnet', IN_FORCE))
		await page.getByText('Radio and addressing').click()
		await page.getByLabel('DHCP range').fill('10.4.2.0/24')
		await page.getByRole('button', { name: 'Apply' }).click()

		await expect(bar(page)).toHaveAttribute('data-stage', 'errored')
		await expect(bar(page)).toContainText('Not applied.')
		await expect(bar(page)).toContainText('The device did not take these settings')
		await expect(page.locator('.hotspot')).toContainText('Not accepted. Nothing was changed.')
		await expect(page.locator('.hotspot .reason')).toHaveText('collides with the upstream subnet')
		await expect(page.locator('.stages')).toHaveCount(0)
		await expect(page.getByLabel('DHCP range')).toHaveClass(/field-fault/)
		await expect(page.getByLabel('DHCP range')).toHaveValue('10.4.2.0/24')
		await expect(page.getByLabel('DHCP range')).toBeEditable()
		await expect(bar(page).getByRole('button')).toHaveText(['Apply', 'Reset'])

		// The proposal is still unapplied, so it counts, and applying it again proposes it again.
		const [first] = await proposals(page)
		await page.getByLabel('DHCP range').fill('10.4.3.0/24')
		await page.getByRole('button', { name: 'Apply' }).click()
		const [, second] = await proposals(page)
		expect(second.document.hotspot['dhcp-range']).toBe('10.4.3.0/24')
		expect(first.document.hotspot['dhcp-range']).toBe('10.4.2.0/24')
	})

	test('reset after a refusal returns the fields to what the device holds', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await answer(page, 'configuration', refusing("$['regulatory-domain']", 'not a country this radio knows', IN_FORCE))
		await page.getByLabel('Country').selectOption('FJ')
		await page.getByRole('button', { name: 'Apply' }).click()
		await expect(bar(page)).toHaveAttribute('data-stage', 'errored')
		await bar(page).getByRole('button', { name: 'Reset' }).click()
		await expect(bar(page)).toHaveAttribute('data-stage', 'editing')
		await expect(page.getByLabel('Country')).toHaveValue('VU')
		await expect(page.locator('.notice.fault')).toHaveCount(0)
	})

	// CFG: a proposal is saved only where the device sends back what was proposed.
	test('a configuration other than the proposal is not taken for it being saved', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await answer(page, 'configuration', [message({ type: 'configuration', document: IN_FORCE }), stateOf()])
		await page.getByLabel('Country').selectOption('FJ')
		await page.getByRole('button', { name: 'Apply' }).click()
		await expect(bar(page)).toHaveAttribute('data-stage', 'errored')
		await expect(page.getByLabel('Country')).toHaveValue('FJ')
		await expect(bar(page)).toContainText('did not take these settings')
	})

	// CFG: the outcome stands, so a failure outlasts the state sent after a refusal.
	test('a refusal stands through the state that follows it', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await answer(page, 'configuration', refusing("$['regulatory-domain']", 'no', IN_FORCE))
		await page.getByLabel('Country').selectOption('FJ')
		await page.getByRole('button', { name: 'Apply' }).click()
		await expect(page.getByText('no', { exact: true })).toBeVisible()
		await say(page, stateOf())
		await expect(bar(page)).toHaveAttribute('data-stage', 'errored')
		await expect(page.getByText('no', { exact: true })).toBeVisible()
	})

	test("the device's reason is rendered as the device wrote it", async ({ page }) => {
		const reason = "iwd: net.connman.iwd.Failed (reason=15) 'Clinic-Staff' <4-way handshake timeout>"
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await answer(page, 'configuration', failsAt("$['attachments'][3]['security']['passphrase']", reason, 'association'))
		await page.getByLabel('Country').selectOption('FJ')
		await page.getByRole('button', { name: 'Apply' }).click()
		await expect(page.locator('.why.reason')).toHaveText(reason)
	})
})

// NSCR: every candidate is checked, except one the operator applies without checking after its
// checking failed a proposal.
test.describe('applying without checking', () => {
	const failure = failsAt("$['attachments'][1]['gateway']", '192.168.60.254 did not answer.', 'gateway')
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
		expect(proposal.document.attachments.map((each) => each.enabled)).toEqual([true, true, true, true, true, true])
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
		await expect(bar(page).getByRole('button')).toHaveText(['Apply', 'Reset'])
		await open(page, 'Clinic wall port')
		await expect(unchecked(page)).toHaveCount(0)

		// The failure stands on the device, so reset leaves it offered on the candidate it names.
		await bar(page).getByRole('button', { name: 'Reset' }).click()
		await expect(page.locator('.candidate')).toHaveAttribute('aria-label', 'North site')
		await expect(unchecked(page)).toBeEnabled()
	})

	// A refused passphrase names the field beneath its candidate, and the offer is on that candidate.
	test('a refused passphrase marks its field, and its candidate is offered unchecked', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await answer(page, 'configuration', failsAt(
			"$['attachments'][3]['security']['passphrase']",
			'"Clinic-Staff" refused the connection; the passphrase is most likely wrong',
			'association',
		))
		await page.getByLabel('Country').selectOption('FJ')
		await page.getByRole('button', { name: 'Apply' }).click()
		await expect(bar(page)).toHaveAttribute('data-stage', 'errored')

		await expect(page.locator('.candidate').getByLabel('Passphrase')).toHaveClass(/field-fault/)
		await expect(page.locator('.candidate').getByLabel('SSID')).not.toHaveClass(/field-fault/)
		await unchecked(page).click()
		const [failed, again] = await proposals(page)
		expect(again.document.attachments[3]).toEqual({ ...failed.document.attachments[3], verify: false })
		expect(again.document.attachments.filter((_, index) => index !== 3)).toEqual(failed.document.attachments.filter((_, index) => index !== 3))
	})

	test('it is not offered where nothing was checked', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await answer(page, 'configuration', refusing("$['attachments'][1]['interface']", 'eth0 is not free.', IN_FORCE))
		await open(page, 'North site')
		await page.getByLabel('Gateway').fill('192.168.60.254')
		await page.getByRole('button', { name: 'Apply' }).click()
		await expect(bar(page)).toHaveAttribute('data-stage', 'errored')
		await expect(page.locator('.candidate')).toContainText('eth0 is not free.')
		await expect(unchecked(page)).toHaveCount(0)
	})

	test('the document that failed is proposed again with that candidate not checked', async ({ page }) => {
		await failGateway(page)
		await answer(page, 'configuration', [echo(), stateOf({ outcome: 'verifying' })])
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

		await say(page, stateOf({ outcome: 'applied' }))
		await expect(bar(page)).toHaveAttribute('data-stage', 'editing')
		await expect(bar(page)).toContainText('Applied.')
		await expect(page.locator('.candidate .notice.fault')).toHaveCount(0)
		await expect(page.locator('.candidate')).toContainText('Not checked, so')
		await expect(checkedAgain(page)).toBeEnabled()
	})

	test('an edit after the failure is applied with checking', async ({ page }) => {
		await failGateway(page)
		await page.getByLabel('Gateway').fill('192.168.60.1')
		await expect(unchecked(page)).toBeDisabled()
		await expect(page.locator('.candidate')).toContainText('Undo your edits first.')
		// Back to what failed, and it is offered again.
		await page.getByLabel('Gateway').fill('192.168.60.254')
		await expect(unchecked(page)).toBeEnabled()

		await page.getByLabel('Gateway').fill('192.168.60.1')
		await bar(page).getByRole('button', { name: 'Apply', exact: true }).click()
		const [, edited] = await proposals(page)
		expect(edited.document.attachments[1]).toMatchObject({ gateway: '192.168.60.1', verify: true })
	})

	test('a candidate applied without checking is saved not checked', async ({ page }) => {
		await failGateway(page)
		await answer(page, 'configuration', [echo(), stateOf({ outcome: 'applied' })])
		await unchecked(page).click()

		await expect(bar(page)).toContainText('Applied.')
		await expect(row(page, 'North site')).toContainText('not checked')
		await expect(page.getByLabel('Gateway')).toHaveValue('192.168.60.254')
		expect((await sent(page)).map((each) => each.type)).toEqual(['configure', 'configuration', 'configuration'])
	})

	// CFG: a candidate not checked is still brought up, and reported through `state`.
	test('a candidate not checked is shown by what the device observed', async ({ page }) => {
		await failGateway(page)
		await answer(page, 'configuration', [echo(), stateOf({ outcome: 'applied' })])
		await unchecked(page).click()
		await expect(bar(page)).toHaveAttribute('data-stage', 'editing')
		await say(page, stateOf({
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
		await expect(page.locator('.candidate')).toContainText('Not checked, so it applies even if it cannot connect.')

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
	// A device that cannot hold a connection to chosen bands offers none, and the screen offers only
	// what it is offered (WLAN, NSCR).
	test('wireless bands are offered only where the device offers them', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await open(page, 'Clinic-Staff')
		await expect(page.locator('.candidate').getByText('Bands')).toHaveCount(0)
	})

	// Every band on is any band, so nothing is written until one is turned off, and the last stays on.
	test('the bands a device offers are narrowed and proposed', async ({ page }) => {
		const offering = structuredClone(PI)
		offering.document.attachments.kind.wireless.interface.wlan0.bands = ['2ghz', '5ghz']
		await openNetwork(page, { document: IN_FORCE, capabilities: offering })
		await open(page, 'Clinic-Staff')
		const candidate = page.locator('.candidate')
		const low = candidate.getByRole('checkbox', { name: '2.4 GHz' })
		const high = candidate.getByRole('checkbox', { name: '5 GHz' })
		await expect(low).toBeChecked()
		await expect(high).toBeChecked()

		await low.uncheck()
		await expect(high).toBeDisabled()
		await page.getByRole('button', { name: 'Apply' }).click()
		const [proposal] = await proposals(page)
		expect(proposal.document.attachments.find((each) => each.label === 'Clinic-Staff').bands).toEqual(['5ghz'])
	})

	test('every band left on proposes no bands', async ({ page }) => {
		const offering = structuredClone(PI)
		offering.document.attachments.kind.wireless.interface.wlan0.bands = ['2ghz', '5ghz']
		await openNetwork(page, { document: IN_FORCE, capabilities: offering })
		await open(page, 'Clinic-Staff')
		const low = page.locator('.candidate').getByRole('checkbox', { name: '2.4 GHz' })
		await low.uncheck()
		await low.check()
		await page.getByLabel('Country').selectOption('FJ')
		await page.getByRole('button', { name: 'Apply' }).click()
		const [proposal] = await proposals(page)
		expect(proposal.document.attachments.find((each) => each.label === 'Clinic-Staff')).not.toHaveProperty('bands')
	})

	test('a hotspot radio offering no settings is left out without comment', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: UNTUNED })
		await expect(page.getByText('Radio and addressing')).toHaveCount(0)
		await page.getByText('Addressing', { exact: true }).click()
		await expect(page.getByLabel('Band')).toHaveCount(0)
		await expect(page.getByLabel('Channel')).toHaveCount(0)
		await expect(page.getByLabel('Width')).toHaveCount(0)
		await expect(page.getByText('The radio runs the hotspot on the same channel as its wireless connection.')).toHaveCount(0)
		await expect(page.getByLabel('DHCP range')).toBeVisible()
	})

	test('a shared-channel radio offers the hotspot a channel only while no wireless network could share it', async ({ page }) => {
		const wiredOnly = { ...IN_FORCE, attachments: IN_FORCE.attachments.filter((each) => each.kind !== 'wireless') }
		await openNetwork(page, { document: wiredOnly, capabilities: PI })
		const hotspot = page.locator('.hotspot')
		await hotspot.getByText('Radio and addressing').click()
		await hotspot.getByLabel('Band').selectOption('5ghz')
		await hotspot.getByLabel('Channel').selectOption('44')
		await expect(hotspot.getByText('The radio runs the hotspot on the same channel as its wireless connection.')).toHaveCount(0)

		await addWireless(page)
		await expect(hotspot.getByLabel('Band')).toHaveCount(0)
		await expect(hotspot.getByLabel('Channel')).toHaveCount(0)
		await expect(hotspot.getByLabel('Width')).toHaveCount(0)
		await expect(hotspot.getByText('The radio runs the hotspot on the same channel as its wireless connection.')).toBeVisible()
	})

	test('a chosen channel on a shared-channel radio is proposed where no wireless network could share it', async ({ page }) => {
		const wiredOnly = { ...IN_FORCE, attachments: IN_FORCE.attachments.filter((each) => each.kind !== 'wireless') }
		await openNetwork(page, { document: wiredOnly, capabilities: PI })
		const hotspot = page.locator('.hotspot')
		await hotspot.getByText('Radio and addressing').click()
		await hotspot.getByLabel('Band').selectOption('2ghz')
		await hotspot.getByLabel('Channel').selectOption('11')
		await page.getByRole('button', { name: 'Apply' }).click()
		const [proposal] = await proposals(page)
		expect(proposal.document.hotspot).toMatchObject({ band: '2ghz', channel: 11 })
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
		const wired = { attachments: [{ kind: 'wired-dynamic', label: 'eth0 automatic', enabled: true, verify: true, interface: 'eth0' }] }
		await openNetwork(page, { document: wired, capabilities: WIRED_ONLY })
		await expect(page.locator('.hotspot')).toContainText('This device has no wireless radio.')
		await expect(page.locator('section.hotspot').getByRole('button', { name: 'Turn on' })).toHaveCount(0)
		await expect(page.getByRole('heading', { name: 'Country' })).toHaveCount(0)
		await page.getByRole('button', { name: 'Add' }).click()
		await expect(page.locator('.adding button')).toHaveText(['Wired, DHCP', 'Wired, static'])
	})

	test('a document outside the capabilities is not proposed', async ({ page }) => {
		const withEth1 = {
			attachments: [{ kind: 'wired-dynamic', label: 'eth1 automatic', enabled: true, verify: true, interface: 'eth1' }],
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
			enabled: true,
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
				ap({ bssid: '5c:a6:e6:02:71:9b', ssid: 'Guest', security: ['open'], signal: -61, band: '2ghz', channel: 6 }),
			],
		}))
		await addWireless(page)
		await page.locator('.candidate').getByRole('button', { name: 'Scan' }).click()
		await expect(page.getByRole('button', { name: 'Guest', exact: true })).toBeDisabled()
		await page.getByRole('button', { name: 'Clinic', exact: true }).click()
		await expect(page.locator('.candidate').getByLabel('SSID')).toHaveValue('Clinic')
		await expect(page.getByLabel('Security')).toHaveValue('psk-sae')
	})
})

test.describe('the hotspot beside a connection', () => {
	const joinedOn = (number) =>
		message({
			type: 'fact',
			at: 1,
			fact: 'wireless-network',
			kind: 'text',
			value: 'Clinic-Staff',
			traits: { status: { is: 'passed' }, interface: { name: 'wlan0' }, security: 'sae', channel: { band: '5ghz', number } },
		})

	// Told before applying where the adapter's connection is on a channel it cannot start a hotspot
	// on, and still left to the device to decide (NSCR, HOT).
	test('a hotspot that cannot run beside the connection joined now says so, and can still be applied', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await feed(page, joinedOn(136))
		const notice = page.locator('section.hotspot .notice')
		await expect(notice).toHaveText("Can't run beside Clinic-Staff on 5 GHz channel 136; turning that connection off lets it run.")
		await page.locator('section.hotspot').getByLabel('SSID').fill('Clinic-Field-05')
		await expect(page.getByRole('button', { name: 'Apply' })).toBeEnabled()

		await feed(page, joinedOn(36))
		await expect(notice).toHaveCount(0)
	})
})

test.describe('the state of the session', () => {
	test('how the device is faring stays in view while the operator scrolls', async ({ page }) => {
		await page.setViewportSize({ width: 390, height: 640 })
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await answer(page, 'configuration', [echo(), stateOf({ outcome: 'verifying' })])
		await open(page, 'North site')
		await page.getByText('Radio and addressing').click()
		await page.getByLabel('Country').selectOption('FJ')
		await page.getByRole('button', { name: 'Apply' }).click()
		await expect(bar(page)).toHaveAttribute('data-stage', 'applying')

		for (const y of [0, 400, 800]) {
			await page.evaluate((y) => window.scrollTo(0, y), y)
			await expect(bar(page)).toBeInViewport({ ratio: 1 })
			await expect(bar(page)).toContainText('Applying…')
		}
		expect(await page.evaluate(() => document.documentElement.scrollHeight > window.innerHeight)).toBe(true)
	})

	test('the vocabulary of the wire does not appear on screen', async ({ page }) => {
		const document = structuredClone(IN_FORCE)
		document.attachments[4].verify = false
		document.attachments[3].enabled = false
		await openNetwork(page, { document, capabilities: PI, states: STATES })
		await answer(page, 'configuration', failsAt("$['attachments'][1]['gateway']", 'no answer', 'gateway'))
		await page.getByLabel('Country').selectOption('FJ')
		await page.getByRole('button', { name: 'Apply' }).click()
		await expect(bar(page)).toHaveAttribute('data-stage', 'errored')
		await page.getByText('Radio and addressing').click()
		const shown = await page.locator('.network').innerText()
		for (const word of ['invalid', 'configure', 'discard', 'refus', 'cannot', 'wired-static', 'wired-dynamic', 'psk', 'sae', 'regulatory', 'default-route', 'unavailable', 'verif', 'enabled', '$[']) {
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
			enabled: true,
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
		await hotspot.getByLabel('Band').selectOption('2ghz')
		await expect(options(hotspot.getByLabel('Channel'))).toHaveText(['Device picks', '1', '6', '11'])
		await hotspot.getByLabel('Channel').selectOption('6')
		await page.getByRole('button', { name: 'Apply' }).click()
		const [proposal] = await proposals(page)
		expect(proposal.document.hotspot).toMatchObject({ interface: 'wlx00c0caa1b2c3', band: '2ghz', channel: 6 })
	})

	test('a single radio offers no adapter to choose', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await addWireless(page)
		await expect(page.getByLabel('Adapter')).toHaveCount(0)
	})
})
