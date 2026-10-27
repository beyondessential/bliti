// The acts of the Status screen's Tailscale and Tamanu rows (STAT), and how the Tailscale screen reads
// and shares what a network check or a ping found (TSCR), as plain functions: no page, because none
// of it touches one.

import { expect, test } from '@playwright/test'

import {
	ROW_ACTS,
	TAILSCALE_ACTS,
	netcheckFigures,
	netcheckShare,
	pingSays,
	tailscaleActSaid,
	tailscaleRowAct,
	tamanuRowAct,
	verdict,
} from '../src/services.js'

const EVERY_TAILSCALE_ACT = ['sign-in', 'turn-on', 'turn-off', 'log-out', 'netcheck', 'ping']

test.describe('the Tailscale row', () => {
	test('offers the act its table gives for each state', () => {
		for (const [state, act] of [
			['needs-login', 'sign-in'],
			['expired', 'sign-in'],
			['stopped', 'turn-on'],
			['running', 'netcheck'],
			['broken', 'netcheck'],
			['needs-approval', 'log-out'],
		]) {
			expect(tailscaleRowAct(state, EVERY_TAILSCALE_ACT), state).toBe(act)
		}
	})

	test('offers nothing while connecting, though the device lists acts', () => {
		expect(tailscaleRowAct('starting', ['turn-off', 'log-out', 'netcheck'])).toBeNull()
	})

	test('offers no act the device has not listed, nor any before it lists them', () => {
		expect(tailscaleRowAct('running', ['turn-off', 'log-out'])).toBeNull()
		expect(tailscaleRowAct('running', null)).toBeNull()
		expect(tailscaleRowAct(null, EVERY_TAILSCALE_ACT)).toBeNull()
	})

	test('confirms only Cancel, asking whether to cancel the sign-in and preferring to keep waiting', () => {
		const confirmed = ['sign-in', 'turn-on', 'netcheck', 'log-out'].filter((act) => ROW_ACTS[act].question)
		expect(confirmed).toEqual(['log-out'])
		expect(ROW_ACTS['log-out']).toMatchObject({ button: 'Cancel', question: 'Cancel the sign-in?', keep: 'Keep waiting' })
	})
})

test.describe('the Tamanu row', () => {
	test('offers Start while stopped and Restart while running or partly running', () => {
		expect(tamanuRowAct('stopped', ['start'])).toBe('start')
		expect(tamanuRowAct('running', ['restart'])).toBe('restart')
		expect(tamanuRowAct('degraded', ['restart'])).toBe('restart')
	})

	test('offers nothing while starting, restarting, absent or unknown', () => {
		for (const state of ['starting', 'restarting', 'absent', 'broken', null]) {
			expect(tamanuRowAct(state, ['start', 'restart']), String(state)).toBeNull()
		}
	})

	test('confirms both, each naming its act', () => {
		expect(ROW_ACTS.start.question).toBe('Start Tamanu?')
		expect(ROW_ACTS.restart.question).toBe('Restart Tamanu?')
		expect(ROW_ACTS.restart.warning).toBe('Tamanu may be unavailable while it restarts.')
	})
})

test.describe('the Tailscale screen acts', () => {
	test('are in the order connect, turn on, turn off, log out', () => {
		expect(TAILSCALE_ACTS.map((each) => each.act)).toEqual(['sign-in', 'turn-on', 'turn-off', 'log-out'])
	})

	test('confirm turning off and logging out in words saying the fleet loses the device', () => {
		const words = Object.fromEntries(TAILSCALE_ACTS.filter((each) => each.question).map((each) => [each.act, each.warning]))
		expect(Object.keys(words)).toEqual(['turn-off', 'log-out'])
		expect(words['turn-off']).toContain('until it is turned on again')
		expect(words['log-out']).toContain('until it is signed in again')
		for (const warning of Object.values(words)) expect(warning).toContain('fleet')
	})
})

const relay = (id, name, latency) => ({ id, name, latency })

