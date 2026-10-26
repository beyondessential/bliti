// The rows of the Status screen of STAT, as plain functions: no page, because none of it touches one.

import { expect, test } from '@playwright/test'

import { IN_PROGRESS, NOT_WELL, WARNING, WELL, fromNow, isLinkLocal, statusRows } from '../src/status.js'

const passed = { status: { is: 'passed' } }
const fact = (name, value, traits = {}, kind = 'text') => ({ fact: true, name, kind, value, traits: { ...passed, ...traits } })
const reading = (name, value, traits = {}) => ({ fact: false, name, kind: 'quantity', value, traits: { ...passed, ...traits } })
const on = (name, extra = {}) => ({ interface: { name, ...extra } })
const address = (iface, value, extra) => fact('network-address', value, on(iface, extra), value.includes(':') ? 'ipv6' : 'ipv4')
const carrier = (iface, plugged, extra) => fact('wired-carrier', plugged, on(iface, extra), 'boolean')
const joined = (iface, network, extra) => fact('wireless-network', network, on(iface, extra))
const hotspot = () => fact('hotspot', 'Clinic-Field-05', { channel: { number: 11 } })
const broken = (name, reason) => ({ fact: true, name, kind: 'text', traits: { status: { is: 'broken', reason } } })
const withStatus = (entry, is, reason) => ({ ...entry, traits: { ...entry.traits, status: { is, reason } } })

const rows = (entries, settled = true, now) => statusRows(entries, settled, now)
const network = (...entries) => rows(entries).network
const said = (row) => row.lines.map((parts) => parts.map((part) => [part.label, part.text].filter(Boolean).join(' ')).join(' · '))
const icons = (row) => row.lines.flat().map((part) => part.icon).filter(Boolean)

test.describe('before the first batch has ended', () => {
	test('every row is checking, and Tailscale is shown', () => {
		const { network, tailscale, tamanu } = rows([], false)
		expect([network, tailscale, tamanu].map((row) => [row.mark, row.headline])).toEqual([
			[IN_PROGRESS, 'Checking network'],
			[IN_PROGRESS, 'Checking Tailscale'],
			[IN_PROGRESS, 'Checking Tamanu'],
		])
		expect(network.action).toBeUndefined()
	})

	test('the network row is checking even with what it would read held', () => {
		expect(rows([carrier('end0', true), address('end0', '192.0.2.4')], false).network.mark).toBe(IN_PROGRESS)
	})
})

