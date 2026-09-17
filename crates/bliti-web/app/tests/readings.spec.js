// The diagnostics view of BLI-SYS, fed decoded readings with no wasm and no Bluetooth in the loop.
//
// The property under test throughout is that nothing here is matched by name. Every case below feeds
// a reading this application has never been told about and expects it rendered from what it says
// about itself.

import { expect, test } from '@playwright/test'

import { emit, openChannel } from './fake-client.js'

const identity = (readings) => ({ kind: 'message', message: { type: 'system-identity', readings } })
const sample = (at, readings) => ({
	kind: 'message',
	message: { type: 'system-sample', at, readings },
})
const history = (samples) => ({ kind: 'message', message: { type: 'system-history', samples } })

const fraction = (number) => ({ kind: 'fraction', number })

test.describe('rendering from the reading alone', () => {
	/// The floor the whole format rests on: a device that gained a reading appears in an application
	/// that has never heard of it, with no release in between.
	test('a reading this build has never heard of is rendered from its own description', async ({
		page,
	}) => {
		await openChannel(page)
		await emit(
			page,
			identity([
				{
					name: 'radiation',
					label: 'Radiation',
					value: { kind: 'quantity', number: 0.12, unit: 'µSv/h' },
				},
			]),
		)
		await expect(page.getByText('Radiation')).toBeVisible()
		await expect(page.getByText('0.12 µSv/h')).toBeVisible()
	})

	/// A kind is not a name: an unknown one leaves the reading readable as a label rather than
	/// inventing a rendering for a unit whose meaning this build does not know.
	test('a value of an unknown kind leaves the label readable', async ({ page }) => {
		await openChannel(page)
		await emit(
			page,
			identity([{ name: 'pressure', label: 'Pressure', value: { kind: 'barometric', pascals: 101325 } }]),
		)
		await expect(page.getByText('Pressure')).toBeVisible()
		await expect(page.getByText('not understood')).toBeVisible()
	})

	test('each value kind is rendered in its own terms', async ({ page }) => {
		await openChannel(page)
		await emit(
			page,
			identity([
				{ name: 'cpu', label: 'CPU', value: fraction(0.12) },
				{ name: 'uptime', label: 'Uptime', value: { kind: 'duration', seconds: 20308 } },
				{ name: 'os', label: 'OS', value: { kind: 'text', text: 'Ubuntu 26.04 LTS' } },
			]),
		)
		await expect(page.getByText('12%')).toBeVisible()
		await expect(page.getByText('5h 38m')).toBeVisible()
		await expect(page.getByText('Ubuntu 26.04 LTS')).toBeVisible()
	})
})

test.describe('the face and the reveal', () => {
	test('detail, note and limits are behind the tap rather than on the face', async ({ page }) => {
		await openChannel(page)
		await emit(
			page,
			identity([
				{
					name: 'temperature',
					label: 'Temperature',
					value: { kind: 'quantity', number: 48.5, unit: '°C', max: 110 },
					detail: [{ label: 'Disk', value: { kind: 'quantity', number: 37.8, unit: '°C' } }],
					limits: [{ at: 75, label: 'Cooling' }],
					note: 'This is the CPU core, not the case or the room.',
				},
			]),
		)

		// The face carries the label and the headline, and nothing else.
		await expect(page.getByText('48.5 °C')).toBeVisible()
		await expect(page.getByText('This is the CPU core')).toBeHidden()
		await expect(page.getByText('Disk')).toBeHidden()

		await page.getByRole('button', { name: /Temperature/ }).click()
		await expect(page.getByText('This is the CPU core')).toBeVisible()
		await expect(page.getByText('Disk')).toBeVisible()
		await expect(page.getByText('Cooling')).toBeVisible()
	})

	/// A reading the device declares but could not take is a fault nobody can see from outside the
	/// case, so it is shown as failing with its reason rather than quietly left out.
	test('a reading that failed says so, with why', async ({ page }) => {
		await openChannel(page)
		await emit(
			page,
			identity([
				{
					name: 'battery',
					label: 'Battery',
					state: 'fault',
					error: 'no answer from the gauge at 0x36',
				},
			]),
		)
		await expect(page.getByText('unavailable')).toBeVisible()

		await page.getByRole('button', { name: /Battery/ }).click()
		await expect(page.getByText('no answer from the gauge at 0x36')).toBeVisible()
	})

	/// A reading the device never declared is not an absence the application can observe: it holds no
	/// list of expected readings to miss one from.
	test('a reading that never arrived is not reported as missing', async ({ page }) => {
		await openChannel(page)
		await emit(page, identity([{ name: 'cpu', label: 'CPU', value: fraction(0.12) }]))
		await expect(page.getByText('12%')).toBeVisible()
		// Scoped to the tiles: the device's own version line says "not reported" before a hello
		// arrives, and that is about the software rather than about a reading.
		await expect(page.locator('.tiles').getByText(/missing|unavailable/i)).toBeHidden()
		await expect(page.locator('.tile')).toHaveCount(1)
	})

	/// A state this build does not know must not make an older application cry wolf.
	test('an unknown state is not treated as trouble', async ({ page }) => {
		await openChannel(page)
		await emit(
			page,
			identity([{ name: 'cpu', label: 'CPU', state: 'degraded', value: fraction(0.12) }]),
		)
		const value = page.locator('.tile .value').first()
		await expect(value).not.toHaveClass(/bad|warn/)
	})

	test('trouble colours the face', async ({ page }) => {
		await openChannel(page)
		await emit(page, identity([{ name: 'disk', label: 'Disk', state: 'warn', value: fraction(0.97) }]))
		await expect(page.locator('.tile .value.warn')).toBeVisible()
	})
})

