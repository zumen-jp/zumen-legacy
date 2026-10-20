# Releasing

zumen is not public yet. This is the checklist for the first public release and the routine for later ones.

## Before going public

1. **Rights.** Confirm who owns the code and decide the copyright holder shown in the license.
2. **License.** Add `LICENSE` (MIT is the plan), set `"license": "MIT"` in every `packages/*/package.json`,
   and remove `"private": true` from those packages. The workspace root stays private. The Python package needs
   the same ([Python wheels](#python-wheels)).
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
2. Tag the commit as `vX.Y.Z` and push the tag. The release workflow publishes the packages, and the Python workflow
   attaches the wheels to a GitHub Release ([Python wheels](#python-wheels)).

## Python wheels

`.github/workflows/python-wheels.yml` builds the Python package of `bindings/python`, so that an application installs
it without a Rust toolchain. It does not publish to PyPI: the package has the classifier `Private :: Do Not Upload`,
which PyPI refuses, until the license is decided (steps 1 and 2 of "Before going public").

- **What it builds.** One abi3 wheel per platform, which serves every Python from 3.12: `manylinux_2_28` for x86_64
  and aarch64 (glibc 2.28 or newer, as in Debian 10, Ubuntu 20.04 and RHEL 8) and macOS 11 or newer for arm64, and the
  sdist. There are no wheels for Windows, Intel macOS or musl (Alpine); the sdist builds there with a Rust toolchain.
  maturin is the version of `bindings/python/uv.lock`, and Rust the toolchain of `rust-toolchain.toml`.
- **Pull requests and `main`.** The wheels and the sdist are artifacts of the workflow run (kept for 14 days). Each
  wheel is installed on a runner of its platform, on Python 3.12 and 3.14, and the tests of `bindings/python/tests` run
  against it, not against a development build. The sdist must build a wheel by itself.
  `bindings/python/scripts/check_dist.py` checks every file: the extension module holds the bundled font (BIZ UDGothic)
  and `dist-info/licenses` holds the text of its license (SIL OFL 1.1, a link in `bindings/python/licenses/` to the
  file next to the font), as the license asks of every copy of the font.
- **Tags.** Pushing `vX.Y.Z` runs the same jobs, then attaches the wheels and the sdist to a GitHub Release of the tag.
  The files carry the `version` of `[workspace.package]` in `Cargo.toml`; the release job stops before it creates
  anything when the tag is another version, so change the version, delete the tag and tag again.
- **Installing a release.** The repository is private, so a release is downloaded with `gh` or a token. Install the file
  of the platform:

  ```bash
  gh release download v0.1.0 --repo zumenjs/zumen --pattern '*manylinux_2_28_x86_64.whl' --dir wheels
  pip install wheels/zumen-0.1.0-cp312-abi3-manylinux_2_28_x86_64.whl
  ```

  Install the file itself, or use `--no-index --find-links wheels`. The name `zumen` is not registered on PyPI, so an
  installer that looks it up there would take whatever anyone registers under it.
- **When the license is decided.** In `bindings/python/pyproject.toml`, add `LICENSE` to `license-files` next to
  `licenses/*` (the wheel holds only the license of the font until then), set `license`, and remove the classifier.
  Then add a job to the workflow that uploads the wheels and the sdist to PyPI as a trusted publisher
  (`id-token: write`), like the npm release workflow.