test.describe('the network row', () => {
	test('is well on a cable with an address, offering Change', () => {
		const row = network(carrier('end0', true), address('end0', '192.0.2.4'))
		expect([row.mark, row.headline, row.action]).toEqual([WELL, 'Network connected', 'Change'])
		expect(said(row)).toEqual(['Cable'])
		expect(icons(row)).toEqual(['cable'])
	})

	test('is well on wifi with an address, naming the network joined', () => {
		const row = network(joined('wlan0', 'Clinic-Staff'), address('wlan0', '10.0.0.4'))
		expect(row.mark).toBe(WELL)
		expect(said(row)).toEqual(['Clinic-Staff'])
		expect(icons(row)).toEqual(['wifi'])
	})

	test('is well on the hotspot alone, with how many have joined', () => {
		const row = network(hotspot(), reading('hotspot-clients', 2))
		expect(row.mark).toBe(WELL)
		expect(said(row)).toEqual(['Hotspot, 2 joined'])
		expect(icons(row)).toEqual(['hotspot'])
	})

	test('says no count where hotspot-clients carries no value', () => {
		const clients = { fact: false, name: 'hotspot-clients', kind: 'quantity', traits: { status: { is: 'broken', reason: 'no answer' } } }
		expect(said(network(hotspot(), clients))).toEqual(['Hotspot'])
		expect(said(network(hotspot()))).toEqual(['Hotspot'])
	})

	test('names one of cable and wifi, then the hotspot', () => {
		const row = network(carrier('end0', true), address('end0', '192.0.2.4'), joined('wlan0', 'Clinic-Staff'), address('wlan0', '10.0.0.4'), hotspot(), reading('hotspot-clients', 0))
		expect(said(row)).toEqual(['Cable · Hotspot, 0 joined'])
		expect(icons(row)).toEqual(['cable', 'hotspot'])
	})

	test('names the way carrying the default route', () => {
		const row = network(carrier('end0', true), address('end0', '192.0.2.4'), joined('wlan0', 'Clinic-Staff'), address('wlan0', '10.0.0.4', { route: 'default' }))
		expect(said(row)).toEqual(['Clinic-Staff'])
	})

	test('falls back to the first cable by interface name, then the first wifi', () => {
		expect(said(network(carrier('end1', true), address('end1', '192.0.2.5'), carrier('end0', true), address('end0', '192.0.2.4', { route: 'default' }), joined('wlan0', 'B'), address('wlan0', '10.0.0.4')))).toEqual(['Cable'])
		const routeElsewhere = [address('tailscale0', '100.64.0.1', { route: 'default', overlay: 'tailscale' })]
		expect(said(network(joined('wlan1', 'B'), address('wlan1', '10.0.1.4'), joined('wlan0', 'A'), address('wlan0', '10.0.0.4'), ...routeElsewhere))).toEqual(['A'])
		expect(said(network(joined('wlan0', 'A'), address('wlan0', '10.0.0.4'), carrier('end0', true), address('end0', '192.0.2.4')))).toEqual(['Cable'])
	})

	test('counts a link-local address as none', () => {
		expect(network(carrier('end0', true), address('end0', '169.254.3.7')).mark).toBe(NOT_WELL)
		expect(network(carrier('end0', true), address('end0', 'fe80::1')).mark).toBe(NOT_WELL)
		expect(network(carrier('end0', true), address('end0', 'fd6d::3')).mark).toBe(WELL)
	})

	test('counts an address on another interface as none', () => {
		expect(network(carrier('end0', true), address('wlan0', '10.0.0.4')).mark).toBe(NOT_WELL)
	})

	test('is not well with nothing in use, offering Set up', () => {
		const row = network(carrier('end0', false))
		expect([row.mark, row.headline, row.action]).toEqual([NOT_WELL, 'Network not connected', 'Set up'])
		expect(row.lines).toEqual([])
	})

	test('says a cable is plugged in with no address', () => {
		expect(said(network(carrier('end0', true)))).toEqual(['Cable plugged in, no address'])
	})

	test('says the network joined with no address, naming it', () => {
		expect(said(network(joined('wlan0', 'Clinic-Staff')))).toEqual(['Joined Clinic-Staff, no address'])
	})

	test('says both where both hold no address', () => {
		expect(said(network(carrier('end0', true), joined('wlan0', 'Clinic-Staff')))).toEqual(['Cable plugged in, no address', 'Joined Clinic-Staff, no address'])
	})
})

test.describe('the Tailscale row', () => {
	const state = (value, is = 'passed', reason) => withStatus(fact('tailscale-state', value), is, reason)
	const tailscale = (...entries) => rows(entries).tailscale
	const view = (row) => [row.mark, row.headline, row.reason]

	test('is hidden once the first batch has ended where the device reports no tailscale-state', () => {
		expect(tailscale()).toBeNull()
	})

	test('reads each state it recognises by the table', () => {
		expect(view(tailscale(state('running')))).toEqual([WELL, 'Tailscale connected', null])
		expect(view(tailscale(state('running', 'warning', 'no route to the coordination server')))).toEqual([WARNING, 'Tailscale connected', 'no route to the coordination server'])
		expect(view(tailscale(state('starting')))).toEqual([IN_PROGRESS, 'Tailscale connecting', null])
		expect(view(tailscale(state('stopped')))).toEqual([NOT_WELL, 'Tailscale turned off', null])
		expect(view(tailscale(state('needs-login')))).toEqual([NOT_WELL, 'Tailscale not signed in', null])
		expect(view(tailscale(state('expired')))).toEqual([NOT_WELL, 'Tailscale sign-in expired', null])
		expect(view(tailscale(state('needs-approval')))).toEqual([NOT_WELL, 'Tailscale waiting for approval', null])
		expect(view(tailscale(broken('tailscale-state', 'tailscaled is not answering')))).toEqual([NOT_WELL, 'Tailscale state unknown', 'tailscaled is not answering'])
	})

	test('names the device as it is reached, where tailscale-name carries a value', () => {
		expect(said(tailscale(state('running'), fact('tailscale-name', 'clinic-iti-07')))).toEqual(['Device name clinic-iti-07'])
		expect(said(tailscale(state('running'), broken('tailscale-name', 'no answer')))).toEqual([])
	})

	test('a sign-in lapsing lifts well to warning and says when', () => {
		const now = Date.parse('2026-10-27T00:00:00Z')
		const expiry = withStatus(fact('tailscale-key-expiry', '2026-10-30T00:00:00Z', {}, 'datetime'), 'warning', 'lapses within a week')
		const row = rows([state('running'), expiry], true, now).tailscale
		expect(row.mark).toBe(WARNING)
		expect(said(row)).toEqual(['Sign-in expires in 3 days'])

		// Only well is lifted.
		expect(rows([state('stopped'), expiry], true, now).tailscale.mark).toBe(NOT_WELL)
		// A sign-in not lapsing soon says nothing.
		expect(said(rows([state('running'), withStatus(expiry, 'passed')], true, now).tailscale)).toEqual([])
	})
})

