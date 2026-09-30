// The application is served from a hosted origin and built to static files (WEB). It is installable
// and works offline once loaded, so a phone that has opened it before is useful at a site with no
// connectivity; the wasm module is part of what is cached, because the protocol is in it and the
// application is nothing without it.
//
// One worker, src/sw.js, is served at /sw.js by a build and by the dev server, so what is tried on a
// phone against the dev server is the worker that ships. A build lists its bundle for the precache
// and answers from it; the dev server lists the module graph it would serve and answers from the
// network while there is one, which keeps live reload working.

import { createHash } from 'node:crypto'
import { readFile, readdir } from 'node:fs/promises'
import { extname, join, relative } from 'node:path'

import { init, parse } from 'es-module-lexer'

const SOURCE = new URL('./src/sw.js', import.meta.url)
const TOKEN = 'self.__BLITI_PRECACHE__'
const BUNDLED = new Set(['.js', '.css', '.html', '.wasm'])

export function inject(source, list) {
	if (!source.includes(TOKEN)) throw new Error(`the service worker does not carry ${TOKEN}`)
	return source.replace(TOKEN, JSON.stringify(list))
}

const worker = async (list) => inject(await readFile(SOURCE, 'utf8'), list)

const revision = (bytes) => createHash('sha256').update(bytes).digest('hex').slice(0, 16)

// The same as the worker's own key: a `t` stamp would change the list, and so the worker, with every
// edit that reaches an importer.
function unstamped(url) {
	const [path, query = ''] = url.split('?')
	const kept = query.split('&').filter((param) => param && param !== 't' && !param.startsWith('t='))
	return kept.length ? `${path}?${kept.join('&')}` : path
}

async function files(dir) {
	const found = []
	for (const entry of await readdir(dir, { withFileTypes: true, recursive: true })) {
		if (entry.isFile()) found.push(join(entry.parentPath, entry.name))
	}
	return found
}

async function publicEntries(publicDir, withRevision) {
	const entries = []
	for (const file of await files(publicDir)) {
		const url = `/${relative(publicDir, file).split('\\').join('/')}`
		entries.push({ url, revision: withRevision ? revision(await readFile(file)) : null })
	}
	return entries
}

// The modules a piece of code imports or re-exports from, by the specifier written in it.
function specifiers(code) {
	const [imports, exports] = parse(code)
	return [
		...imports.map((imp) => imp.specifier),
		...exports.map((exp) => exp.from),
	].filter((spec) => typeof spec === 'string')
}

// Every URL a browser asks the dev server for on loading the application, found by transforming
// each module as the browser would get it and following the imports in what comes back. The
// transformed code, not the module graph, because the graph records a module without the query the
// browser asks for it by.
async function devList(server) {
	await init()
	const env = server.environments.client
	const raw = await readFile(join(server.config.root, 'index.html'), 'utf8')
	const html = await server.transformIndexHtml('/', raw)

	const pending = []
	for (const [, src] of html.matchAll(/<script\b[^>]*\ssrc="([^"]+)"/g)) pending.push(src)
	for (const [, body] of html.matchAll(/<script\b(?![^>]*\ssrc=)[^>]*>([\s\S]*?)<\/script>/g)) {
		pending.push(...specifiers(body))
	}

	const listed = new Set(['/'])
	const walked = new Set()
	while (pending.length) {
		const url = unstamped(pending.pop())
		if (!url.startsWith('/') || walked.has(url)) continue
		walked.add(url)
		listed.add(url)
		const result = await env.transformRequest(url)
		if (!result) throw new Error(`the dev server cannot serve ${url}`)
		if (new URLSearchParams(url.split('?')[1] ?? '').has('url')) {
			// Such a module exports the URL of a file the page then fetches as it is.
			const [, file] = result.code.match(/export default "([^"]+)"/) ?? []
			if (!file) throw new Error(`no URL in what the dev server serves for ${url}`)
			listed.add(file)
			continue
		}
		for (const spec of specifiers(result.code)) {
			pending.push(new URL(spec, `http://dev${url}`).href.slice('http://dev'.length))
		}
	}
	return [...listed]
}

export default function serviceWorker() {
	let config
	return {
		name: 'bliti-service-worker',
		configResolved(resolved) {
			config = resolved
		},
		configureServer(server) {
			server.middlewares.use(async (req, res, next) => {
				if (req.url.split('?')[0] !== '/sw.js') return next()
				try {
					const modules = (await devList(server)).map((url) => ({ url, revision: null }))
					const entries = [...modules, ...(await publicEntries(config.publicDir, false))]
					entries.sort((a, b) => (a.url < b.url ? -1 : a.url > b.url ? 1 : 0))
					res.setHeader('Content-Type', 'text/javascript')
					res.setHeader('Cache-Control', 'no-cache')
					res.end(await worker({ networkFirst: true, entries }))
				} catch (error) {
					next(error)
				}
			})
		},
		generateBundle: {
			// After the HTML is emitted, so index.html is in the bundle to be listed.
			order: 'post',
			async handler(_, bundle) {
				const entries = []
				for (const file of Object.values(bundle)) {
					if (!BUNDLED.has(extname(file.fileName))) continue
					const bytes = file.type === 'chunk' ? file.code : file.source
					const url = file.fileName === 'index.html' ? '/' : `/${file.fileName}`
					entries.push({ url, revision: revision(bytes) })
				}
				entries.push(...(await publicEntries(config.publicDir, true)))
				entries.sort((a, b) => (a.url < b.url ? -1 : a.url > b.url ? 1 : 0))
				this.emitFile({
					type: 'asset',
					fileName: 'sw.js',
					source: await worker({ networkFirst: false, entries }),
				})
			},
		},
	}
}
