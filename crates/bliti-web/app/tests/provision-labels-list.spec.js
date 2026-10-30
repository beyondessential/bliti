// The list of labels, with no browser in it (PSHT, "The list"). labels.js keeps nothing in storage
// under Node, so these run on lists handed in.

import { expect, test } from '@playwright/test'

import { REMEMBERED, offered, ordered, printed, removed, suffixOf, unprinted } from '../src/provision/labels.js'

const empty = { labels: [], seen: {} }
const label = (payload, at, version = '4.0') => ({ payload, version, at })
const A = 'BLITI:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA'.replace(/.{4}$/, 'AAAA')
const B = A.replace(/.{4}$/, 'BBBB')

test('a label is added unprinted, under the last four characters of its payload', () => {
	const list = offered(empty, label(A, '2026-10-31T01:00:00Z'))
	expect(suffixOf(A)).toBe('AAAA')
	expect(list.labels).toEqual([{ payload: A, version: '4.0', at: '2026-10-31T01:00:00Z', printed: null }])
	expect(list.seen).toEqual({ AAAA: { at: '2026-10-31T01:00:00Z', until: null } })
})

test('a result whose time is not later than the latest added for its board is ignored', () => {
	const list = offered(empty, label(A, '2026-10-31T01:00:00Z'))
	expect(offered(list, label(A, '2026-10-31T01:00:00Z'))).toBe(list)
	expect(offered(list, label(A, '2026-10-30T01:00:00Z'))).toBe(list)
})

test('a later result for a payload replaces its label, unprinted', () => {
	const first = printed(offered(empty, label(A, '2026-10-31T01:00:00Z')), [A], 1)
	expect(first.labels[0].printed).not.toBeNull()
	const again = offered(first, label(A, '2026-10-31T02:00:00Z'))
	expect(again.labels).toEqual([{ payload: A, version: '4.0', at: '2026-10-31T02:00:00Z', printed: null }])
})

test('the list holds at most one label for a payload and one for each board', () => {
	let list = offered(empty, label(A, '2026-10-31T01:00:00Z'))
	list = offered(list, label(B, '2026-10-31T01:30:00Z'))
	list = offered(list, label(A, '2026-10-31T02:00:00Z'))
	expect(list.labels.map((each) => each.payload).sort()).toEqual([A, B].sort())
})

test('printing marks only the labels named, as of the time given', () => {
	let list = offered(offered(empty, label(A, '2026-10-31T01:00:00Z')), label(B, '2026-10-31T02:00:00Z'))
	list = printed(list, [B], Date.UTC(2026, 9, 31, 3))
	expect(list.labels.find((each) => each.payload === A).printed).toBeNull()
	expect(list.labels.find((each) => each.payload === B).printed).toBe('2026-10-31T03:00:00.000Z')
	expect(unprinted(list)).toBe(1)
})

test('removing a label drops its payload and remembers its time for a week', () => {
	const now = Date.UTC(2026, 9, 31, 4)
	const list = removed(offered(empty, label(A, '2026-10-31T01:00:00Z')), A, now)
	expect(list.labels).toEqual([])
	expect(JSON.stringify(list)).not.toContain(A)
	expect(list.seen.AAAA).toEqual({ at: '2026-10-31T01:00:00Z', until: now + REMEMBERED })
	// Seen again, the same result adds nothing.
	expect(offered(list, label(A, '2026-10-31T01:00:00Z'))).toBe(list)
	// A later result, from running the step again, adds it.
	expect(offered(list, label(A, '2026-10-31T05:00:00Z')).labels).toHaveLength(1)
})

test('removing a label the list does not hold changes nothing', () => {
	expect(removed(empty, A)).toBe(empty)
})

test('the latest label is shown and printed first', () => {
	const list = offered(offered(empty, label(A, '2026-10-31T01:00:00Z')), label(B, '2026-10-31T02:00:00Z'))
	expect(ordered(list).map((each) => each.payload)).toEqual([B, A])
})
