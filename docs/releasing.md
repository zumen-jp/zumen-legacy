# Releasing

zumen is released under the [MIT license](../LICENSE). The repository is public; registry releases remain gated. This is the checklist for the first public
release and the routine for later ones.

A release is a tag `vX.Y.Z`. Two workflows react to a pushed tag and nothing else publishes: `python-wheels.yml` for the
Python package and `npm-release.yml` for the npm packages. The uploads to PyPI and to npm happen only when the repository
variable `PUBLISH_TO_PYPI` or `PUBLISH_TO_NPM` is `true`, and the packages still carry the two guards that stop them
(step 9 below), so a tag cannot publish to either until then. (`python-wheels.yml` also attaches the wheels to a GitHub
Release for every tag.)

## Before the first package release

1. **Rights.** Done. The copyright holder is the author of the repository, Yusuke Hayashi.
2. **License.** Done. `LICENSE` is MIT, and the root `package.json`, every `packages/*/package.json`, `Cargo.toml` and
   `bindings/python/pyproject.toml` say so. The wheels carry the text of `LICENSE` and of the license of the bundled
   font. The licenses of the Rust crates that are compiled into the wheels and into the WebAssembly reader of
   `@zumenjs/reader` are in `THIRD_PARTY_LICENSES.txt` next to them (`pnpm run notices` writes them from `Cargo.lock`, and
   `pnpm run check` fails when they are out of date).
3. **Code of conduct.** [CODE_OF_CONDUCT.md](../CODE_OF_CONDUCT.md) identifies the maintainer who handles reports.
4. **Source maps.** Done. The packages ship `src` next to `dist`, so the source maps and the declaration maps point at
   files that are in the package (the release check verifies every listed source directory). The first job of `npm-release.yml`
   fails when an entry of `files` is missing.
5. **GitHub.** The repository is public. Before registry publishing, enable private vulnerability reporting and protect `main` with the CI checks;
   choose "GitHub Actions" as the Pages source, and set the repository variable `DEPLOY_DEMO` to `true`. Create the
   environments `pypi` and `npm` and add yourself as a required reviewer of each, so that a tag alone does not publish.
   Set the repository variables `PUBLISH_TO_PYPI` and `PUBLISH_TO_NPM` to `true` when a tag should publish.
