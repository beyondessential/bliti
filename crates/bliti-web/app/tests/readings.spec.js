// The device view of VIEW, fed decoded facts and readings with no wasm and no Bluetooth in the loop.
//
// The property under test throughout is that what our application recognises renders bespoke, and
// what it does not still renders from the entry's own name, kind and traits.

import { expect, test } from '@playwright/test'

import { emit, openChannel } from './fake-client.js'

const msg = (message) => ({ kind: 'message', message })
const reading = (measurement, extra = {}) =>
	msg({ type: 'reading', at: 1, measurement, traits: { status: { is: 'passed' } }, ...extra })
const fact = (name, extra = {}) =>
	msg({ type: 'fact', at: 1, fact: name, traits: { status: { is: 'passed' } }, ...extra })
const fraction = (value) => ({ kind: 'fraction', value })

test.describe('rendering from the entry alone', () => {
	// The floor the whole format rests on: a device that gained a reading appears in an application
	// that has never heard of it, from its own name and kind (VIEW).
	test('a reading this build has never heard of renders from its own description', async ({ page }) => {
		await openChannel(page)
		await emit(page, reading('radiation', { kind: 'quantity', unit: 'µSv/h', value: 0.12 }))
		await expect(page.getByText('Radiation')).toBeVisible()
		// The unrecognised unit is written out as it was sent (VIEW).
		await expect(page.getByText('0.12 µSv/h')).toBeVisible()
	})

	// An unrecognised kind renders as the stringification of the value, with the unit where there is
	// one (VIEW).
	test('a value of an unknown kind renders stringified with its unit', async ({ page }) => {
		await openChannel(page)
		await emit(page, reading('pressure', { kind: 'barometric', value: 101325, unit: 'pascal' }))
		await expect(page.getByText('101325 pascal')).toBeVisible()
	})

	test('each value kind is rendered in its own terms', async ({ page }) => {
		await openChannel(page)
		await emit(page, reading('cpu-usage', fraction(0.12)))
		await emit(page, reading('temperature', { kind: 'quantity', unit: 'celsius', value: 48.5, traits: { status: { is: 'passed' }, sensor: 'cpu' } }))
		await expect(page.getByText('12%')).toBeVisible()
		await expect(page.getByText('48.5 °C')).toBeVisible()
	})
})

test.describe('the header', () => {
	// The header names the device from hostname, board, os and kernel, which get no tiles (VIEW).
	test('names the device and gives those facts no tiles', async ({ page }) => {
		await openChannel(page)
		await emit(page, fact('hostname', { kind: 'text', value: 'tamanu-iti' }))
		await emit(page, fact('board', { kind: 'text', value: 'Raspberry Pi 5' }))
		await emit(page, fact('board-revision', { kind: 'text', value: 'Rev 1.0' }))
		await emit(page, fact('os', { kind: 'text', value: 'Debian 12' }))
		await emit(page, fact('kernel', { kind: 'text', value: '6.6.20' }))
		await emit(page, reading('cpu-usage', fraction(0.12)))

		await expect(page.locator('.device .name')).toHaveText('tamanu-iti')
		await expect(page.locator('.device .sub')).toContainText('Raspberry Pi 5 Rev 1.0')
		await expect(page.locator('.device .sub')).toContainText('Debian 12')
		// Only the processor is a tile; the four header facts are not.
		await expect(page.locator('.tile')).toHaveCount(1)
	})

	// An unrecognised fact is a tile, not a header entry (VIEW).
	test('an unrecognised fact renders as a tile rather than in the header', async ({ page }) => {
		await openChannel(page)
		await emit(page, fact('hostname', { kind: 'text', value: 'tamanu-iti' }))
		await emit(page, fact('firmware', { kind: 'text', value: '2026-04-17' }))
		await expect(page.locator('.device')).not.toContainText('2026-04-17')
		await expect(page.locator('.tile')).toContainText('2026-04-17')
	})
})