test.describe('a network check', () => {
	test('can connect where any relay answered, whether or not UDP got through', () => {
		const relays = [relay(5, 'Sydney', 0.03)]
		expect(verdict({ udp: true, relays })).toEqual({ connects: true, text: 'Can connect' })
		expect(verdict({ udp: false, relays })).toEqual({ connects: true, text: 'Can connect' })
	})

	test('is blocked where no relay answered, saying why as far as the result says', () => {
		expect(verdict({ relays: [], 'captive-portal': true, ipv4: false, ipv6: false }).text).toBe('Blocked: the network asks for a sign-in')
		expect(verdict({ relays: [], 'captive-portal': null, ipv4: false, ipv6: false }).text).toBe('Blocked: no internet')
		expect(verdict({ relays: [], 'captive-portal': false, ipv4: true, ipv6: false }).text).toBe(
			"Blocked: Tailscale can't be reached from this network",
		)
		expect(verdict({ relays: [] }).connects).toBe(false)
	})

	test('shows each figure, the two the test may not tell only where it did, and relays nearest first', () => {
		const told = netcheckFigures({
			udp: true,
			ipv4: true,
			ipv6: false,
			'nat-varies': true,
			'captive-portal': false,
			relays: [relay(9, 'Dallas', 0.15), relay(12, null, 0.0904), relay(5, 'Sydney', 0.0384)],
		})
		expect(told.figures).toEqual([
			['UDP', 'Yes'],
			['IPv4', 'Yes'],
			['IPv6', 'No'],
			['Address varies', 'Yes'],
			['Asks for sign-in', 'No'],
		])
		expect(told.relays).toEqual([
			{ id: 5, name: 'Sydney', latency: '38 ms' },
			{ id: 12, name: 'Relay 12', latency: '90 ms' },
			{ id: 9, name: 'Dallas', latency: '150 ms' },
		])
		const untold = netcheckFigures({ udp: false, ipv4: true, ipv6: false, 'nat-varies': null, 'captive-portal': null, relays: [] })
		expect(untold.figures.map(([label]) => label)).toEqual(['UDP', 'IPv4', 'IPv6'])
	})

	test('is shared as the device, when it ran, the result without report, then report untouched', () => {
		const report = '{\n\t"UDP": true,\n\t"GlobalV4": "203.0.113.7:41641"\n}\n'
		const result = { udp: true, ipv4: true, relays: [relay(5, 'Sydney', 0.03)], report }
		const text = netcheckShare('clinic-iti-07', new Date('2026-10-27T09:42:00Z'), result)
		const [head, json, ...rest] = text.split('\n\n')
		expect(head).toBe('Network check of clinic-iti-07 at 2026-10-27T09:42:00.000Z')
		expect(JSON.parse(json)).toEqual({ udp: true, ipv4: true, relays: [relay(5, 'Sydney', 0.03)] })
		expect(rest.join('\n\n')).toBe(report)
		expect(text.endsWith(report)).toBe(true)
	})
})

test.describe('a ping', () => {
	test('says the machine answered, how, and the round trip', () => {
		expect(pingSays({ peer: 'tamanu-central', reachable: true, path: 'relayed', relay: relay(5, 'Sydney'), latency: 0.058 })).toEqual({
			text: 'tamanu-central answered',
			figures: [
				['Path', 'Through a relay, Sydney'],
				['Round trip', '58 ms'],
			],
		})
		expect(pingSays({ peer: 'canopy', reachable: true, path: 'direct', relay: null, latency: 0.004 }).figures[0]).toEqual(['Path', 'Direct'])
	})

	test('says the machine did not answer, and nothing more', () => {
		expect(pingSays({ peer: 'canopy', reachable: false, path: null, relay: null, latency: null })).toEqual({
			text: 'canopy did not answer',
			figures: [],
		})
	})
})

test('the activity record says a sign-in by its method and name, with no way to carry the key', () => {
	expect(tailscaleActSaid('sign-in', { method: 'auth-key', key: 'tskey-auth-hunter2', name: 'ward-3' })).toBe(
		'tailscale-act  sign-in, method auth-key, name ward-3',
	)
	expect(tailscaleActSaid('ping', { peer: 'canopy' })).toBe('tailscale-act  ping, peer canopy')
	expect(tailscaleActSaid('netcheck')).toBe('tailscale-act  netcheck')
})
