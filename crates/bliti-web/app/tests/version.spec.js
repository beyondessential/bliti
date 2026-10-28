import { execFileSync } from 'node:child_process'
import { mkdtemp, readFile, readdir, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { expect, test } from '@playwright/test'

// What a client reports itself as (WEB): the package version, tagged as build metadata with the
// build that made it where BLITI_BUILD names one, so a tester can tell which commit a deploy serves.
// Each test builds the shipped bundle once more, to a directory of its own, and reads the result.

async function bundleBuiltWith(build) {
	const out = await mkdtemp(join(tmpdir(), 'bliti-version-'))
	try {
		const env = { ...process.env }
		delete env.BLITI_BUILD
		if (build !== undefined) env.BLITI_BUILD = build
		execFileSync('npx', ['vite', 'build', '--outDir', out], { env, stdio: 'pipe' })
		const assets = (await readdir(join(out, 'assets'))).filter((file) => file.endsWith('.js'))
		const code = await Promise.all(assets.map((file) => readFile(join(out, 'assets', file), 'utf8')))
		return code.join('\n')
	} finally {
		await rm(out, { recursive: true, force: true })
	}
}

test('tags the version with the build that names itself', async () => {
	expect(await bundleBuiltWith('abc1234')).toContain('"0.0.0+abc1234"')
})

test('reports the package version alone where no build names itself', async () => {
	const code = await bundleBuiltWith(undefined)
	expect(code).toContain('"0.0.0"')
	expect(code).not.toContain('0.0.0+')
})

test('reports the package version alone where the build is named empty', async () => {
	expect(await bundleBuiltWith('')).not.toContain('0.0.0+')
})
