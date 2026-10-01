// The service worker, served at /sw.js by a build and by the dev server alike (WEB). sw-plugin.js
// replaces the placeholder below with the precache list, and with whether precached entries are
// answered from the network first: a build answers them from the cache, and the dev server, whose
// modules change under it, from the network while there is one.
//
// A classic script with no imports, so the dev server can serve it as it is.

const { networkFirst, entries } = self.__BLITI_PRECACHE__

const PRECACHE_PREFIX = 'bliti-precache-'
const RUNTIME = 'bliti-runtime'
// Where each precache keeps the list it was filled from, so the next version can tell which of its
// entries it can take over rather than fetch again.
const LIST_KEY = '/__bliti-precache-list'
const DOCUMENT = '/'

// The dev server stamps a module with `t` once it has changed, so an HMR-stamped request finds the
// entry for its module and edits do not pile up one entry per save. A build has no `t`.
function keyOf(url) {
	const u = new URL(url, self.location.origin)
	u.hash = ''
	u.search = u.search
		.slice(1)
		.split('&')
		.filter((param) => param && param !== 't' && !param.startsWith('t='))
		.join('&')
	return u.pathname + u.search
}

const precached = new Set(entries.map((entry) => keyOf(entry.url)))

// Named after the list itself, so any change to it gets a cache of its own.
const precacheName = (async () => {
	const digest = await crypto.subtle.digest(
		'SHA-256',
		new TextEncoder().encode(JSON.stringify(entries)),
	)
	const hex = [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('')
	return PRECACHE_PREFIX + hex.slice(0, 16)
})()

async function fetchOk(url) {
	const res = await fetch(url, { cache: 'no-cache' })
	if (!res.ok) throw new Error(`precaching ${url}: status ${res.status}`)
	return res
}

// An entry whose URL and revision an older precache already holds is copied from it, so a release
// that leaves the wasm module alone does not download it again.
async function carried(name) {
	const found = new Map()
	for (const other of await caches.keys()) {
		if (!other.startsWith(PRECACHE_PREFIX) || other === name) continue
		const cache = await caches.open(other)
		const list = await cache.match(LIST_KEY)
		if (!list) continue
		for (const entry of await list.json()) {
			const key = `${keyOf(entry.url)} ${entry.revision}`
			if (found.has(key)) continue
			const res = await cache.match(keyOf(entry.url))
			if (res) found.set(key, res)
		}
	}
	return found
}

// There is no skipWaiting: a new version waits until no page of the old one is open. Activating
// deletes the precache the new version replaces, and the page loads the wasm module lazily, so a
// page left running across a takeover would ask the network for a module that is gone, which
// offline fails.
self.addEventListener('install', (event) => {
	event.waitUntil(
		(async () => {
			const name = await precacheName
			// A version that changes the worker and not its list shares the precache in use, filled
			// already: the list is written last, so a precache holding it is complete.
			if (await (await caches.open(name)).match(LIST_KEY)) return
			const old = await carried(name)
			const cache = await caches.open(name)
			try {
				await Promise.all(
					entries.map(async (entry) => {
						const key = keyOf(entry.url)
						const res = old.get(`${key} ${entry.revision}`)?.clone() ?? (await fetchOk(entry.url))
						await cache.put(key, res)
					}),
				)
				await cache.put(LIST_KEY, new Response(JSON.stringify(entries)))
			} catch (error) {
				// Filled only in part, it must not be taken for a complete one by a later version.
				await caches.delete(name)
				throw error
			}
		})(),
	)
})

// Claiming is kept, so a first visit is served from the cache once it is filled rather than only
// after a reload.
self.addEventListener('activate', (event) => {
	event.waitUntil(
		(async () => {
			const name = await precacheName
			for (const other of await caches.keys()) {
				if (other.startsWith(PRECACHE_PREFIX) && other !== name) await caches.delete(other)
			}
			await self.clients.claim()
		})(),
	)
})

// How long a request waits on the network before a stored copy answers it. A phone in airplane mode
// with a VPN up, as on the tailnet, holds a connection open rather than failing it.
const PATIENCE = 3000

// Set once the network has kept a request waiting past PATIENCE, so the rest of a load is answered
// from the cache at once rather than each import waiting its turn behind the one before. Cleared by
// the next answer the network gives; a navigation always asks it, so a page loaded once the network
// is back finds it.
let unreachable = false

// The network's answer, stored under key in cacheName where it is one worth keeping, or the stored
// one where the network fails or keeps the request waiting.
async function fromNetwork(request, cacheName, key) {
	const cache = await caches.open(cacheName)
	const stored = await cache.match(key)
	if (stored && unreachable && request.mode !== 'navigate') return stored
	const fetched = fetch(request).then(async (res) => {
		unreachable = false
		if (res.ok) await cache.put(key, res.clone())
		return res
	})
	if (!stored) return fetched
	// Answered late or not at all, it still stores what it brings for the next load.
	fetched.catch(() => {})
	const late = new Promise((resolve) => setTimeout(resolve, PATIENCE, null))
	try {
		const res = await Promise.race([fetched, late])
		if (res) return res
		unreachable = true
	} catch {
		// Failed outright: the stored copy answers.
	}
	return stored
}

async function respond(request) {
	const key = request.mode === 'navigate' ? DOCUMENT : keyOf(request.url)
	if (precached.has(key)) {
		const name = await precacheName
		if (networkFirst) return fromNetwork(request, name, key)
		const stored = await (await caches.open(name)).match(key)
		if (stored) return stored
	}
	return fromNetwork(request, RUNTIME, key)
}

self.addEventListener('fetch', (event) => {
	const { request } = event
	const url = new URL(request.url)
	if (request.method !== 'GET' || url.origin !== self.location.origin) return
	if (url.pathname === '/sw.js') return
	event.respondWith(respond(request))
})
