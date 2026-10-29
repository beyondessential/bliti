// Upload the built bundle to an S3 prefix that a CloudFront distribution serves (WEB). The same
// script runs in CI and from a laptop with credentials, so a deploy by hand is the deploy CI makes.
//
//   node deploy.js s3://bucket/prefix/ [--prune] [--dry-run]
//
// What the distribution cannot work out for itself is set here, per object: the type, how long it
// may be cached, and which encoding a variant holds. It shells out to the AWS CLI, which finds
// credentials as it always does, rather than carrying an SDK for a few dozen puts.

import { execFile } from 'node:child_process'
import { readdir, stat } from 'node:fs/promises'
import { extname, join, relative, sep } from 'node:path'
import { fileURLToPath } from 'node:url'
import { promisify } from 'node:util'

const run = promisify(execFile)

// By extension, because the CLI guesses from the host's MIME database, which differs between a
// laptop and a runner. An extension with no entry here fails the deploy rather than being guessed.
const TYPES = new Map([
	['.css', 'text/css; charset=utf-8'],
	['.html', 'text/html; charset=utf-8'],
	['.ico', 'image/x-icon'],
	['.jpg', 'image/jpeg'],
	['.js', 'text/javascript; charset=utf-8'],
	['.json', 'application/json'],
	['.mjs', 'text/javascript; charset=utf-8'],
	['.png', 'image/png'],
	['.svg', 'image/svg+xml'],
	['.txt', 'text/plain; charset=utf-8'],
	['.wasm', 'application/wasm'],
	['.webmanifest', 'application/manifest+json'],
])

// The encodings precompress.js writes beside a file, by the suffix it writes them under.
const ENCODINGS = [
	['.br', 'br'],
	['.gz', 'gzip'],
	['.zst', 'zstd'],
]

// The types the distribution serves a variant of, by the key's suffix. Every file of one of these
// has all three variant keys, so a request for one the file has no encoding of is never a miss.
const VARIED = new Set(['.html', '.js', '.mjs', '.css', '.json', '.svg', '.wasm', '.webmanifest'])

// Vite names what it emits under assets/ by a hash of its content, so those never change under
// their name. Everything else is read again on every load.
const IMMUTABLE = 'public, max-age=31536000, immutable'
const REVALIDATE = 'no-cache'

// A client must never load a document that names files not yet uploaded: the hashed files go up
// first, then everything else, then the worker, then the pages. Within a phase the order is free.
const PHASES = { assets: 0, other: 1, worker: 2, pages: 3 }

async function* walk(dir) {
	for (const entry of await readdir(dir, { withFileTypes: true })) {
		const path = join(dir, entry.name)
		if (entry.isDirectory()) yield* walk(path)
		else yield path
	}
}

const phaseOf = (key) => {
	if (key.startsWith('assets/')) return PHASES.assets
	if (key === 'sw.js') return PHASES.worker
	if (key.endsWith('.html')) return PHASES.pages
	return PHASES.other
}

async function exists(path) {
	try {
		return (await stat(path)).isFile()
	} catch {
		return false
	}
}

// The objects a deploy of `dist` writes, in the order it writes them, with no AWS in the loop. Each
// is `{ key, file, contentType, cacheControl, contentEncoding }`, the key relative to the prefix.
export async function plan(dist) {
	const files = []
	for await (const path of walk(dist)) files.push(path)
	const present = new Set(files)

	const objects = []
	for (const path of files) {
		const ext = extname(path)
		// A variant is written under the key of the file it encodes, below.
		if (ENCODINGS.some(([suffix]) => ext === suffix) && present.has(path.slice(0, -ext.length))) continue

		const key = relative(dist, path).split(sep).join('/')
		const contentType = TYPES.get(ext)
		if (!contentType) throw new Error(`no content type is known for ${key}`)
		const cacheControl = key.startsWith('assets/') ? IMMUTABLE : REVALIDATE
		const phase = phaseOf(key)

		objects.push({ key, file: path, contentType, cacheControl, contentEncoding: null, phase })
		if (!VARIED.has(ext)) continue
		// Where there is no variant of a file (precompress.js leaves a file under its floor, or one
		// that would not shrink, as it is), the key holds the file itself, with no encoding. An origin
		// that serves precompressed files by name would call it what its name says, so this is not
		// precompress.js's to do.
		for (const [suffix, encoding] of ENCODINGS) {
			const variant = path + suffix
			const encoded = await exists(variant)
			objects.push({
				key: key + suffix,
				file: encoded ? variant : path,
				contentType,
				cacheControl,
				contentEncoding: encoded ? encoding : null,
				phase,
			})
		}
	}
	return objects.sort(
		(a, b) => a.phase - b.phase || (a.key < b.key ? -1 : a.key > b.key ? 1 : 0),
	)
}

