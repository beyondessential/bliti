// The Network and Cable screens of NSET: the ways the device is on a network, and moving to the cable
// once one is plugged in.

import { expect, test } from '@playwright/test'

import { PI } from './network-fixtures.js'
import { UNNAMED, answer, feed, message, say, sent, stateOf } from './fake-client.js'
import {
	BOTH,
	ON_CABLE,
	ON_WIFI,
	OUTSIDE,
	SHARING,
	address,
	carrier,
	clients,
	fact,
	joined,
	openSetup,
	openWay,
	proposals,
	title,
	way,
} from './setup-fixtures.js'

const ways = (page) => page.locator('.ways button')
const marked = (page) => page.locator('.ways button.on .name')

test.describe('the Network screen', () => {
	test('says phones and laptops must share the network, and offers the three ways in order', async ({ page }) => {
		await openSetup(page, { document: ON_CABLE, capabilities: PI, facts: [carrier(), address('eth0')] })
		await expect(page.getByText('Phones and laptops must be on the same network as the Iti.')).toBeVisible()
		await expect(page.locator('.ways .name')).toHaveText(['Cable', 'Wifi', 'Iti hotspot'])
		await expect(marked(page)).toHaveText(['Cable'])
		await expect(page.locator('.ways .now')).toHaveText(['Connected', 'Off', 'Off'])
	})

	test('marks the cable and the hotspot beside it, with the devices joined', async ({ page }) => {
		await openSetup(page, { document: SHARING, capabilities: PI, facts: [carrier(), address('eth0'), clients(3)] })
		await expect(marked(page)).toHaveText(['Cable', 'Iti hotspot'])
		await expect(way(page, 'Iti hotspot').locator('.now')).toHaveText('On·3 devices')
	})

	test('names the network wifi is connected to, saying nothing of a cable plugged in beside it', async ({ page }) => {
		await openSetup(page, {
			document: ON_WIFI,
			capabilities: PI,
			facts: [carrier(), joined('Clinic-Staff'), address('wlan0', '10.0.0.4')],
		})
		await expect(marked(page)).toHaveText(['Wifi'])
		await expect(page.locator('.ways .now')).toHaveText(['Off', 'Connected·Clinic-Staff', 'Off'])
	})

	test('marks only the one in use where the cable and wifi are both on', async ({ page }) => {
		const routed = fact('network-address', '10.0.0.4', { interface: { name: 'wlan0', route: 'default' } }, 'ipv4')
		await openSetup(page, { document: BOTH, capabilities: PI, facts: [carrier(), joined('Clinic-Staff'), routed] })
		await expect(marked(page)).toHaveText(['Wifi'])
		await expect(page.locator('.ways .now')).toHaveText(['On', 'Connected·Clinic-Staff', 'Off'])
	})

	test('follows the facts as they change', async ({ page }) => {
		await openSetup(page, { document: ON_CABLE, capabilities: PI, facts: [carrier(false)] })
		await expect(way(page, 'Cable').locator('.now')).toHaveText('No cable')
		await feed(page, carrier(true))
		await expect(way(page, 'Cable').locator('.now')).toHaveText('Plugged in, no connection')
	})

	test('offers only the network settings where the configuration is set up under Advanced', async ({ page }) => {
		await openSetup(page, { document: OUTSIDE, capabilities: PI, facts: [carrier(), address('eth0')] })
		await expect(page.getByText('Set up under Advanced')).toBeVisible()
		await expect(ways(page)).toHaveCount(0)
		await page.getByRole('button', { name: 'Open network settings' }).click()
		await expect(title(page)).toHaveText('Network settings')
		expect((await page.evaluate(() => window.__blitiSessions.map((each) => each.closedByPage)))[0]).toBe(true)
	})

	test('says where the session ended, and opens it again', async ({ page }) => {
		await openSetup(page, { document: ON_CABLE, capabilities: PI })
		await page.evaluate(() => window.__blitiSession.close('stream reset'))
		await expect(page.getByText('The Iti stopped answering: stream reset')).toBeVisible()
		await answer(page, 'configure', [message({ type: 'configuration', document: ON_CABLE, capabilities: PI }), stateOf()])
		await page.getByRole('button', { name: 'Open again' }).click()
		await expect(page.locator('.ways .name')).toHaveText(['Cable', 'Wifi', 'Iti hotspot'])
	})
})

