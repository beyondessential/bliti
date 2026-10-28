# Deploy the web app

The browser client is built to static files (`just build`, into `crates/bliti-web/app/dist`) and hosted from an S3 bucket behind CloudFront at iti.tamanu.app.
Every same-repo pull request gets a temporary deploy of its own.
This card is the bliti side: preparing the bundle for that origin and the GitHub Actions workflows that build and upload it.
The bucket, distribution, certificate, DNS, CloudFront function and OIDC roles are the infra side, on infra#V3.

## Per-PR deploys: subdomains, not a subpath

Each pull request deploys to its own origin, `pr-<number>.iti.tamanu.app`, rather than to `iti.tamanu.app/_pr/<number>/`.

A subpath shares production's origin, and the client is built around owning its origin:

- Production's service worker is registered at `/sw.js` with scope `/`, so it controls every page under the origin, the PR paths included. Its fetch handler answers every navigation with the precached `/` document, so on any phone that has opened production, a PR link opens production. Making it work means production's worker learning to skip `/_pr/`.
- A worker's `activate` deletes every `bliti-precache-*` cache but its own. A PR's worker and production's would delete each other's precache, and production would stop being ready offline on every phone used for QA, which are also the phones used to set up real devices.
- Cache Storage is per origin, so code from an unmerged PR running on the shared origin can write to production's precache and replace the document production's worker serves at `/`. That persists after the PR deploy is gone. Fork PRs get no OIDC token so never deploy, but a same-repo PR is still unreviewed code by definition.
- The rest is mechanical but wide: `/sw.js`, `/manifest.webmanifest`, the icons, the manifest's `start_url` and `id`, and the precache list in `sw-plugin.js` are all absolute, and an installed PR deploy would collide with an installed production app under the same manifest `id`.

A subdomain gives each deploy its own service worker, caches, storage, Bluetooth permissions and install identity, and the bundle needs no base path at all: the same build serves production and a PR.
What it costs is on the infra side: a wildcard certificate and DNS record for `*.iti.tamanu.app` aliased to the distribution, and mapping each host to its bucket prefix.

## Contract with the infra side

What bliti's upload writes and what infra#V3's distribution reads.

### Bucket layout

- Production at `prod/`, each pull request at `pr/<number>/`. Keeping production under a prefix of its own makes the two roles' write permissions disjoint.
- Within a prefix, the bundle as `dist` holds it, at the same relative paths.

### What the distribution does with it

infra#V3's to design; bliti relies on:

- Host to prefix: `iti.tamanu.app` serves `prod/`, `pr-<number>.iti.tamanu.app` serves `pr/<number>/`.
- Directory index: a path with no extension serves `<path>/index.html`, so `/` is `index.html` and the later `/provision` and `/assemble` entry points are `provision/index.html` and `assemble/index.html`. No single-page fallback: the client keeps its screens in history state, not in the URL.
- The precompressed variants served as WEB's Delivery section requires. How the distribution chooses among them is the infra side's.

### Object metadata the upload sets

- `Content-Type` explicitly per file, from its extension rather than the CLI's guess, and on an encoded variant, the type of the file it encodes.
- `Content-Encoding` on each variant.
- Every file of a type the distribution serves variants for (`.html`, `.js`, `.mjs`, `.css`, `.json`, `.svg`, `.wasm`, `.webmanifest`) has all three variant keys beside its plain one, whatever its size, as the infra side requires. Other types, the icons among them, are uploaded plain only. `precompress.js` skips a variant under its size floor or where encoding would not shrink the file; the upload fills each skipped key with the plain file and no `Content-Encoding`. The filling happens at upload, not in `precompress.js`: an origin that serves precompressed files by name, such as Caddy's `precompressed`, would label a plain copy at `.br` as brotli.
- `Cache-Control`: `public, max-age=31536000, immutable` under `assets/` (Vite's hashed output); `no-cache` on everything else (`index.html`, `sw.js`, the manifest and the icons from `public/`), so a deploy is visible on the next load without an invalidation.

## GitHub Actions

### Roles and environments

- Production: GitHub environment `production`, restricted to `main`. Its OIDC role is trusted for `repo:beyondessential/bliti:environment:production` and may write `prod/*`.
- Previews: GitHub environment `preview`, unrestricted, with the deploy's URL set per run so the PR shows a "View deployment" link. Its role is trusted for `repo:beyondessential/bliti:environment:preview` and may write and delete `pr/*`.
- A `pull_request` run takes its workflow from the PR head, so a PR could name `production` in its own copy of the workflow. The environment's branch restriction is what stops that, not the workflow file.
- Neither role needs invalidation rights given the cache-control above.

### Workflows

- The steps that build the bundle move from `ci.yml`'s `web` job into a local composite action that both `ci.yml` and `cd.yml` use.
- `cd.yml` triggers on push to `main` (every merge deploys production until the first release; revisit for tags then) and on `pull_request` (opened, synchronize, reopened, closed).
  - Build: the composite action, skipped on `closed`.
  - Deploy production: on push to `main`, environment `production`, uploads to `prod/`.
  - Deploy preview: on a same-repo PR not closed (`head.repo.full_name == github.repository`), environment `preview`, uploads to `pr/<number>/`.
  - Tear down preview: on `closed`, environment `preview`, deletes `pr/<number>/`.
  - Concurrency per ref, not cancelled in progress, so two uploads to one prefix never interleave and a teardown never races a deploy.
- Fork PRs build in CI as now and do not deploy.

### Action versions

Latest releases as of writing, checked with `gh api repos/<action>/releases/latest`; recheck when implementing:

- `actions/checkout@v7.0.1`
- `actions/setup-node@v7.1.0` (ci.yml has v4)
- `actions/upload-artifact@v7.0.2` (ci.yml has v4)
- `actions/download-artifact@v8.0.2`
- `aws-actions/configure-aws-credentials@v6.3.0`
- `taiki-e/install-action@v2`, `Swatinem/rust-cache@v2`

ci.yml's v4 pins move up with the composite action, since the two share those steps. Dependabot already tracks github-actions.

## Upload

- Upload order: everything under `assets/` first, then the rest, `sw.js` and `index.html` last, so a client never loads a document that names files not yet uploaded.
- Previews delete files the new bundle no longer has, after uploading. Production keeps superseded hashed assets, so a page open across a deploy can still lazily load the wasm module it was built against; they are a few files per release.
- Written as a script beside `precompress.js`, run by a `just deploy <s3 url>` recipe, so a deploy from a laptop with credentials is the same as one from CI. It shells out to the AWS CLI rather than taking on the AWS SDK as a dependency.

## Version identity

A built bundle reports `package.json`'s version alone, `0.0.0`, which cannot tell one deploy from another.
The deploy build tags it with the commit as build metadata (`0.0.0+<short sha>`), as the dev server already does and as `BLITI_BUILD` does for the daemon, so a tester can tell which commit a preview is serving and the device's log names it.

## Open

- Region and account for the bucket, the bucket name, and the role ARNs: from infra#V3.
