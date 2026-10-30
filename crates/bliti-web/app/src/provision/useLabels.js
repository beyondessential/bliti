// The page's hold on its list of labels: what is in it, kept current as other tabs change it, and
// the changes the screens make (PSHT).

import { useCallback, useEffect, useState } from 'react'

import { DURABLE_KEPT, STORAGE_NAME, change, offered, printed, read, removed, unprinted } from './labels.js'

export default function useLabels() {
	const [list, setList] = useState(() => read())

	// Another tab changed it.
	useEffect(() => {
		const update = (event) => {
			if (event.key === STORAGE_NAME || event.key === null) setList(read())
		}
		window.addEventListener('storage', update)
		return () => window.removeEventListener('storage', update)
	}, [])

	const apply = useCallback((how) => {
		const next = change(how)
		if (next) setList(next)
	}, [])

	return {
		list,
		unprinted: unprinted(list),
		kept: DURABLE_KEPT,
		offer: useCallback((label) => apply((held) => offered(held, label)), [apply]),
		markPrinted: useCallback((payloads) => apply((held, now) => printed(held, payloads, now)), [apply]),
		remove: useCallback((payload) => apply((held, now) => removed(held, payload, now)), [apply]),
	}
}
