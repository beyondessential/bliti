// The Tailscale auth key the page keeps between boards (PWEB, "Tailscale details").
//
// Kept in local storage with the time it was saved, so a bench of boards is provisioned from one
// entry, across tabs and visits, and dropped once it is a week old. Where storage cannot be used at
// all the key is simply never kept.

const NAME = 'bliti.provision.tailscale-key'

/// How long a saved key is kept: as long as the key itself is asked to last (PWEB).
export const KEPT = 7 * 86_400_000

/// The step and prompt that ask for the key (PTS).
export const STEP = 'tailscale'
export const PROMPT = 'auth-key'

/// Whether `prompt` is the one that asks for the key.
export const isKeyPrompt = (prompt) => prompt.step === STEP && prompt.id === PROMPT && prompt.kind === 'secret'

function storage() {
	try {
		return window.localStorage
	} catch {
		return null
	}
}

/// The key saved and when (an ISO time), or null where there is none or it is a week old or more,
/// which is removed.
export function savedKey(now = Date.now()) {
	const held = storage()
	if (!held) return null
	let found = null
	try {
		found = JSON.parse(held.getItem(NAME))
	} catch {
		// Unreadable is as good as none.
	}
	const saved = typeof found?.saved === 'string' ? Date.parse(found.saved) : Number.NaN
	if (typeof found?.key !== 'string' || found.key === '' || Number.isNaN(saved) || now - saved >= KEPT || saved > now) {
		forgetKey()
		return null
	}
	return { key: found.key, saved: found.saved }
}

/// Keep `key`, saved now.
export function saveKey(key, now = Date.now()) {
	try {
		storage()?.setItem(NAME, JSON.stringify({ key, saved: new Date(now).toISOString() }))
	} catch {
		// Storage that will not take it leaves the key unkept.
	}
}

/// Remove the saved key.
export function forgetKey() {
	try {
		storage()?.removeItem(NAME)
	} catch {
		// As above.
	}
}
