import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'

import { expect, test } from '@playwright/test'

import { parseTarget, plan } from '../deploy.js'

// What a deploy writes and how each object is labelled (WEB), as the plan for a fixture bundle. The
// plan is a pure function of the directory, so nothing here reaches AWS.

async function bundle(files) {
	const dist = await mkdtemp(join(tmpdir(), 'bliti-deploy-'))
	for (const file of files) {
		await mkdir(dirname(join(dist, file)), { recursive: true })
		await writeFile(join(dist, file), file)
	}
	return dist
}

// The bundle a build and precompress.js leave: a script with every variant, a module and a
// stylesheet with one each, and documents and a worker small enough to have none.
const FILES = [
	'index.html',
	'provision.html',
	'sw.js',
	'manifest.webmanifest',
	'icon.svg',
	'icon-192.png',
	'assets/app-AAAA.js',
	'assets/app-AAAA.js.br',
	'assets/app-AAAA.js.gz',
	'assets/app-AAAA.js.zst',
	'assets/app-BBBB.wasm',
	'assets/app-BBBB.wasm.br',
	'assets/style-CCCC.css',
	'assets/board-DDDD.jpg',
]

const planned = async (files = FILES) => {
	const dist = await bundle(files)
	try {
		const objects = await plan(dist)
		return { dist, objects, byKey: new Map(objects.map((object) => [object.key, object])) }
	} finally {
		await rm(dist, { recursive: true, force: true })
	}
}

test('writes every file once under its own key, a variant under the key of what it encodes', async () => {
	const { objects } = await planned()
	const keys = objects.map((object) => object.key)
	expect(new Set(keys).size).toBe(keys.length)
	expect(keys.sort()).toEqual(
		[
			'assets/app-AAAA.js',
			'assets/app-AAAA.js.br',
			'assets/app-AAAA.js.gz',
			'assets/app-AAAA.js.zst',
			'assets/app-BBBB.wasm',
			'assets/app-BBBB.wasm.br',
			'assets/app-BBBB.wasm.gz',
			'assets/app-BBBB.wasm.zst',
			'assets/board-DDDD.jpg',
			'assets/style-CCCC.css',
			'assets/style-CCCC.css.br',
			'assets/style-CCCC.css.gz',
			'assets/style-CCCC.css.zst',
			'icon-192.png',
			'icon.svg',
			'icon.svg.br',
			'icon.svg.gz',
			'icon.svg.zst',
			'index.html',
			'index.html.br',
			'index.html.gz',
			'index.html.zst',
			'manifest.webmanifest',
			'manifest.webmanifest.br',
			'manifest.webmanifest.gz',
			'manifest.webmanifest.zst',
			'provision.html',
			'provision.html.br',
			'provision.html.gz',
			'provision.html.zst',
			'sw.js',
			'sw.js.br',
			'sw.js.gz',
			'sw.js.zst',
		].sort(),
	)
})

test('types each file by its extension, and each variant as the file it encodes', async () => {
	const { byKey } = await planned()
	const types = {
		'index.html': 'text/html; charset=utf-8',
		'sw.js': 'text/javascript; charset=utf-8',
		'manifest.webmanifest': 'application/manifest+json',
		'icon.svg': 'image/svg+xml',
		'icon-192.png': 'image/png',
		'assets/board-DDDD.jpg': 'image/jpeg',
		'assets/app-BBBB.wasm': 'application/wasm',
		'assets/style-CCCC.css': 'text/css; charset=utf-8',
	}
	for (const [key, type] of Object.entries(types)) {
		expect(byKey.get(key).contentType).toBe(type)
		for (const suffix of /\.(png|jpg)$/.test(key) ? [] : ['.br', '.gz', '.zst']) {
			expect(byKey.get(key + suffix).contentType).toBe(type)
		}
	}
})

test('labels an encoded variant with its encoding, and uploads the file precompress.js wrote', async () => {
	const { byKey } = await planned()
	for (const [suffix, encoding] of [
		['.br', 'br'],
		['.gz', 'gzip'],
		['.zst', 'zstd'],
	]) {
		const variant = byKey.get(`assets/app-AAAA.js${suffix}`)
		expect(variant.contentEncoding).toBe(encoding)
		expect(variant.file.endsWith(`assets/app-AAAA.js${suffix}`)).toBe(true)
	}
	expect(byKey.get('assets/app-AAAA.js').contentEncoding).toBeNull()
})

test('fills a variant key precompress.js skipped with the plain file and no encoding', async () => {
	const { byKey } = await planned()
	for (const key of ['index.html.br', 'sw.js.zst', 'assets/app-BBBB.wasm.gz', 'assets/app-BBBB.wasm.zst']) {
		const filled = byKey.get(key)
		expect(filled.contentEncoding).toBeNull()
		expect(filled.file).toBe(byKey.get(key.replace(/\.(br|gz|zst)$/, '')).file)
	}
})

test('writes no variant of a type the distribution serves none of', async () => {
	const { byKey } = await planned()
	expect(byKey.has('icon-192.png.br')).toBe(false)
	expect(byKey.has('icon-192.png.gz')).toBe(false)
	expect(byKey.has('icon-192.png.zst')).toBe(false)
	expect(byKey.has('assets/board-DDDD.jpg.br')).toBe(false)
})

test('lets a hashed file be cached for good, and has everything else revalidated', async () => {
	const { objects } = await planned()
	for (const object of objects) {
		expect(object.cacheControl).toBe(
			object.key.startsWith('assets/') ? 'public, max-age=31536000, immutable' : 'no-cache',
		)
	}
})

test('writes the hashed files, then the rest, then the worker, then the pages', async () => {
	const { objects } = await planned()
	const at = (pred) => objects.map((object) => pred(object.key))
	const last = (flags) => flags.lastIndexOf(true)
	const first = (flags) => flags.indexOf(true)

	const assets = at((key) => key.startsWith('assets/'))
	const worker = at((key) => key === 'sw.js' || key.startsWith('sw.js.'))
	const pages = at((key) => /\.html(\.|$)/.test(key))
	const rest = at((key) => !key.startsWith('assets/') && !/^sw\.js|\.html(\.|$)/.test(key))

	expect(last(assets)).toBeLessThan(first(rest))
	expect(last(rest)).toBeLessThan(first(worker))
	expect(last(worker)).toBeLessThan(first(pages))
})

test('refuses a file whose type it does not know', async () => {
	await expect(planned([...FILES, 'assets/mystery.xyz'])).rejects.toThrow(/mystery\.xyz/)
})

test.describe('the target', () => {
	test('is a bucket and a prefix ending in a slash', () => {
		expect(parseTarget('s3://bes-bliti-web/prod/')).toEqual({ bucket: 'bes-bliti-web', prefix: 'prod/' })
		expect(parseTarget('s3://bes-bliti-web/previews/pr-7')).toEqual({
			bucket: 'bes-bliti-web',
			prefix: 'previews/pr-7/',
		})
	})

	test('is never the root of a bucket', () => {
		expect(() => parseTarget('s3://bes-bliti-web')).toThrow()
		expect(() => parseTarget('s3://bes-bliti-web/')).toThrow()
		expect(() => parseTarget(undefined)).toThrow()
		expect(() => parseTarget('https://example.com/prod/')).toThrow()
	})
})
