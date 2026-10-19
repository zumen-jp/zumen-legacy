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
- `@zumenjs/stb` (ST-Bridge structural models) is a separate 3D line that imports no other package. Its parsing
  (`src/`, `src/xml/`, `src/stb/`) must not depend on the DOM, three.js or React; three.js belongs in `src/three/` and
  React in `src/react/`. Read ST-Bridge 2.0 and 2.1 into the same `StructureModel`, never one of them only, and never
  copy the text of the specification.
- Never put client names, client drawings or private repository details in this repository.
- The Rust readers (`crates/`) are ports of the TypeScript readers and must return the same scene. Keep the structure
  and order of operations, format user-visible numbers and sort with `zumen_core::js` (`js::sort_by` orders like V8),
  bound recursion and loops on malformed input with limits that both readers share, and after changing a TypeScript
  reader run `pnpm run golden` and fix the Rust side until `cargo test -p zumen` passes. Never change a golden file by hand.
- Rust runtime dependencies follow the same rules as npm ones (no GPL or AGPL, works in WebAssembly) and are recorded
  with the reason in `docs/architecture.md`.
- Work on a branch cut from the latest `origin/main` and open a pull request for every change; do not push to `main`.
  Run `pnpm run check` before pushing, then squash-merge once CI passes, unless the user asked to stop at review.
  Delete the branch after merging. Commit messages and pull request titles follow Conventional Commits.
- Record design decisions (a choice between alternatives and its reasons) in `docs/decisions/`, in English,
  as its README describes.
- Development ports come from `config/local-ports.json` (the playground uses 24100). Repositories of the zumenjs
  organization use 24100–24199, so they can run next to other projects on the same machine.
- Keep drawings for manual checks in a folder outside the repository and pass it as `ZUMEN_SAMPLES`. Keep
  machine-specific files such as `.claude/launch.json` out of Git by listing them in `.git/info/exclude`.
