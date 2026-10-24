# Contributing to zumen

> English reference preserved when Japanese became the primary documentation. See [CONTRIBUTING.md](CONTRIBUTING.md) for the Japanese guide and current policy.

Thank you for helping. Please open an issue to discuss larger changes before you start.
Everyone taking part follows the [code of conduct](CODE_OF_CONDUCT.md).

## Never share confidential drawings

Drawings often belong to clients. Do not attach them to issues or pull requests, and do not commit them.
If a problem needs a drawing, make the smallest drawing that reproduces it, or describe the file:
format, version, authoring software, and the notices the viewer shows.

## Setup

- Node.js from `.node-version` (24) and pnpm from `packageManager` in `package.json` (enable it with `corepack enable`).
- Rust from `rust-toolchain.toml` (installed by rustup on first use) and [uv](https://docs.astral.sh/uv/) for the Python bindings.
- wasm-bindgen-cli of the version of `wasm-bindgen` in `Cargo.lock`, which builds the WebAssembly reader of
  `@zumenjs/reader` (`node scripts/install-wasm-bindgen.mjs` installs the locked version; `node scripts/wasm.mjs`
  prints the command for the current version when it differs).
- Chromium, Firefox and WebKit for the tests of the engine, the React hooks and the reader's worker, which run in a
  browser (`*.browser.test.*`): `pnpm exec playwright install --only-shell chromium firefox webkit` installs the
  browsers of the Playwright version in `package.json` (the headless shell for Chromium). `ZUMEN_BROWSERS` runs them
  in fewer browsers, for example `ZUMEN_BROWSERS=chromium pnpm test`.
- Install and run every check:

  ```bash
  pnpm install --frozen-lockfile
  pnpm exec playwright install --only-shell chromium firefox webkit
  pnpm run check
  ```

- Open the playground with `pnpm dev`, which first builds the WebAssembly reader into `packages/reader/wasm`
  (`scripts/wasm.mjs`; `pnpm test`, the type checker and the builds need it too). It listens on `127.0.0.1` at the
  `playground` port in
  [config/local-ports.json](config/local-ports.json) (24100); set `PORT` to override it.
  To open drawings that are not in Git, set `ZUMEN_SAMPLES` to their folder and visit `/?sample=<file name>`.
  Drawings are shown by the 2D sample (`playground/drawing-sample.tsx`), which uses only the public API of `@zumenjs/react` and
  `@zumenjs/viewer`; the pages of PDF drawings are drawn by `playground/pdf-sample.ts` with `@zumenjs/pdf` and pdf.js,
  whose CMaps and image decoders `vite.config.ts` serves under `pdfjs/`.

### Windows

Use native PowerShell 7 and the MSVC Rust toolchain, with the C++ build tools and Windows SDK installed.
Run the checks in Developer PowerShell so native dependencies can find MSVC. Keep Node, Rust and Python on the
Windows side. The Node and Python versions are pinned in `.node-version` and `bindings/python/.python-version`.
If Python is already managed locally, set `$env:UV_PYTHON_DOWNLOADS = "never"` to use that installation.
As in CI, the browser tests run in Chromium only on Windows (`ZUMEN_BROWSERS`).

```powershell
pnpm install --frozen-lockfile
node scripts/install-wasm-bindgen.mjs
pnpm exec playwright install --only-shell chromium
$env:ZUMEN_BROWSERS = "chromium"
pnpm run check
$env:ZUMEN_SAMPLES = "D:\zumen-samples"
pnpm dev
```

`pnpm run golden` works in PowerShell as well as POSIX shells. For commands which explicitly require an environment
variable, use `$env:ZUMEN_RECORD_IDS = "1"` (or `$env:UPDATE_GOLDEN = "1"`) before the command, and
`Remove-Item Env:ZUMEN_RECORD_IDS` (or `Env:UPDATE_GOLDEN`) afterwards.
`pnpm run parity` discovers the Cargo target directory and `.exe` suffix; `ZUMEN_BINARY` can select another CLI.
A missing executable, failed parser, or input set with no comparable drawings fails the check.

CI runs the full check on Linux and Windows. `check:scripts` exercises the native CLI with spaces and Japanese
characters in its path. Python tests cover directory junctions without requiring Developer Mode; the separate
symlink test skips only when Windows denies the symlink privilege. No system settings need to be changed.
CI also builds a release wheel and installs it in a fresh virtual environment outside the checkout:

```powershell
uv run --project bindings/python --frozen maturin build --release -m bindings/python/Cargo.toml --out target/wheels
node scripts/check-wheel.mjs
```

The wheel check expects exactly one wheel in `target/wheels`, and checks importing the installed extension, a
Japanese file path, reading and PNG rendering. See [verification records](docs/verification.md#windows-native-verification)
for the separate Jw_cad and font checks.

## How the code is organized

Read [docs/architecture.md](docs/architecture.md) first. In short: format packages and `@zumenjs/viewer` depend only
on `@zumenjs/core`, parsing and `@zumenjs/core` never touch the DOM, interaction belongs in the engine
(`DrawingViewer` of `@zumenjs/viewer`), `@zumenjs/react` has hooks only, and the packages ship no CSS and no text for
people: the interface is the application's, and the playground has a sample.

- Follow the public specification of each format and cite the section in a comment.
  Write down behavior you had to infer in `docs/formats/`.
- Report anything that is not drawn, or drawn differently, through `notices`. Do not drop content silently.
- Do not add runtime dependencies without discussing it first. GPL and AGPL dependencies are not accepted. When
  `Cargo.lock` changes, run `pnpm run notices` to refresh the license texts of the Rust crates that end up in the Python
  wheels and in `@zumenjs/dwg` (`THIRD_PARTY_LICENSES.txt`, checked by `pnpm run check`).

## Tests

Keep tests few and meaningful. Add a test when a mistake would go unnoticed and produce wrong output:
encodings, coordinate systems, scales, nesting, and regressions of serious bugs. Do not add tests that restate the
implementation or check UI text. Build test drawings inside the test (see `packages/jww/tests`).
Tests named `*.browser.test.ts` or `*.browser.test.tsx` run in Chromium, Firefox and WebKit (the `browser` project of
`vitest.config.ts`; CI runs Firefox and WebKit on Ubuntu only): the engine and the React hooks are tested there with
synthetic pointer, wheel and key events and the pixels of the canvas, and the reader in its Web Worker.

The Rust readers must return the same scenes as the TypeScript readers. The synthetic drawings that the TypeScript
tests build are also written to `tests/fixtures` with the scenes they produce; after changing a TypeScript reader, run
`pnpm run golden` and make the Rust reader match (`cargo test -p zumen`). DWG has no TypeScript reader: its drawings
are written by the Rust tests (`UPDATE_GOLDEN=1 cargo test -p zumen-dwg`) and their scenes by `pnpm run golden` through
the WebAssembly reader. To compare both on your own drawings,
build them (`pnpm run build`, `cargo build --release -p zumen-cli`) and run `pnpm run parity <folder>`.
`pnpm run fuzz [cases] [seed] [formats…]` reads randomly damaged copies of the fixtures with both (after `pnpm run build` and
`pnpm run wasm`) and reports any input where the scenes or the error messages differ. With `ZUMEN_FUZZ_TRACE=1` it writes
each input to the temporary folder before reading it, so an input that hangs or crashes a reader can be found.
A reader must finish on any input: bound recursion and loops by limits that both implementations share.

## Style

- Format with Biome (`pnpm run format`), rustfmt (`cargo fmt --all`) and Ruff. Lines are up to 120 characters.
- Many existing comments are in Japanese because they quote Japanese specifications. English comments are welcome.
- Commit messages follow Conventional Commits, for example `feat(sxf): …` or `fix(core): …`.

## Pull requests

Keep one purpose per pull request, and include the tests and documentation it needs.
`pnpm run check` must pass, and visible changes should be checked in the playground.

## License of contributions

zumen is released under the [MIT license](LICENSE). Contributions are accepted under it: by opening a pull request you
agree that your contribution is licensed under the same terms.
