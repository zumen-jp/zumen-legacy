# Contributing to zumen

Thank you for helping. Please open an issue to discuss larger changes before you start.

## Never share confidential drawings

Drawings often belong to clients. Do not attach them to issues or pull requests, and do not commit them.
If a problem needs a drawing, make the smallest drawing that reproduces it, or describe the file:
format, version, authoring software, and the notices the viewer shows.

## Setup

- Node.js from `.node-version` (24) and pnpm from `packageManager` in `package.json` (enable it with `corepack enable`).
- Rust from `rust-toolchain.toml` (installed by rustup on first use) and [uv](https://docs.astral.sh/uv/) for the Python bindings.
- wasm-bindgen-cli of the version of `wasm-bindgen` in `Cargo.lock`, which builds the WebAssembly reader of
  `@zumenjs/dwg` (`cargo install wasm-bindgen-cli --locked --version 0.2.129`; `node scripts/wasm.mjs` prints the
  command for the current version when it differs).
- Chromium for the tests of the engine and the React hooks, which run in a browser (`*.browser.test.*`):
  `pnpm exec playwright install --only-shell chromium` installs the headless shell of the Playwright version in
  `package.json`.
- Install and run every check:

  ```bash
  pnpm install --frozen-lockfile
  pnpm exec playwright install --only-shell chromium
  pnpm run check
  ```

- Open the playground with `pnpm dev`, which first builds the WebAssembly reader into `packages/dwg/wasm`
  (`scripts/wasm.mjs`; `pnpm test`, the type checker and the builds need it too). It listens on `127.0.0.1` at the
  `playground` port in
  [config/local-ports.json](config/local-ports.json) (24100); set `PORT` to override it.
  To open drawings that are not in Git, set `ZUMEN_SAMPLES` to their folder and visit `/?sample=<file name>`.
  Drawings are shown by the 2D sample (`playground/drawing-sample.tsx`), which uses only the public API of `@zumenjs/react` and
  `@zumenjs/viewer`.

## How the code is organized

Read [docs/architecture.md](docs/architecture.md) first. In short: format packages and `@zumenjs/viewer` depend only
on `@zumenjs/core`, parsing and `@zumenjs/core` never touch the DOM, interaction belongs in the engine
(`DrawingViewer` of `@zumenjs/viewer`), `@zumenjs/react` has hooks only, and the packages ship no CSS and no text for
people: the interface is the application's, and the playground has a sample.

- Follow the public specification of each format and cite the section in a comment.
  Write down behavior you had to infer in `docs/formats/`.
- Report anything that is not drawn, or drawn differently, through `notices`. Do not drop content silently.
- Do not add runtime dependencies without discussing it first. GPL and AGPL dependencies are not accepted.

## Tests

Keep tests few and meaningful. Add a test when a mistake would go unnoticed and produce wrong output:
encodings, coordinate systems, scales, nesting, and regressions of serious bugs. Do not add tests that restate the
implementation or check UI text. Build test drawings inside the test (see `packages/jww/tests`).
Tests named `*.browser.test.ts` or `*.browser.test.tsx` run in Chromium (the `browser` project of `vitest.config.ts`):
the engine and the React hooks are tested there with synthetic pointer, wheel and key events and the pixels of the
canvas.

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

The project license will be announced before the first public release. Contributions will be accepted under it.
