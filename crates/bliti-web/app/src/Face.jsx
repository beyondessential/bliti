// An entry's value as a face shows it, shared by the screens that render facts and readings (VIEW).

import { formatMargin, headline } from './readings.js'

/// The headline with the entry's margin beside it, where it carries one.
export function face(entry) {
	return withMargin(entry, headline(entry))
}

/// A value with how far either way it may be off beside it, for any reading carrying a margin (VIEW).
export function withMargin(entry, text) {
	const margin = formatMargin(entry)
	if (!margin) return text
	return (
		<>
			{text}
			<span className="margin">{margin}</span>
		</>
	)
}
