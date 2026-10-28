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

The infra repository's Bliti web app hosting spec (BLWEB, on infra#V3) is the contract: bucket prefixes, host and path routing, variants, and the deploy roles' subjects and prefixes. What bliti relies on from it, and why:

### Bucket layout

- Production at `prod/`, each pull request at `previews/pr-<number>/`. Keeping production under a prefix of its own makes the two roles' write permissions disjoint.
- Within a prefix, the bundle as `dist` holds it, at the same relative paths.
- Everything under `previews/` expires 30 days after its upload.

### What the distribution does with it

- Host to prefix: `iti.tamanu.app` serves `prod/`, `pr-<number>.iti.tamanu.app` serves `previews/pr-<number>/`.
- Pages: `/` serves `index.html`, and any other path with no extension serves that path plus `.html`, so `/provision` is `provision.html`. The page-to-variant step comes after this one, so a page gets its encodings like any other `.html`.
- Trailing slash: a path other than `/` ending in `/` is a permanent redirect to the same path without it, query kept, so `/provision/` goes to `/provision`. Each page has one URL, and so one history entry and one key in the service worker, while a slash typed or carried in a printed link still lands on the page.
- A missing key is a 404, not the 403 S3 gives where the distribution cannot list the bucket, and never falls back to `index.html`: the client keeps its screens in history state, not in the URL, so there is nothing for a single-page fallback to catch, and one would answer a mistyped page with the operator app.
- The precompressed variants served as WEB's Delivery section requires, zstd preferred, then brotli, then gzip.

### Object metadata the upload sets