test.describe('grouping and scales', () => {
	/// Grouping is an improvement on the floor, never part of it: ignoring it would show the members
	/// separately and still be correct.
	test('readings sharing a group become one tile', async ({ page }) => {
		await openChannel(page)
		await emit(
			page,
			identity([
				{
					name: 'network-in-end0',
					label: 'end0 in',
					group: 'network-throughput',
					direction: 'in',
					value: { kind: 'quantity', number: 1.2, unit: 'MB/s' },
				},
				{
					name: 'network-out-end0',
					label: 'end0 out',
					group: 'network-throughput',
					direction: 'out',
					value: { kind: 'quantity', number: 84, unit: 'kB/s' },
				},
			]),
		)
		await expect(page.locator('.tile')).toHaveCount(1)
		await expect(page.getByText('1.2 MB/s')).toBeVisible()
		await expect(page.getByText('84 kB/s')).toBeVisible()
	})

	/// A quantity that named no ceiling is never drawn against one: the bar would imply a maximum the
	/// device never claimed.
	test('a bar is drawn only where the reading has a scale', async ({ page }) => {
		await openChannel(page)
		await emit(
			page,
			identity([
				{ name: 'cpu', label: 'CPU', value: fraction(0.12) },
				{ name: 'fan', label: 'Fan', value: { kind: 'quantity', number: 3113, unit: 'rpm' } },
			]),
		)

		await page.getByRole('button', { name: /CPU/ }).click()
		await expect(page.locator('.tile .bar')).toHaveCount(1)
	})

	/// A tile with nothing behind its headline is not a tap target. Offering one that does nothing
	/// teaches an operator that tapping is not worth trying.
	test('a tile with nothing to reveal is not tappable', async ({ page }) => {
		await openChannel(page)
		await emit(
			page,
			identity([
				// A ceiling gives the CPU a bar to reveal; the fan has no ceiling, no detail and no
				// history, so there is nothing behind it.
				{ name: 'cpu', label: 'CPU', value: fraction(0.12) },
				{ name: 'fan', label: 'Fan', value: { kind: 'quantity', number: 3113, unit: 'rpm' } },
			]),
		)
		await expect(page.getByRole('button', { name: /CPU/ })).toBeVisible()
		await expect(page.getByRole('button', { name: /Fan/ })).toHaveCount(0)
		await expect(page.locator('.tile.flat')).toHaveCount(1)
	})

	/// Text runs long where a number does not, so a hostname or a board name takes the full row
	/// rather than wrapping inside half of one.
	test('a long headline takes the full width', async ({ page }) => {
		await openChannel(page)
		await emit(
			page,
			identity([
				{ name: 'board', label: 'Board', value: { kind: 'text', text: 'Raspberry Pi 5 Model B Rev 1.1' } },
				{ name: 'cpu', label: 'CPU', value: fraction(0.12) },
			]),
		)
		await expect(page.locator('.tile.wide')).toHaveCount(1)
		await expect(page.locator('.tile.wide')).toContainText('Raspberry Pi 5')
	})
})

test.describe('history', () => {
	/// The window arrives before anything live, so a graph is populated the moment it appears rather
	/// than filling from empty while an operator waits.
	test('a graph is drawn from the window sent on subscribing', async ({ page }) => {
		await openChannel(page)
		await emit(
			page,
			history([
				{ at: 1000, readings: [{ name: 'cpu', label: 'CPU', value: fraction(0.1) }] },
				{ at: 2000, readings: [{ name: 'cpu', label: 'CPU', value: fraction(0.5) }] },
				{ at: 3000, readings: [{ name: 'cpu', label: 'CPU', value: fraction(0.2) }] },
			]),
		)
		await page.getByRole('button', { name: /CPU/ }).click()
		await expect(page.locator('.tile .spark polyline')).toBeVisible()
	})

	test('a later sample replaces the headline and extends the graph', async ({ page }) => {
		await openChannel(page)
		await emit(page, sample(1000, [{ name: 'cpu', label: 'CPU', value: fraction(0.1) }]))
		await expect(page.getByText('10%')).toBeVisible()

		await emit(page, sample(2000, [{ name: 'cpu', label: 'CPU', value: fraction(0.9) }]))
		await expect(page.getByText('90%')).toBeVisible()
		await expect(page.getByText('10%')).toBeHidden()
	})

	/// Two opposed flows are read together, about one axis, rather than as two separate charts.
	test('opposed directions draw as one mirrored graph with both peaks stated', async ({ page }) => {
		await openChannel(page)
		const pair = (inbound, outbound) => [
			{
				name: 'network-in-end0',
				label: 'in',
				group: 'network-throughput',
				direction: 'in',
				value: { kind: 'quantity', number: inbound, unit: 'MB/s' },
			},
			{
				name: 'network-out-end0',
				label: 'out',
				group: 'network-throughput',
				direction: 'out',
				value: { kind: 'quantity', number: outbound, unit: 'MB/s' },
			},
		]
		await emit(
			page,
			history([
				{ at: 1000, readings: pair(1.0, 0.05) },
				{ at: 2000, readings: pair(2.8, 0.08) },
			]),
		)

		await page.locator('.tile').first().click()
		await expect(page.locator('.mirror')).toBeVisible()
		// Each side is scaled to its own peak, so both peaks are stated rather than implied.
		await expect(page.getByText(/peak 2.8 MB\/s/)).toBeVisible()
		await expect(page.getByText(/peak 0.08 MB\/s/)).toBeVisible()
	})
})
