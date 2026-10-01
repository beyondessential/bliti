// Whether the application is ready offline, and keeping it so (WEB). The same worker in a build and
// on the dev server; a browser without one still runs the page, only not offline.

import { useEffect, useState } from 'react'

// A worker takes control only once its precache holds everything the application needs to load, so
// a page it controls is ready offline.
const isReady = () => Boolean(navigator.serviceWorker?.controller)

export function keepReady() {
	const workers = navigator.serviceWorker
	if (!workers) return
	const register = () =>
		workers.register('/sw.js', { scope: '/' }).catch((error) => {
			console.warn('service worker not registered', error)
		})
	register()
	// An install cut short by a lost connection leaves nothing behind, so it is tried again on the next.
	window.addEventListener('online', () => {
		if (!isReady()) register()
	})
}

export function useOfflineReady() {
	const [ready, setReady] = useState(isReady)
	useEffect(() => {
		const workers = navigator.serviceWorker
		if (!workers) return
		const update = () => setReady(isReady())
		workers.addEventListener('controllerchange', update)
		update()
		return () => workers.removeEventListener('controllerchange', update)
	}, [])
	return ready
}
