// What the provisioning page decides from a `provision-state`, with no browser in it (PWEB, PRS).
// state.js imports nothing from the browser, so these run under Node.

import { expect, test } from '@playwright/test'

import {
	actSaid,
	civilDate,
	daysBefore,
	pictureFor,
	recordOf,
	refusal,
	screenOf,
	showDate,
	showSaved,
	stateSaid,
	ticked,
} from '../src/provision/state.js'

const LIMITS = { revisions: [2, 4], lifetime: 30 }
const state = (revision, built) => ({ revision, built })
const refusing = (revision, built, today = '2026-10-29') => refusal(state(revision, built), { ...LIMITS, today })

test.describe('the revision', () => {
	test('is accepted anywhere in the range, both ends included', () => {
		for (const revision of [2, 3, 4]) expect(refusing(revision, '2026-10-29')).toBeNull()
	})

	test('below the range is a medium to write again', () => {
		expect(refusing(1, '2026-10-29')).toEqual({ kind: 'medium', built: null })
	})

	test('above the range is a page to reload, whatever the date', () => {
		expect(refusing(5, '2026-10-29')).toEqual({ kind: 'page' })
		expect(refusing(5, '2020-01-01')).toEqual({ kind: 'page' })
	})
})

test.describe('the image date', () => {
	test('29 days before today is accepted and 30 days is refused, with its date', () => {
		expect(refusing(3, '2026-09-30')).toBeNull()
		expect(refusing(3, '2026-09-29')).toEqual({ kind: 'medium', built: '2026-09-29' })
	})

	test('counts across a month and a year boundary', () => {
		expect(daysBefore('2026-02-28', '2026-03-01')).toBe(1)
		expect(daysBefore('2025-12-31', '2026-01-01')).toBe(1)
		expect(daysBefore('2024-02-28', '2024-03-01')).toBe(2)
		expect(refusing(3, '2025-12-02', '2026-01-01')).toEqual({ kind: 'medium', built: '2025-12-02' })
		expect(refusing(3, '2025-12-03', '2026-01-01')).toBeNull()
	})

	test('built today or in the future is accepted', () => {
		expect(refusing(3, '2026-10-29')).toBeNull()
		expect(refusing(3, '2026-11-15')).toBeNull()
	})

	test('says its date as well where the revision is also below the range', () => {
		expect(refusing(1, '2026-01-01')).toEqual({ kind: 'medium', built: '2026-01-01' })
	})

	test('reads the phone date as the local date, not the UTC one', () => {
		expect(civilDate(new Date(2026, 0, 5, 23, 59))).toBe('2026-01-05')
		expect(civilDate(new Date(2026, 0, 5, 0, 1))).toBe('2026-01-05')
	})

	test('writes a date for reading', () => {
		expect(showDate('2026-09-12')).toBe('12 Sep 2026')
		expect(showSaved(new Date(2026, 9, 29, 14, 32).toISOString())).toBe('29 Oct 2026, 14:32')
		expect(showSaved('not a time')).toBe('not a time')
	})
})

test('each state of the device shows its screen', () => {
	expect(['idle', 'running', 'waiting', 'failed', 'finished'].map((run) => screenOf({ run }))).toEqual([
		'choosing',
		'run',
		'run',
		'run',
		'done',
	])
})

test('a step is ticked unless it is done', () => {
	expect(['done', 'not-done', 'unknown'].map((done) => ticked({ done }))).toEqual([false, true, true])
})

test('the record is the first one a step reports', () => {
	const record = { suffix: 'K7QX', at: '2026-10-29T14:32:00+11:00' }
	expect(recordOf([{ id: 'a' }, { id: 'b', result: {} }])).toBeNull()
	expect(recordOf([{ id: 'a' }, { id: 'b', result: { record } }, { id: 'c', result: { record: { suffix: 'X' } } }])).toBe(record)
})

test('a picture goes beneath the prompt of its step only', () => {
	expect(pictureFor('hardware', 'case')).toBe('case-switch')
	expect(pictureFor('eeprom', 'id-wp')).toBe('wp-pads')
	expect(pictureFor('eeprom', 'case')).toBeNull()
	expect(pictureFor('hardware', 'id-wp')).toBeNull()
	expect(pictureFor('tailscale', 'case')).toBeNull()
})

test.describe('the activity record', () => {
	test('names the acts and what they chose', () => {
		expect(actSaid('start', { steps: ['hardware', 'eeprom'] })).toBe('provision-act  start  steps hardware, eeprom')
		expect(actSaid('retry')).toBe('provision-act  retry')
		expect(actSaid('answer', { prompt: 'sure', value: true })).toBe('provision-act  answer  prompt sure  value true')
	})

	test('says a secret answer without its value', () => {
		const said = actSaid('answer', { prompt: 'key', value: 'tskey-hunter2', secret: true })
		expect(said).toBe('provision-act  answer  prompt key')
		expect(said).not.toContain('hunter2')
	})

	test('says a state without its progress', () => {
		const steps = [
			{ id: 'a', state: 'passed', progress: 1 },
			{ id: 'b', state: 'running', progress: 0.4 },
		]
		expect(stateSaid({ run: 'running', steps, prompts: [] })).toBe('provision-state  running  a passed, b running')
		expect(stateSaid({ run: 'waiting', steps: [steps[0]], prompts: [{ id: 'case' }] })).toBe(
			'provision-state  waiting  a passed, prompts case',
		)
	})
})
