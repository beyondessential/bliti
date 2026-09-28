// The devices this tab has opened a channel with, and the one it is on (WEB, "Remembering devices",
// "After a reload").
//
// Kept in session storage, which lives exactly as long as the tab: a reload or a restored tab keeps it,
// and closing the tab forgets it. A code is kept as its human-readable rendering, which reads back to
// the same payload, so a remembered code is read by the same path as a typed one.

// How many devices are remembered (WEB).
const KEPT = 3

const RECENT = 'bliti.recent'
const ON = 'bliti.on'

/// Put `code` at the top of `recent`, keeping the hostname it had where none is given, and dropping
/// the oldest beyond the cap.
export function remember(recent, code, hostname) {
	const held = recent.find((each) => each.code === code)
	const entry = { code, hostname: hostname ?? held?.hostname ?? null }
	return [entry, ...recent.filter((each) => each.code !== code)].slice(0, KEPT)
}

/// The last group of a rendering, which is all public key (WEB).
export function lastGroup(code) {
	return code.split('-').pop()
}

/// The remembered devices as stored, most recent first. Anything unreadable is treated as nothing.
export function loadRecent() {
	const stored = read(RECENT)
	if (!Array.isArray(stored)) return []
	return stored
		.filter((each) => typeof each?.code === 'string')
		.map(({ code, hostname }) => ({ code, hostname: typeof hostname === 'string' ? hostname : null }))
}

export function saveRecent(recent) {
	write(RECENT, recent)
}

/// The code of the device the page was on when it last unloaded, if it was on one.
export function loadOn() {
	const stored = read(ON)
	return typeof stored === 'string' ? stored : null
}

export function saveOn(code) {
	write(ON, code)
}

// Storage can be refused outright, as in some private modes. Remembering is then only lost, never an
// error the operator meets.
function read(key) {
	try {
		const text = sessionStorage.getItem(key)
		return text === null ? null : JSON.parse(text)
	} catch {
		return null
	}
}

function write(key, value) {
	try {
		if (value === null) sessionStorage.removeItem(key)
		else sessionStorage.setItem(key, JSON.stringify(value))
	} catch {
		// As above.
	}
}