test.describe('the Cable screen', () => {
	test('waits for a cable, and proposes nothing until one is in', async ({ page }) => {
		await openSetup(page, { document: ON_WIFI, capabilities: PI, facts: [joined('Clinic-Staff'), address('wlan0')] })
		await openWay(page, 'Cable')
		await expect(title(page)).toHaveText('Cable')
		await expect(page.getByText('Plug in a network cable')).toBeVisible()
		await expect(page.getByText('Waiting for a cable…')).toBeVisible()
		await page.waitForTimeout(100)
		expect(await proposals(page)).toEqual([])
	})

	test('uses the cable once one is plugged in, leaving wifi on, and returns to the Status screen', async ({ page }) => {
		await openSetup(page, { document: ON_WIFI, capabilities: PI, facts: [joined('Clinic-Staff'), address('wlan0')] })
		await answer(page, 'configuration', [{ echo: true }, stateOf({ outcome: 'verifying' })])
		await openWay(page, 'Cable')
		await feed(page, carrier(true))
		await expect(page.getByText('Switching to the cable')).toBeVisible()
		await expect(page.getByRole('button', { name: 'Cancel' })).toHaveCount(0)
		const [proposed] = await proposals(page)
		expect(proposed.attachments.map((each) => [each.kind, each.enabled])).toEqual([
			['wired-dynamic', true],
			['wireless', true],
		])
		expect(proposed.hotspot).toEqual(ON_WIFI.hotspot)
		await say(page, stateOf({ outcome: 'applied' }))
		await expect(title(page)).toHaveText(UNNAMED)
	})

	test('keeps the hotspot as it is when using the cable', async ({ page }) => {
		await openSetup(page, { document: ON_WIFI, capabilities: PI, facts: [carrier(), joined('Clinic-Staff')] })
		await openWay(page, 'Cable')
		await expect.poll(async () => (await proposals(page)).length).toBe(1)
		expect((await proposals(page))[0].hotspot.enabled).toBe(false)
	})

	test('says the network the cable reaches gave no connection, with the device’s reason', async ({ page }) => {
		await openSetup(page, { document: ON_WIFI, capabilities: PI, facts: [carrier(), joined('Clinic-Staff')] })
		await answer(page, 'configuration', [
			{ echo: true },
			stateOf({ outcome: 'verifying' }),
			stateOf({ outcome: 'failed', failure: { at: "$['attachments'][0]", reason: 'No address from DHCP', reached: 'addressing' } }),
		])
		await openWay(page, 'Cable')
		await expect(page.getByText("The cable isn't connecting")).toBeVisible()
		await expect(page.getByText('No address from DHCP')).toBeVisible()
		await page.waitForTimeout(100)
		expect(await proposals(page)).toHaveLength(1)
	})

	test('proposes nothing while the cable is on, and says it is not connecting', async ({ page }) => {
		await openSetup(page, { document: ON_CABLE, capabilities: PI, facts: [carrier()] })
		await openWay(page, 'Cable')
		await expect(page.getByText("The cable isn't connecting")).toBeVisible()
		await expect(page.getByText("hasn't given the Iti a connection")).toBeVisible()
		await page.waitForTimeout(100)
		expect(await proposals(page)).toEqual([])
	})

	test('says the device is connected by cable where it is in use', async ({ page }) => {
		await openSetup(page, { document: ON_CABLE, capabilities: PI, facts: [carrier(), address('eth0')] })
		await openWay(page, 'Cable')
		await expect(page.getByText('The Iti is connected by cable')).toBeVisible()
	})

	test('returns to the Status screen once a cable that was on starts working', async ({ page }) => {
		await openSetup(page, { document: ON_CABLE, capabilities: PI, facts: [carrier(false)] })
		await openWay(page, 'Cable')
		await expect(page.getByText('Waiting for a cable…')).toBeVisible()
		await feed(page, carrier(true), address('eth0'))
		await expect(title(page)).toHaveText(UNNAMED)
		expect(await proposals(page)).toEqual([])
	})

	test('Back returns to the Network screen and keeps the session open', async ({ page }) => {
		await openSetup(page, { document: ON_CABLE, capabilities: PI, facts: [carrier(false)] })
		await openWay(page, 'Cable')
		await page.getByRole('button', { name: 'Back', exact: true }).click()
		await expect(title(page)).toHaveText('Network')
		expect(await page.evaluate(() => window.__blitiSessions.map((each) => each.closedByPage))).toEqual([false])
		expect((await sent(page)).filter((each) => each.type === 'configure')).toHaveLength(1)
	})
})
