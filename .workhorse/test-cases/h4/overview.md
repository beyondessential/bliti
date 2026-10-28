# Deploy the web app

## Pages

- [x] A second page is built and served at its path, with a script of its own (verifies spec: WEB)
- [x] A path that is no page is a 404, with and without a service worker controlling the origin (verifies spec: WEB)
- [x] A page opens offline at its path after one load of the operator app (verifies spec: WEB)
- [x] A page opens offline at its path with a trailing slash (verifies spec: WEB)
- [x] The worker's list carries every page under its path, in the build and in the dev server

## Version identity

- [x] A build with a name carries `<version>+<name>` in the bundle
- [x] A build with no name, or an empty one, reports the package version alone

## Upload plan

- [x] Every file has one key, and a variant sits under the key of the file it encodes
- [x] Types come from the extension, variants typed as the file they encode, an unknown extension fails the plan
- [x] A variant `precompress.js` skipped is filled with the plain file and no `Content-Encoding`
- [x] Types the distribution serves no variant of get none
- [x] Hashed files are `immutable`, everything else `no-cache`
- [x] Order: hashed files, the rest, the worker, the pages
- [x] A target is a bucket and a prefix, never the bucket root

## Live

- [ ] This card's PR deploys `pr-<n>.iti.tamanu.app`, it reports a version naming the head commit
- [ ] `curl` with each `Accept-Encoding` gets the matching variant, `Content-Type`, `Cache-Control` and `Vary`
- [ ] The preview opens offline on a phone after one load
- [ ] Closing the PR tears the preview down; reopening it brings it back
- [ ] The PR shows "View deployment" linking to the preview, and the link goes inactive on close
- [ ] After merge, `iti.tamanu.app` serves the production deploy
- [ ] A fork PR builds in CI and deploys nothing
