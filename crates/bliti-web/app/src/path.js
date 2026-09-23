// The RFC 9535 Normalized Path of a node in a document, e.g. `$['attachments'][1]['gateway']`.
//
// A device names the part of a document it rejected by this path (BLI-CFG), and a Normalized Path is
// the one spelling of a node, so the screen finds the field an answer names by comparing strings
// rather than by reading the path back. The escapes are the RFC's, which are the device's.

/// The path of the node reached by `segments`: a string is a member name, a number an array index.
export function pathOf(segments) {
	let out = '$'
	for (const segment of segments) {
		out += typeof segment === 'number' ? `[${segment}]` : `['${escape(segment)}']`
	}
	return out
}

/// Whether `at` names the node at `path` or a node beneath it.
export function within(at, path) {
	if (typeof at !== 'string') return false
	return at === path || at.startsWith(`${path}[`)
}

const SEGMENT = /\[(\d+)\]|\['((?:[^'\\]|\\(?:u[0-9a-f]{4}|.))*)'\]/gy
const UNESCAPES = { b: '\b', f: '\f', n: '\n', r: '\r', t: '\t', "'": "'", '\\': '\\' }

/// The segments of a Normalized Path, as `pathOf` takes them: the inverse of `pathOf`. Null where `at`
/// is not a Normalized Path.
export function segmentsOf(at) {
	if (typeof at !== 'string' || !at.startsWith('$')) return null
	const segments = []
	SEGMENT.lastIndex = 1
	while (SEGMENT.lastIndex < at.length) {
		const match = SEGMENT.exec(at)
		if (!match) return null
		segments.push(
			match[1] !== undefined
				? Number(match[1])
				: match[2].replace(/\\(u[0-9a-f]{4}|.)/g, (_, c) => (c.length > 1 ? String.fromCharCode(parseInt(c.slice(1), 16)) : UNESCAPES[c])),
		)
	}
	return segments
}

const ESCAPES = { '\b': '\\b', '\f': '\\f', '\n': '\\n', '\r': '\\r', '\t': '\\t', "'": "\\'", '\\': '\\\\' }

function escape(name) {
	let out = ''
	for (const c of String(name)) {
		if (ESCAPES[c]) out += ESCAPES[c]
		else if (c < ' ') out += `\\u${c.charCodeAt(0).toString(16).padStart(4, '0')}`
		else out += c
	}
	return out
}