6. **npm.** Create the `zumenjs` organization with two-factor authentication required for all members. For the first
   release by hand and the trusted publishers, see [First release](#first-release).
7. **PyPI.** Sign in with two-factor authentication and add a pending trusted publisher: project `zumen`, owner
   `zumenjs`, repository `zumen`, workflow `python-wheels.yml`, environment `pypi`. A pending publisher lets the first
   upload from the workflow create the project, so the first PyPI release needs no hand work.
8. **Third-party rights.** Keep third-party license texts with distributed binaries. ST-Bridge is outside this repository ([decision 0013](decisions/0013-st-bridge-out-of-zumen.md)).
9. **Lift the two guards.** In the commit that you tag first, remove `"private": true` from `packages/core`, `packages/viewer`, `packages/reader` and `packages/react` only (the workspace root and TypeScript reference readers stay private) and replace the classifier `Private :: Do Not Upload` in
   `bindings/python/pyproject.toml` with `License :: OSI Approved :: MIT License`. They are what stops `pnpm publish`
   and PyPI today. Do this when you have read the audit of the repository and decided to publish.

Steps 1 to 4 and 9 are changes to the repository; 5 to 8 cover account settings and third-party rights.

## What the workflows do

| Step | By | Needs |
| --- | --- | --- |
| Build the wheels and the sdist, install each wheel and run the Python tests, build a wheel from the sdist | `python-wheels.yml`, on every pull request and push to `main` | |
| Attach the wheels and the sdist to a GitHub Release | `python-wheels.yml`, on a tag | |
| Upload them to PyPI with attestations (PEP 740) | `python-wheels.yml`, on a tag | `PUBLISH_TO_PYPI`, the pending trusted publisher (step 7), the `pypi` environment, step 9 |
| Build the WebAssembly reader and the packages, check the tarballs | `npm-release.yml`, on a tag | `PUBLISH_TO_NPM` |
| Publish the packages to npm with provenance, in dependency order | `npm-release.yml`, on a tag | The packages on npm with a trusted publisher each (see [First release](#first-release)), the `npm` environment, step 9 |

`npm-release.yml` stops before it builds when a version differs from the tag, when the tag is not a commit of `main`, or
when the complete `ci.yml` workflow (including Linux and Windows) has not passed on that commit. Its first job has no permission to publish; the second job
publishes the tarballs that the first one checked and has `id-token: write` only. A package that npm already has is
skipped, so the job can run again after a failure half way. `python-wheels.yml` checks the tag against the version of
the files before it creates the release, and uploads to PyPI only after that.

## Versions

During 0.x all packages share one version number and are released together: the root `package.json`, every
`packages/*/package.json` and `version` in `[workspace.package]` of `Cargo.toml`, which the crates and the Python
package use. Both workflows stop when a version differs from the tag.

## First release

If npm does not allow adding a trusted publisher before a package exists (its documentation does not say), publish the
first release by hand; if it does, add the trusted publishers first (step 3) and skip steps 1 and 2.

1. Run `pnpm install --frozen-lockfile` and `pnpm run check` on a clean checkout of `main`, at the commit with step 9.
2. Publish in dependency order with two-factor authentication:
   `pnpm --filter @zumenjs/core publish`, then `@zumenjs/viewer` and `@zumenjs/reader`, then `@zumenjs/react`. `@zumenjs/reader` publishes the WebAssembly reader in
   `packages/reader/wasm`, which `pnpm run check` has just built from the same commit. pnpm replaces `workspace:*` with the
   version, and `publishConfig.exports` drops the development-only condition. Add `--no-provenance` to each command:
   provenance needs a CI that npm knows, not a laptop.
3. On npm, add a trusted publisher to each of the four public packages: GitHub Actions, organization `zumenjs`, repository
   `zumen`, workflow `npm-release.yml`, environment `npm`. npm expires a new trusted publisher that has not completed a
   publish within two days, so tag the next release right after this.
4. From then on, `npm-release.yml` publishes with provenance (it is automatic for a public repository and public
   packages).

## Later releases

1. Update `CHANGELOG.md` and the version of every package (see [Versions](#versions)) in one commit, and let CI pass on
   `main`.
2. Tag the commit as `vX.Y.Z` and push the tag. The workflows above do the rest; approve the `pypi` and `npm`
   environments if you required a reviewer. To publish nothing for a tag, leave the variables unset.

If a tag was wrong, delete it (`git push origin :refs/tags/vX.Y.Z`), fix the commit and tag again. A version that PyPI
or npm has accepted cannot be uploaded again, so correct it with the next version.

## Python wheels

`.github/workflows/python-wheels.yml` builds the Python package of `bindings/python`, so that an application installs
it without a Rust toolchain, and uploads it to PyPI when the variable `PUBLISH_TO_PYPI` is `true`.

- **What it builds.** One abi3 wheel per platform, which serves every Python from 3.12: `manylinux_2_28` for x86_64
  and aarch64 (glibc 2.28 or newer, as in Debian 10, Ubuntu 20.04 and RHEL 8) and macOS 11 or newer for arm64, and the
  sdist. There are no wheels for Windows, Intel macOS or musl (Alpine); the sdist builds there with a Rust toolchain.
  maturin is the version of `bindings/python/uv.lock`, and Rust the toolchain of `rust-toolchain.toml`.
- **Pull requests and `main`.** The wheels and the sdist are artifacts of the workflow run (kept for 14 days). Each
  wheel is installed on a runner of its platform, on Python 3.12 and 3.14, and the tests of `bindings/python/tests` run
  against it, not against a development build. The sdist must build a wheel by itself.
  `bindings/python/scripts/check_dist.py` checks every file: the extension module holds the bundled font (BIZ UDGothic),
  and `dist-info/licenses` holds the text of the license of zumen, the text of the license of the font (SIL OFL 1.1) and
  the licenses of the linked Rust crates, as the licenses ask of every copy. `bindings/python/LICENSE` is a copy of the
  `LICENSE` of the repository root, and `bindings/python/licenses/` has a copy of the `OFL.txt` next to the font and the
  generated `THIRD_PARTY_LICENSES.txt`, because `license-files` cannot reach outside the project folder; the check fails
  when they differ.
- **Tags.** Pushing `vX.Y.Z` runs the same jobs, then attaches the wheels and the sdist to a GitHub Release of the tag,
  and with `PUBLISH_TO_PYPI` uploads them to PyPI. The files carry the `version` of `[workspace.package]` in
  `Cargo.toml`; the release job stops before it creates anything when the tag is another version.
- **Installing a release.** Download a release with `gh` or from its public GitHub release page. Install the
  file of the platform:

  ```bash
  gh release download v0.1.0 --repo zumenjs/zumen --pattern '*manylinux_2_28_x86_64.whl' --dir wheels
  pip install wheels/zumen-0.1.0-cp312-abi3-manylinux_2_28_x86_64.whl
  ```

  Install the file itself, or use `--no-index --find-links wheels`, as long as the name `zumen` is not on PyPI: an
  installer that looks the name up there would take whatever anyone registers under it. Once the package is on PyPI,
  `pip install zumen` works.