- `Content-Type` explicitly per file, from its extension rather than the CLI's guess, and on an encoded variant, the type of the file it encodes.
- `Content-Encoding` on each variant.
- Every file of a type the distribution serves variants for (`.html`, `.js`, `.mjs`, `.css`, `.json`, `.svg`, `.wasm`, `.webmanifest`) has all three variant keys beside its plain one, whatever its size, as the infra side requires. Other types, the icons among them, are uploaded plain only. `precompress.js` skips a variant under its size floor or where encoding would not shrink the file; the upload fills each skipped key with the plain file and no `Content-Encoding`. The filling happens at upload, not in `precompress.js`: an origin that serves precompressed files by name, such as Caddy's `precompressed`, would label a plain copy at `.br` as brotli.
- `Cache-Control`: `public, max-age=31536000, immutable` under `assets/` (Vite's hashed output); `no-cache` on everything else (`index.html`, `sw.js`, the manifest and the icons from `public/`), so a deploy is visible on the next load without an invalidation.

## Pages

The operator app is the only page now; F4's cards add the provisioning client at `/provision` and the assembly manual at `/assemble`.
This card puts in what any page after the first needs, so adding one is adding its HTML file and its code.

- Each page is an HTML file at the top of the app, `provision.html` beside `index.html`, built to `dist/provision.html`. Shared code lands once under `assets/`.
- Flat rather than `provision/index.html` because that is how Vite's dev server resolves `/provision`: it tries `provision.html` and otherwise falls back to `index.html`, so a directory page would be the operator app under the dev server and the provisioning client when deployed. The distribution's rule is the dev server's, so a path means the same page on both.
- `vite.config.js` takes every `.html` file at the app's top level as a build input, rather than a list kept beside the files.
- The dev server's single-page fallback goes (`appType: 'mpa'`), so a path that is no page is a 404 under it too, as deployed. `/provision/` is also a 404 there, where deployed it redirects; our own links never carry the slash.
- `sw-plugin.js` lists each page under its path, `provision.html` under `/provision` as `index.html` is under `/`, and the dev server's list walks the module graph from every page, not from `index.html` alone.
- `sw.js` answers a navigation with the precached document of the page at its path, a trailing slash dropped, so `/provision/` opens offline too, where the distribution's redirect cannot be reached. A navigation to a path that is no page goes to the network.
- The worker precaches every page, as it precaches the whole bundle now. Whether the operator app's worker should leave out other pages and their images, which every field phone would otherwise download, is the first new page's card's to decide.
- Tests: the test-mode build carries a fixture page beside the operator app, as it carries the harness's seam, so the harness covers loading a second page, opening it offline, and `/provision/`-style keys without a real page existing yet.
- Nothing in the deploy is per page: the upload walks `dist`, and the distribution's rules are generic, so a new page needs no change to either.

## GitHub Actions

### Roles and environments

- Production: the deploy job runs in GitHub environment `production`, which allows deployments from `main` and from tags matching `v*` only, set up in the repository settings as a branch rule and a tag rule. Its role trusts the subject `repo:beyondessential/bliti:environment:production` and writes `prod/`.
- Previews: the deploy and teardown jobs run in no environment, because the preview role trusts the subject `repo:beyondessential/bliti:pull_request`, and a job in an environment carries the environment's subject instead. It writes `previews/`, and cannot tell one PR's preview from another's.
- A `pull_request` run takes its workflow from the PR head, so a PR could name `production` in its own copy of the workflow. The environment's branch restriction is what stops that, not the workflow file. The `production` environment, with that restriction, is set up in the bliti repository's settings.
- Both roles may invalidate; nothing here does, given the cache-control above.
- With no environment on the preview job, the link to a preview comes from a GitHub deployment the job creates through the API (environment `preview`, its URL on the deployment status), so the PR shows "View deployment" without changing the job's subject. Teardown marks it inactive. That needs `deployments: write`, beside `id-token: write`.

### Workflows

- The steps that build the bundle move from `ci.yml`'s `web` job into a local composite action that both `ci.yml` and `cd.yml` use.
- `cd.yml` triggers on push to `main` (every merge deploys production until the first release; revisit for tags then) and on `pull_request` (opened, synchronize, reopened, closed).
  - Build: the composite action, skipped on `closed`.
  - Deploy production: on push to `main`, environment `production`, uploads to `prod/`.
  - Deploy preview: on a same-repo PR not closed (`head.repo.full_name == github.repository`), uploads to `previews/pr-<number>/`.
  - Tear down preview: on `closed`, deletes `previews/pr-<number>/`.
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
- Every deploy uploads every file, changed or not, rather than syncing only what differs. Preview objects expire 30 days after their upload, so a sync would leave a long-lived PR's unchanged files to expire under the rest of its preview. A PR left without a push for 30 days loses its preview, and its next push brings it back.
- Previews delete files the new bundle no longer has, after uploading. Production keeps superseded hashed assets, so a page open across a deploy can still lazily load the wasm module it was built against; they are a few files per release.
- Written as a script beside `precompress.js`, run by a `just deploy <s3 url>` recipe, so a deploy from a laptop with credentials is the same as one from CI. It shells out to the AWS CLI rather than taking on the AWS SDK as a dependency.

## Version identity

A built bundle reports `package.json`'s version alone, `0.0.0`, which cannot tell one deploy from another.
The deploy build tags it with the commit as build metadata (`0.0.0+<short sha>`), as the dev server already does and as `BLITI_BUILD` does for the daemon, so a tester can tell which commit a preview is serving and the device's log names it.

## AWS resources

From infra#V3, in account 143295493206, region ap-southeast-2:

- Bucket `bes-bliti-web`.
- Distribution `E2STLUOPU1SHP5`. The workflows do not need it, as nothing invalidates.
- Production role `arn:aws:iam::143295493206:role/gha-bliti-web-prod`, assumed under environment `production`.
- Preview role `arn:aws:iam::143295493206:role/gha-bliti-web-preview`, assumed by a `pull_request` job in no environment.

These go in `cd.yml`'s `env`, as linux-images keeps its role, rather than in repository variables: none of them is secret, and the workflow reads complete without a trip to the settings.