test.describe('the Tamanu row', () => {
	const state = (value, is = 'passed', reason) => withStatus(fact('tamanu-state', value), is, reason)
	const tamanu = (...entries) => rows(entries).tamanu
	const view = (row) => [row.mark, row.headline, row.reason]

	test('is checking while the device reports no tamanu-state', () => {
		expect(view(tamanu())).toEqual([IN_PROGRESS, 'Checking Tamanu', null])
	})

	test('reads each state it recognises by the table', () => {
		expect(view(tamanu(state('running')))).toEqual([WELL, 'Tamanu running', null])
		expect(view(tamanu(state('degraded', 'warning', 'Facility server is running 2.33.2.')))).toEqual([WARNING, 'Tamanu partly running', 'Facility server is running 2.33.2.'])
		expect(view(tamanu(state('stopped', 'failed', 'No required service is running.')))).toEqual([NOT_WELL, 'Tamanu stopped', 'No required service is running.'])
		expect(view(tamanu(state('absent', 'failed', 'not installed')))).toEqual([NOT_WELL, 'Tamanu not installed', null])
		expect(view(tamanu(state('starting')))).toEqual([IN_PROGRESS, 'Tamanu starting', null])
		expect(view(tamanu(state('restarting')))).toEqual([IN_PROGRESS, 'Tamanu restarting', null])
		expect(view(tamanu(broken('tamanu-state', 'could not reach systemd')))).toEqual([NOT_WELL, 'Tamanu state unknown', 'could not reach systemd'])
	})

	test('carries both versions, each only where it carries a value', () => {
		expect(said(tamanu(state('running'), fact('tamanu-web-version', '2.34.1'), fact('tamanu-facility-server-version', '2.34.0')))).toEqual(['Web 2.34.1 · Facility server 2.34.0'])
		expect(said(tamanu(state('running'), broken('tamanu-web-version', 'no answer'), fact('tamanu-facility-server-version', '2.34.0')))).toEqual(['Facility server 2.34.0'])
		expect(said(tamanu(state('running')))).toEqual([])
	})
})

test.describe('a state this build does not recognise', () => {
	test('is marked by its status, says its value as sent, and carries the reason', () => {
		const state = (is, reason) => withStatus(fact('tamanu-state', 'migrating'), is, reason)
		const view = (row) => [row.mark, row.headline, row.reason]
		expect(view(rows([state('passed')]).tamanu)).toEqual([WELL, 'Tamanu migrating', null])
		expect(view(rows([state('warning', 'slow')]).tamanu)).toEqual([WARNING, 'Tamanu migrating', 'slow'])
		expect(view(rows([state('failed', 'stuck')]).tamanu)).toEqual([NOT_WELL, 'Tamanu migrating', 'stuck'])
		expect(view(rows([withStatus(fact('tailscale-state', 'hibernating'), 'weird', 'odd')]).tailscale)).toEqual([NOT_WELL, 'Tailscale hibernating', 'odd'])
	})
})

test('link-local addresses are told apart', () => {
	expect(['169.254.0.1', 'fe80::1', 'FE80::1', 'febf::1'].map(isLinkLocal)).toEqual([true, true, true, true])
	expect(['192.0.2.4', 'fec0::1', 'fd6d::3', '2407:8b00::3'].map(isLinkLocal)).toEqual([false, false, false, false])
})

test('a time from now is said in its largest whole unit', () => {
	const now = 0
	expect(fromNow(3 * 86_400_000 + 5, now)).toBe('in 3 days')
	expect(fromNow(5 * 3_600_000, now)).toBe('in 5 hours')
	expect(fromNow(20 * 60_000, now)).toBe('in 20 minutes')
	expect(fromNow(86_400_000, now)).toBe('tomorrow')
})
