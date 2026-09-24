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
		await page.getByRole('button', { name: /^Network \d/ }).click()
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

		await page.getByRole('button', { name: /^Network \d/ }).click()
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

	// Found on the prototype: an interface's addresses shared one key, so only the last one sampled
	// showed. Each is its own entry, and one that ends goes alone (NFO, VIEW).
	test('every address an interface holds is shown, and one that ends goes alone', async ({ page }) => {
		await openChannel(page)
		const addr = (kind, value, is = 'passed') =>
			fact('network-address', {
				kind,
				value,
				traits: { status: { is, ...(is === 'ended' ? { reason: 'no longer applies' } : {}) }, interface: { name: 'end0', route: 'default' } },
			})
		await emit(page, addr('ipv4', '10.0.101.3'))
		await emit(page, addr('ipv6', '2407:8b00::3'))
		await emit(page, addr('ipv6', 'fd6d::3'))

		const tile = page.locator('.tile').filter({ hasText: 'Address' })
		await expect(tile).toContainText('10.0.101.3')
		await expect(tile).toContainText('2407:8b00::3')
		await expect(tile).toContainText('fd6d::3')

		await emit(page, addr('ipv6', '2407:8b00::3', 'ended'))
		await expect(tile).not.toContainText('2407:8b00::3')
		await expect(tile).toContainText('10.0.101.3')
		await expect(tile).toContainText('fd6d::3')
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

test.describe('batteries', () => {
	const battery = (name, extra = {}) => ({ name, ...extra })
	const charge = (about, value) =>
		reading('battery-charge', { kind: 'fraction', value, traits: { status: { is: 'passed' }, battery: about } })
	const volts = (about, value) =>
		reading('battery-voltage', { kind: 'quantity', unit: 'volts', value, traits: { status: { is: 'passed' }, battery: about } })
	const noVolts = (about) =>
		reading('battery-voltage', {
			kind: 'quantity',
			traits: { status: { is: 'skipped', reason: 'the battery reports no cell voltage' }, battery: about },
		})
	const direction = (about, value) =>
		reading('battery-direction', { kind: 'text', value, traits: { status: { is: 'passed' }, battery: about } })

	// One battery is the ordinary case, and keeps the plain reading tile with its voltage and
	// direction folded in beneath (VIEW).
	test('a single battery folds its voltage and direction into the reveal', async ({ page }) => {
		await openChannel(page)
		const about = battery('DELL T453X', { model: 'DELL T453X', vendor: 'LGC', serial: '109' })
		await emit(page, charge(about, 1))
		await emit(page, volts(about, 12.887))
		await emit(page, direction(about, 'idle'))

		await expect(page.locator('.tile').filter({ hasText: 'Battery' }).locator('.value').first()).toHaveText('100%')
		await page.getByRole('button', { name: /Battery/ }).click()
		await expect(page.getByText('12.89 V')).toBeVisible()
		await expect(page.getByText('Idle')).toBeVisible()
	})

	// The built-in cell is the headline whatever else is attached, and every battery is in the
	// reveal with its own voltage and direction (VIEW).
	test('the built-in cell headlines and every battery is revealed', async ({ page }) => {
		await openChannel(page)
		const internal = battery('built-in', { vendor: 'SupTronics' })
		const ups = battery('Eaton 3S', { model: 'Eaton 3S', vendor: 'Eaton' })
		await emit(page, charge(ups, 1))
		await emit(page, charge(internal, 0.62))
		await emit(page, volts(internal, 4.156))
		await emit(page, direction(internal, 'discharging'))
		await emit(page, noVolts(ups))
		await emit(page, direction(ups, 'idle'))

		// The UPS arrived first and sorts earlier, and the built-in cell still headlines.
		await expect(page.locator('.tile').filter({ hasText: 'Battery' }).locator('.value').first()).toHaveText('62%')
		await page.getByRole('button', { name: /Battery/ }).click()
		await expect(page.getByText('built-in')).toBeVisible()
		await expect(page.getByText('Eaton 3S')).toBeVisible()
		await expect(page.getByText('4.16 V')).toBeVisible()
		await expect(page.getByText('Discharging', { exact: true })).toBeVisible()
	})

	// Each battery's voltage belongs to that battery, not to whichever arrived first (VIEW).
	test('voltage and direction pair with their own battery', async ({ page }) => {
		await openChannel(page)
		const first = battery('DELL T453X')
		const second = battery('Eaton 3S')
		await emit(page, charge(first, 1))
		await emit(page, charge(second, 0.5))
		await emit(page, volts(first, 12.887))
		await emit(page, noVolts(second))
		await emit(page, direction(first, 'charging'))
		await emit(page, direction(second, 'discharging'))

		await page.getByRole('button', { name: /Battery/ }).click()
		// The skipped voltage says why rather than borrowing the other battery's figure.
		await expect(page.getByText('the battery reports no cell voltage')).toBeVisible()
		await expect(page.getByText('12.89 V')).toHaveCount(1)
		// Exact, because a substring match for "Charging" also finds "Discharging".
		await expect(page.getByText('Charging', { exact: true })).toBeVisible()
		await expect(page.getByText('Discharging', { exact: true })).toBeVisible()
	})

	// With no built-in cell the headline is settled by name rather than by arrival order (VIEW).
	test('with no built-in cell the first by name headlines', async ({ page }) => {
		await openChannel(page)
		await emit(page, charge(battery('Eaton 3S'), 0.5))
		await emit(page, charge(battery('APC Back-UPS'), 0.9))
		await expect(page.locator('.tile').filter({ hasText: 'Battery' }).locator('.value').first()).toHaveText('90%')
	})

	// A battery's serial, model and vendor describe the cell and do not split its history; only its
	// name says which battery it is (VIEW).
	test('a changing model does not split one battery into two', async ({ page }) => {
		await openChannel(page)
		await emit(page, charge(battery('BAT0', { model: 'DELL T453X' }), 0.5))
		await emit(page, charge(battery('BAT0', { model: 'DELL T453X (refurbished)' }), 0.9))
		await expect(page.locator('.tile').filter({ hasText: 'Battery' })).toHaveCount(1)
		await expect(page.locator('.tile').filter({ hasText: 'Battery' }).locator('.value').first()).toHaveText('90%')
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

test.describe('wireless', () => {
	const joined = (number, band) =>
		fact('wireless-network', {
			kind: 'text',
			value: 'Clinic-Staff',
			traits: { status: { is: 'passed' }, security: 'sae', channel: { number, band, width: 20 } },
		})

	// `security` and `channel` are descriptive (NFO), so a link whose channel moves is the same link:
	// a shared-channel hotspot following the client onto a new channel must not leave two tiles (VIEW).
	test('a wireless-network fact whose channel changes replaces its tile rather than adding a second', async ({ page }) => {
		await openChannel(page)
		await emit(page, joined(6, '2ghz'))
		await emit(page, joined(36, '5ghz'))
		const tile = page.locator('.tile').filter({ hasText: 'Wireless' })
		await expect(tile).toHaveCount(1)
		await tile.click()
		await expect(tile).toContainText('36 · 5 GHz · 20 MHz')
		await expect(tile).not.toContainText('2.4 GHz')
		await expect(tile).toContainText('WPA3')
	})

	// The new entries take the places VIEW's order gives them, after the address and before the processor.
	test('the wireless and hotspot tiles sit where the order puts them', async ({ page }) => {
		await openChannel(page)
		await emit(page, reading('cpu-usage', fraction(0.12)))
		await emit(page, reading('hotspot-clients', { kind: 'quantity', unit: 'clients', value: 3 }))
		await emit(page, fact('hotspot', { kind: 'text', value: 'iti-setup', traits: { status: { is: 'passed' }, channel: { number: 6 } } }))
		await emit(page, joined(6, '2ghz'))
		await emit(page, fact('network-address', { kind: 'ipv4', value: '10.0.0.5', traits: { status: { is: 'passed' }, interface: { name: 'wlan0', route: 'default' } } }))
		await expect(page.locator('.tile .label')).toHaveText(['Address', 'Wireless', 'Hotspot', 'Hotspot clients', 'Processor'])
		await expect(page.getByText('3 clients')).toBeVisible()
	})

	// Leaving an entry out says nothing to a client already showing it, so a stopped hotspot is sent
	// once more as ended and its tiles go (NFO, VIEW).
	test('a hotspot that has stopped loses its tiles', async ({ page }) => {
		await openChannel(page)
		await emit(page, reading('cpu-usage', fraction(0.12)))
		await emit(page, fact('hotspot', { kind: 'text', value: 'iti-setup', traits: { status: { is: 'passed' }, channel: { number: 6 } } }))
		await emit(page, reading('hotspot-clients', { kind: 'quantity', unit: 'clients', value: 3 }))
		await expect(page.locator('.tile .label')).toHaveText(['Hotspot', 'Hotspot clients', 'Processor'])

		const ended = { status: { is: 'ended', reason: 'no longer applies' } }
		await emit(page, fact('hotspot', { kind: 'text', traits: { ...ended, channel: { number: 11 } } }))
		await emit(page, reading('hotspot-clients', { kind: 'quantity', unit: 'clients', traits: ended }))
		await expect(page.locator('.tile .label')).toHaveText(['Processor'])
	})

	// A proposal being tried shows as such on the network tiles, and gets no tile of its own (VIEW).
	test('settings being tried mark the network tiles and say they revert', async ({ page }) => {
		await openChannel(page)
		await emit(page, reading('cpu-usage', fraction(0.12)))
		await emit(page, fact('hotspot', { kind: 'text', value: 'iti-setup', traits: { status: { is: 'passed' }, channel: { number: 6 } } }))
		await emit(page, fact('network-configuration', { kind: 'text', value: 'provisional' }))
		await expect(page.getByText('They revert unless confirmed')).toBeVisible()
		await expect(page.locator('.tile.provisional')).toHaveCount(1)
		await expect(page.locator('.tile.provisional')).toContainText('Hotspot')
		await expect(page.locator('.tile .label')).toHaveText(['Hotspottrial', 'Processor'])

		await emit(page, fact('network-configuration', { kind: 'text', value: 'recorded' }))
		await expect(page.getByText('They revert unless confirmed')).toHaveCount(0)
		await expect(page.locator('.tile.provisional')).toHaveCount(0)
	})
})
