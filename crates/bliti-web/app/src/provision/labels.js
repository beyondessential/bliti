// The list of labels the page keeps for the boards it has provisioned (PSHT, "The list" and
// "Keeping the list").
//
// Kept in local storage, so every tab and every visit of the browser holds the same one. Each change
// reads the list again just before it writes, so a label another tab added in the meantime is kept.
// Where storage cannot be used at all, the list lives for as long as the page does.

const NAME = 'bliti.provision.labels'

/// How long the `at` of a removed label is remembered, so a finished board seen again does not add
/// its label back (PSHT).
export const REMEMBERED = 7 * 86_400_000

/// What a label says of the board, in its payload's last four characters (PRV).
export const suffixOf = (payload) => payload.slice(-4)

function storage() {
	try {
		return window.localStorage
	} catch {
		return null
	}
}

/// Whether the browser keeps what is written to its storage.
export function kept() {
	const held = storage()
	if (!held) return false
	try {
		held.setItem(`${NAME}.probe`, '1')
		held.removeItem(`${NAME}.probe`)
		return true
	} catch {
		return false
	}
}

const DURABLE = kept()

/// Whether this page's list outlives it.
export const DURABLE_KEPT = DURABLE
let memory = null

const isLabel = (label) =>
	typeof label?.payload === 'string' &&
	typeof label.version === 'string' &&
	typeof label.at === 'string' &&
	(label.printed === null || typeof label.printed === 'string')

/// The list as it stands: `labels`, oldest first, and `seen`, the latest `at` added for each suffix
/// and until when it is remembered, which is null while the label is held.
export function read(now = Date.now()) {
	let found = memory
	if (DURABLE) {
		try {
			found = JSON.parse(storage().getItem(NAME))
		} catch {
			found = null
		}
	}
	const labels = Array.isArray(found?.labels) ? found.labels.filter(isLabel) : []
	const seen = {}
	for (const [suffix, entry] of Object.entries(found?.seen ?? {})) {
		const open = entry?.until === null || (typeof entry?.until === 'number' && entry.until > now)
		if (typeof entry?.at === 'string' && open) seen[suffix] = { at: entry.at, until: entry.until }
	}
	return { labels, seen }
}

function write(list) {
	if (!DURABLE) {
		memory = list
		return
	}
	try {
		storage().setItem(NAME, JSON.stringify(list))
	} catch {
		// Storage that stops taking writes leaves the list as it was.
	}
}

/// The list with `label` added, or `list` itself where its `at` is not later than the latest added
/// for its board. A label for a payload already listed replaces it, unprinted (PSHT).
export function offered(list, { payload, version, at }) {
	const suffix = suffixOf(payload)
	const known = list.seen[suffix]
	if (known && known.at >= at) return list
	return {
		labels: [...list.labels.filter((label) => label.payload !== payload), { payload, version, at, printed: null }],
		seen: { ...list.seen, [suffix]: { at, until: null } },
	}
}

/// The list with each label of `payloads` printed as of `now`.
export function printed(list, payloads, now = Date.now()) {
	const at = new Date(now).toISOString()
	return {
		...list,
		labels: list.labels.map((label) => (payloads.includes(label.payload) ? { ...label, printed: at } : label)),
	}
}

/// The list without the label of `payload`, whose `at` is remembered for a week.
export function removed(list, payload, now = Date.now()) {
	const label = list.labels.find((each) => each.payload === payload)
	if (!label) return list
	return {
		labels: list.labels.filter((each) => each !== label),
		seen: { ...list.seen, [suffixOf(payload)]: { at: label.at, until: now + REMEMBERED } },
	}
}

/// Change the list: read it, apply `change`, keep the result. The list as changed, or null where the
/// change left it as it was.
export function change(change_, now = Date.now()) {
	const list = read(now)
	const next = change_(list, now)
	if (next === list) return null
	write(next)
	return next
}

/// The labels in the order they are shown and printed: the latest first.
export const ordered = (list) => [...list.labels].sort((a, b) => b.at.localeCompare(a.at))

/// How many labels are unprinted.
export const unprinted = (list) => list.labels.filter((label) => label.printed === null).length

export const STORAGE_NAME = NAME
