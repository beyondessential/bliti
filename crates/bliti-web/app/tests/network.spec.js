// The network configuration screen of NSCR, driven through a fake device session with no wasm and no
// Bluetooth in the loop. What the page sends is recorded, so "nothing is proposed" is asserted on the
// wire rather than inferred from the screen.

import { expect, test } from '@playwright/test'

import { answer, message, openChannel, openNetwork, say, sent } from './fake-client.js'

// A Raspberry Pi 5 as the wire shape proposes it: a shared-channel radio, one wall port.
const PI = {
	document: {
		attachments: {
			wireless: {
				security: { psk: true, sae: true, 'psk-sae': true, enterprise: { eap: ['peap', 'ttls', 'tls'] } },
				hidden: true,
				nameservers: true,
			},
			'wired-dynamic': { interface: ['eth0'], nameservers: true },
			'wired-static': { interface: ['eth0'], nameservers: true },
		},
		hotspot: { 'share-upstream': true, 'isolate-clients': true, 'dhcp-range': true },
		'regulatory-domain': true,
	},
	radio: { alongside: 'shared-channel' },
	acts: { scan: true, wps: ['push-button', 'pin'] },
}

// Two radios on independent channels.
const INDEPENDENT = {
	...PI,
	document: {
		...PI.document,
		hotspot: {
			'share-upstream': true,
			'isolate-clients': true,
			band: ['2.4ghz', '5ghz'],
			channel: { '2.4ghz': [1, 6, 11], '5ghz': [36, 40, 44, 48] },
			'channel-width': { '2.4ghz': [20], '5ghz': [20, 40, 80] },
		},
	},
	radio: { alongside: 'independent' },
	acts: { scan: true, survey: true, wps: ['push-button'] },
}

const WIRED_ONLY = {
	document: {
		attachments: {
			'wired-dynamic': { interface: ['eth0', 'eth1'], nameservers: true },
			'wired-static': { interface: ['eth0', 'eth1'], nameservers: true },
		},
	},
	acts: {},
}

const IN_FORCE = {
	attachments: [
		{ kind: 'wired-static', label: 'Clinic wall port', interface: 'eth0', addresses: ['10.4.2.20/24'], gateway: '10.4.2.1' },
		{ kind: 'wired-static', label: 'North site', interface: 'eth0', addresses: ['192.168.60.20/24'], gateway: '192.168.60.1', nameservers: ['192.168.60.1'] },
		{ kind: 'wired-dynamic', label: 'eth0 automatic', interface: 'eth0' },
		{ kind: 'wireless', label: 'Clinic-Staff', ssid: 'Clinic-Staff', security: { kind: 'sae', passphrase: 'correct horse battery' } },
		{ kind: 'wireless', label: 'BackupLink', ssid: 'BackupLink', security: { kind: 'psk', passphrase: 'backup-link-77' } },
	],
	hotspot: { ssid: 'Clinic-Field-04', passphrase: 'ripe-anchor-glass-77', 'share-upstream': true, 'isolate-clients': true },
	'regulatory-domain': 'VU',
}

const STATES = [
	{ is: 'default-route' },
	{ is: 'unavailable', reached: 'gateway', reason: '192.168.60.1 did not answer' },
	{ is: 'unavailable', reached: 'addressing', reason: 'no DHCP offer on eth0' },
	{ is: 'up' },
	{ is: 'unavailable', reached: 'carrier', reason: 'BackupLink is not in range' },
]

const bar = (page) => page.locator('.bar-state')
const row = (page, name) => page.locator('.order li').filter({ hasText: name })
const open = (page, name) => row(page, name).locator('.what').click()
const proposals = async (page) => (await sent(page)).filter((each) => each.type === 'configuration')

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
		const wired = { attachments: [{ kind: 'wired-dynamic', label: 'eth0 automatic', interface: 'eth0' }] }
		await openNetwork(page, { document: wired, capabilities: WIRED_ONLY })
		await expect(page.locator('.hotspot')).toContainText('This device has no wireless radio.')
		await expect(page.getByRole('button', { name: 'Turn on' })).toHaveCount(0)
		await expect(page.getByRole('heading', { name: 'Country' })).toHaveCount(0)
		await page.getByRole('button', { name: 'Add' }).click()
		await expect(page.locator('.adding button')).toHaveText(['Wired, DHCP', 'Wired, static'])
	})

	test('a document outside the capabilities is not proposed', async ({ page }) => {
		const withEth1 = {
			attachments: [{ kind: 'wired-dynamic', label: 'eth1 automatic', interface: 'eth1' }],
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
			interface: 'eth0',
			addresses: ['172.16.4.20/24'],
			gateway: '172.16.4.1',
		})
	})

	test('a scanned network fills the SSID, and one the device will not join cannot be picked', async ({ page }) => {
		await openNetwork(page, { document: IN_FORCE, capabilities: PI })
		await answer(page, 'scan', message({
			type: 'networks',
			networks: [
				{ ssid: 'Clinic', security: ['psk', 'sae'], signal: -52, band: '5ghz', channel: 36 },
				{ ssid: 'Guest', security: ['open'], signal: -61, band: '2.4ghz', channel: 6 },
			],
		}))
		await page.getByRole('button', { name: 'Add' }).click()
		await page.getByRole('button', { name: 'Wireless', exact: true }).click()
		await page.getByRole('button', { name: 'Scan' }).click()
		await expect(page.getByRole('button', { name: 'Guest' })).toBeDisabled()
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
		await openNetwork(page, { document: IN_FORCE, capabilities: PI, states: STATES })
		await answer(page, 'configuration', message({ type: 'invalid', at: "$['attachments'][1]['gateway']", reason: 'no answer', reached: 'gateway' }))
		await page.getByLabel('Country').selectOption('FJ')
		await page.getByRole('button', { name: 'Apply' }).click()
		await page.getByText('Radio and addressing').click()
		const shown = await page.locator('.network').innerText()
		for (const word of ['invalid', 'configure', 'discard', 'wired-static', 'wired-dynamic', 'psk', 'sae', 'regulatory', 'default-route', 'unavailable', '$[']) {
			expect(shown.toLowerCase()).not.toContain(word)
		}
	})
})
