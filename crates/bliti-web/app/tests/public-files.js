import { readdir } from 'node:fs/promises'
import { join, relative, sep } from 'node:path'
import { fileURLToPath } from 'node:url'

const PUBLIC = fileURLToPath(new URL('../public/', import.meta.url))

// The URL of every file in public/, which is served at its path under the root, nested folders and
// all.
export async function publicUrls() {
	const entries = await readdir(PUBLIC, { recursive: true, withFileTypes: true })
	return entries
		.filter((entry) => entry.isFile())
		.map((entry) => `/${relative(PUBLIC, join(entry.parentPath, entry.name)).split(sep).join('/')}`)
}