test.describe('the face and the reveal', () => {
	test('a scale is behind the tap, not on the face', async ({ page }) => {
		await openChannel(page)
		await emit(page, reading('cpu-usage', fraction(0.12)))
		await expect(page.locator('.tile .bar')).toHaveCount(0)
		await page.getByRole('button', { name: /Processor/ }).click()
		await expect(page.locator('.tile .bar')).toHaveCount(1)
	})

	// A reading the device declared but could not take shows it failed, with why (VIEW).
	test('a broken reading shows it is unavailable, with its reason', async ({ page }) => {
		await openChannel(page)
		await emit(page, reading('cpu-usage', { kind: 'fraction', traits: { status: { is: 'broken', reason: 'no answer from /proc/stat' } } }))
		await expect(page.getByText('unavailable')).toBeVisible()
		await page.getByRole('button', { name: /Processor/ }).click()
		await expect(page.getByText('no answer from /proc/stat')).toBeVisible()
	})

	// Skipped and broken both have no value and are distinguished: skipped shows a dash, broken shows
	// it is unavailable (VIEW).
	test('a skipped reading is distinguishable from a broken one', async ({ page }) => {
		await openChannel(page)
		await emit(page, reading('humidity', { kind: 'fraction', traits: { status: { is: 'skipped', reason: 'no sensor warmed up' } } }))
		await expect(page.locator('.tile .value').first()).toHaveText('—')
	})

	// A state this build does not know must not make an older application cry wolf.
	test('an unknown status is not treated as trouble', async ({ page }) => {
		await openChannel(page)
		await emit(page, reading('cpu-usage', { kind: 'fraction', value: 0.12, traits: { status: { is: 'degraded' } } }))
		await expect(page.locator('.tile .value').first()).not.toHaveClass(/bad|warn/)
	})

	test('trouble colours the face, and a passed face carries no colour', async ({ page }) => {
		await openChannel(page)
		await emit(page, reading('cpu-usage', fraction(0.12)))
		await emit(page, reading('cpu-frequency', { kind: 'quantity', unit: 'hertz', value: 600000000, traits: { status: { is: 'warning', reason: 'the platform is limiting the processor' } } }))
		await expect(page.locator('.tile .value.warn')).toHaveCount(1)
		await expect(page.locator('.tile .value:not(.warn):not(.bad)')).not.toHaveCount(0)
	})

	// A tile with nothing behind its headline is not a tap target (VIEW).
	test('a tile with nothing to reveal is not tappable', async ({ page }) => {
		await openChannel(page)
		await emit(page, reading('fan-speed', { kind: 'quantity', unit: 'revolutions/minute', value: 3113, traits: { status: { is: 'passed' }, fan: 'cpu' } }))
		await expect(page.getByRole('button', { name: /Fan/ })).toHaveCount(0)
		await expect(page.locator('.tile.flat')).toHaveCount(1)
	})
})

test.describe('aggregation', () => {
	// Storage headlines the fullest filesystem that is not a boot partition, and shows each in the
	// reveal (VIEW).
	test('storage headlines the fullest non-boot filesystem', async ({ page }) => {
		await openChannel(page)
		const fs = (mount, value, role) =>
			reading('filesystem-usage', {
				kind: 'fraction',
				value,
				traits: { status: { is: 'passed' }, filesystem: { mount, device: mount, ...(role ? { role } : {}) } },
			})
		await emit(page, fs('/boot/firmware', 0.94, 'boot'))
		await emit(page, fs('/', 0.78))
		await emit(page, fs('/mnt/data', 0.31))

		// The boot partition is fuller, but the headline is the fullest non-boot one.
		await expect(page.locator('.tile').filter({ hasText: 'Storage' }).locator('.value').first()).toHaveText('78%')
		await page.getByRole('button', { name: /Storage/ }).click()
		await expect(page.getByText('/mnt/data')).toBeVisible()
		await expect(page.getByText('/boot/firmware')).toBeVisible()
	})

	// Network headlines one combined figure; the per-interface split is behind the tap, drawn as a
	// mirrored graph per interface (VIEW).
	test('network sums the headline and draws per-interface mirrored graphs', async ({ page }) => {
		await openChannel(page)
		const throughput = (name, direction, value, at) =>
			msg({
				type: 'reading',
				at,
				measurement: 'network-throughput',
				kind: 'quantity',
				unit: 'bytes/second',
				value,
				traits: { status: { is: 'passed' }, interface: { name, route: name === 'end0' ? 'default' : undefined }, direction },
			})
		for (const at of [1000, 2000]) {
			await emit(page, throughput('end0', 'in', 1200000, at))
			await emit(page, throughput('end0', 'out', 84000, at))
		}

		await expect(page.locator('.tile').filter({ hasText: 'Network' }).locator('.value').first()).toHaveText('1.28 MB/s')
		await page.getByRole('button', { name: /Network/ }).click()
		await expect(page.locator('.mirror')).toBeVisible()
		await expect(page.getByText(/peak 1.2 MB\/s/)).toBeVisible()
		await expect(page.getByText(/peak 84 kB\/s/)).toBeVisible()
	})

	// A failed network direction shows its reason in the reveal, alongside the other interfaces (VIEW).
	test('a broken network direction shows why', async ({ page }) => {
		await openChannel(page)
		const throughput = (name, direction, extra) =>
			msg({ type: 'reading', at: 1, measurement: 'network-throughput', kind: 'quantity', unit: 'bytes/second', traits: { status: { is: 'passed' }, interface: { name }, direction }, ...extra })
		await emit(page, throughput('end0', 'in', { value: 1000 }))
		await emit(page, throughput('end0', 'out', { value: 2000 }))
		await emit(page, throughput('wlan0', 'out', { traits: { status: { is: 'broken', reason: 'no counters while associating' }, interface: { name: 'wlan0' }, direction: 'out' } }))

		await page.getByRole('button', { name: /Network/ }).click()
		await expect(page.getByText('no counters while associating')).toBeVisible()
	})

	// Address headlines the default-route address together with the overlay address, and shows every
	// address in the reveal (VIEW).
	test('address headlines the default route and the overlay, and reveals all', async ({ page }) => {
		await openChannel(page)
		const addr = (name, value, extra) =>
			fact('network-address', { kind: 'ipv4', value, traits: { status: { is: 'passed' }, interface: { name, ...extra } } })
		await emit(page, addr('end0', '192.168.1.42', { route: 'default' }))
		await emit(page, addr('tailscale0', '100.101.102.103', { overlay: 'tailscale' }))
		await emit(page, addr('wlan0', '10.0.0.5', {}))

		const tile = page.locator('.tile').filter({ hasText: 'Address' })
		await expect(tile).toContainText('192.168.1.42')
		await expect(tile).toContainText('100.101.102.103')
		await tile.click()
		await expect(page.getByText('10.0.0.5')).toBeVisible()
	})
})

