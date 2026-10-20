# Releasing

zumen is not public yet. This is the checklist for the first public release and the routine for later ones.

## Before going public

1. **Rights.** Confirm who owns the code and decide the copyright holder shown in the license.
2. **License.** Add `LICENSE` (MIT is the plan), set `"license": "MIT"` in every `packages/*/package.json`,
   and remove `"private": true` from those packages. The workspace root stays private.
3. **Code of conduct.** Add one with a contact address, preferably in the organization's `.github` repository
   so that it applies to every repository.
4. **Source maps.** The packages ship `dist` only, but the source maps point at `src`. Either add `src` to `files`
   or stop emitting `declarationMap` and `sourceMap` in the published build.
5. **GitHub.** Make the repository public, enable private vulnerability reporting, protect `main` with the CI check,
   choose "GitHub Actions" as the Pages source, and set the repository variable `DEPLOY_DEMO` to `true`.
6. **npm.** Create the `zumenjs` organization with two-factor authentication required for all members.
7. **ST-Bridge.** Confirm with buildingSMART Japan the terms for publishing an implementation of the ST-Bridge
   specification, which its web page does not state ([decision 0001](decisions/0001-st-bridge-in-zumen.md)).

## Versions

During 0.x all packages share one version number and are released together.

## First release

If npm does not allow adding a trusted publisher before a package exists, publish the first release by hand.

1. Run `pnpm install --frozen-lockfile` and `pnpm run check` on a clean checkout of `main`.
2. Publish in dependency order with two-factor authentication:
   `pnpm --filter @zumenjs/core publish`, then `@zumenjs/viewer`, `@zumenjs/sxf`, `@zumenjs/jww`, `@zumenjs/dxf` and
   `@zumenjs/dwg`, then `@zumenjs/react` and `@zumenjs/stb`. `@zumenjs/dwg` publishes the WebAssembly reader in `packages/dwg/wasm`, which
   `pnpm run check` has just built from the same commit.
   pnpm replaces `workspace:*` with the version, and `publishConfig.exports` drops the development-only condition.
3. On npm, add a trusted publisher (GitHub Actions, repository `zumenjs/zumen`, the release workflow) to each package.
4. Add the release workflow, which publishes with provenance from GitHub Actions (`id-token: write`). Like CI, it needs
   the Rust toolchain and wasm-bindgen-cli to build the WebAssembly reader of `@zumenjs/dwg`.

## Later releases

1. Update `CHANGELOG.md` and the version of every package in one commit.
2. Tag the commit as `vX.Y.Z` and push the tag. The release workflow publishes the packages.
