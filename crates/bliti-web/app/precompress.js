// Compress the built bundle once, at settings far too slow to run per request, and write each
// encoding beside the file it came from. An origin configured for it serves the variant the
// browser's Accept-Encoding allows and compresses nothing itself: Caddy's `precompressed`,
// nginx's `gzip_static` and `brotli_static`.
//
// All three are written because the choice belongs to whatever ends up serving this and to the
// browser asking. Brotli is the smallest on this bundle by a clear margin, zstd decodes faster,
// and gzip is understood by everything.

import { readFile, readdir, writeFile } from 'node:fs/promises'
import { extname, join } from 'node:path'
import { brotliCompressSync, constants, gzipSync, zstdCompressSync } from 'node:zlib'

const DIST = 'dist'

// Anything already compressed — images, fonts — only grows, so it is left alone.
const COMPRESSIBLE = new Set([
	'.css',
	'.html',
	'.js',
	'.json',
	'.svg',
	'.txt',
	'.wasm',
	'.webmanifest',
])

// Under this a response is one packet either way, and a variant would only be another file to
// serve and to keep in step.
const FLOOR = 1024

const ENCODINGS = [
	[
		'.br',
		(buf) =>
			brotliCompressSync(buf, {
				params: {
					[constants.BROTLI_PARAM_QUALITY]: constants.BROTLI_MAX_QUALITY,
					[constants.BROTLI_PARAM_SIZE_HINT]: buf.length,
				},
			}),
	],
	['.gz', (buf) => gzipSync(buf, { level: constants.Z_BEST_COMPRESSION })],
	[
		'.zst',
		(buf) =>
			zstdCompressSync(buf, {
				params: { [constants.ZSTD_c_compressionLevel]: 19 },
			}),
	],
]

async function* walk(dir) {
	for (const entry of await readdir(dir, { withFileTypes: true })) {
		const path = join(dir, entry.name)
		if (entry.isDirectory()) yield* walk(path)
		else yield path
	}
}

const totals = new Map(ENCODINGS.map(([suffix]) => [suffix, 0]))
let original = 0

for await (const path of walk(DIST)) {
	if (!COMPRESSIBLE.has(extname(path))) continue
	const source = await readFile(path)
	if (source.length < FLOOR) continue
	original += source.length
	for (const [suffix, compress] of ENCODINGS) {
		const compressed = compress(source)
		// A variant no smaller than what it encodes would cost a request and save nothing.
		if (compressed.length >= source.length) continue
		await writeFile(path + suffix, compressed)
		totals.set(suffix, totals.get(suffix) + compressed.length)
	}
}

const kb = (bytes) => `${(bytes / 1024).toFixed(2)} kB`
const summary = [...totals]
	.map(([suffix, bytes]) => `${suffix.slice(1)} ${kb(bytes)}`)
	.join(' │ ')
console.log(`precompressed  ${kb(original)} raw │ ${summary}`)