// `s3://bucket/prefix/` as `{ bucket, prefix }`, the prefix ending in a slash. A deploy never
// writes at the root of a bucket: each role is allowed one prefix, and `--prune` deletes in it.
export function parseTarget(url) {
	const match = /^s3:\/\/([^/]+)\/(.*?)\/?$/.exec(url ?? '')
	if (!match || !match[2]) throw new Error(`expected s3://<bucket>/<prefix>/, got ${url}`)
	return { bucket: match[1], prefix: `${match[2]}/` }
}

const aws = (...args) => run('aws', args, { maxBuffer: 64 * 1024 * 1024 })

function upload({ bucket, prefix }, object) {
	const args = [
		's3',
		'cp',
		object.file,
		`s3://${bucket}/${prefix}${object.key}`,
		'--no-progress',
		'--only-show-errors',
		'--content-type',
		object.contentType,
		'--cache-control',
		object.cacheControl,
	]
	if (object.contentEncoding) args.push('--content-encoding', object.contentEncoding)
	return aws(...args)
}

// Keys under the prefix, from the CLI's own pagination, as it prints `null` where there are none.
async function listed({ bucket, prefix }) {
	const { stdout } = await aws(
		's3api',
		'list-objects-v2',
		'--bucket',
		bucket,
		'--prefix',
		prefix,
		'--query',
		'Contents[].Key',
		'--output',
		'json',
	)
	return (JSON.parse(stdout) ?? []).map((key) => key.slice(prefix.length))
}

// The deploy is what the bundle now is, so one failed object fails it; what was written stays, and
// the next deploy writes all of it again.
async function inPhases(objects, each) {
	const CONCURRENCY = 8
	for (const phase of new Set(objects.map((object) => object.phase))) {
		const queue = objects.filter((object) => object.phase === phase)
		await Promise.all(
			Array.from({ length: Math.min(CONCURRENCY, queue.length) }, async () => {
				for (let object = queue.shift(); object; object = queue.shift()) await each(object)
			}),
		)
	}
}

async function main(args) {
	const flags = new Set(args.filter((arg) => arg.startsWith('--')))
	const [url] = args.filter((arg) => !arg.startsWith('--'))
	for (const flag of flags) {
		if (!['--prune', '--dry-run'].includes(flag)) throw new Error(`unknown flag ${flag}`)
	}
	const target = parseTarget(url)

	const objects = await plan('dist')
	if (!objects.length) throw new Error('dist holds nothing to deploy; has it been built?')
	if (!objects.some((object) => object.key === 'index.html')) {
		throw new Error('dist has no index.html; has it been built?')
	}

	const dry = flags.has('--dry-run')
	await inPhases(objects, async (object) => {
		console.log(`${dry ? 'would put' : 'put'}  ${object.key}`)
		if (!dry) await upload(target, object)
	})

	if (flags.has('--prune')) {
		const wanted = new Set(objects.map((object) => object.key))
		for (const key of (await listed(target)).filter((key) => !wanted.has(key))) {
			console.log(`${dry ? 'would delete' : 'delete'}  ${key}`)
			if (!dry) await aws('s3', 'rm', `s3://${target.bucket}/${target.prefix}${key}`, '--only-show-errors')
		}
	}
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
	main(process.argv.slice(2)).catch((error) => {
		console.error(error.message)
		process.exit(1)
	})
}
