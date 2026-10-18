# Rules for coding agents

- The entry point for verification is `pnpm run check` (Biome, the type checker, unit tests, `pnpm audit`,
  the builds of every package and the playground, then `check:rust` and `check:python`). Check visible changes in the `pnpm dev` playground
  with real drawings.
- Follow the public specification of each format and cite the section in a comment.
  Write down inferred behavior in `docs/formats/`.
- Report anything not drawn, or drawn differently, through `notices` with its kind and count. Never drop it silently.
- Never commit drawings (received files or public samples). Open them from the `ZUMEN_SAMPLES` folder.
  Tests build the smallest drawing that pins a rule, in the format package's `tests/`.
- Before adding a runtime dependency, check its license (no GPL or AGPL) and that it works in browsers,
  and record the reason in `docs/architecture.md`.
- Dependency directions: `@zumenjs/react` → `@zumenjs/core`, and each format package → `@zumenjs/core` only.
  Keep format knowledge out of `@zumenjs/core`, and keep format packages from importing each other.
  The exception is the DWG reader, which reads into the AutoCAD drawing model of `@zumenjs/dxf` and uses `dxfToScene`.
- Parsing (`parse*`, `read*` and everything below them) must not depend on the DOM, React or the renderer, and uses
  only the types and `NoticeList` from `@zumenjs/core`. The `*ToScene` conversions use the geometry and text helpers.
- Interaction (zoom, selection, measurement) belongs in `DrawingViewer`. `@zumenjs/react` only shows the state and
  calls the viewer's methods.
- Never put client names, client drawings or private repository details in this repository.
- The Rust readers (`crates/`) are ports of the TypeScript readers and must return the same scene. Keep the structure
  and order of operations, format user-visible numbers with `zumen_core::js`, and after changing a TypeScript reader run
  `pnpm run golden` and fix the Rust side until `cargo test -p zumen` passes. Never change a golden file by hand.
- Rust runtime dependencies follow the same rules as npm ones (no GPL or AGPL, works in WebAssembly) and are recorded
  with the reason in `docs/architecture.md`.