test.describe('history', () => {
	// A history accumulates for a reading, and a graph is drawn from it (VIEW).
	test('a reading accumulates a history and draws a graph', async ({ page }) => {
		await openChannel(page)
		await emit(page, msg({ type: 'reading', at: 1000, measurement: 'cpu-usage', traits: { status: { is: 'passed' } }, kind: 'fraction', value: 0.1 }))
		await emit(page, msg({ type: 'reading', at: 2000, measurement: 'cpu-usage', traits: { status: { is: 'passed' } }, kind: 'fraction', value: 0.5 }))
		await page.getByRole('button', { name: /Processor/ }).click()
		await expect(page.locator('.tile .spark polyline').first()).toBeVisible()
	})

	// A later reading replaces the headline (VIEW).
	test('a later reading replaces the headline', async ({ page }) => {
		await openChannel(page)
		await emit(page, msg({ type: 'reading', at: 1000, measurement: 'cpu-usage', traits: { status: { is: 'passed' } }, kind: 'fraction', value: 0.1 }))
		await expect(page.getByText('10%')).toBeVisible()
		await emit(page, msg({ type: 'reading', at: 2000, measurement: 'cpu-usage', traits: { status: { is: 'passed' } }, kind: 'fraction', value: 0.9 }))
		await expect(page.getByText('90%')).toBeVisible()
		await expect(page.getByText('10%')).toBeHidden()
	})
})

test.describe('what it does not recognise', () => {
	// Two unrecognised readings alike but for their traits render distinguishably, with the trait
	// values as the qualifier rather than dropped (VIEW).
	test('two unknown readings differing only in traits are told apart', async ({ page }) => {
		await openChannel(page)
		const modem = (tech, value) =>
			reading('modem-signal', { kind: 'quantity', unit: 'decibel-milliwatts', value, traits: { status: { is: 'passed' }, modem: { name: tech === 'lte' ? 'cdc-wdm0' : 'cdc-wdm1', technology: tech } } })
		await emit(page, modem('lte', -71))
		await emit(page, modem('nr5g', -84))
		await expect(page.getByText('-71 decibel-milliwatts')).toBeVisible()
		await expect(page.getByText('-84 decibel-milliwatts')).toBeVisible()
		await expect(page.locator('.tile').filter({ hasText: 'cdc-wdm0' })).toHaveCount(1)
		await expect(page.locator('.tile').filter({ hasText: 'cdc-wdm1' })).toHaveCount(1)
	})
})
