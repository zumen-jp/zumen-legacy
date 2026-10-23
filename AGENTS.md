# Rules for coding agents

- The entry point for verification is `pnpm run check` (Biome, the WebAssembly reader (`pnpm run wasm`, which needs
  wasm-bindgen-cli of the `Cargo.lock` version), the type checker, unit tests, `pnpm audit`,
  the builds of every package and the playground, then `check:rust` and `check:python`). Check visible changes in the `pnpm dev` playground
  with real drawings. Tests named `*.browser.test.*` run in Chromium, Firefox and WebKit
  (`pnpm exec playwright install --only-shell chromium firefox webkit`; `ZUMEN_BROWSERS=chromium` runs one).
- Follow the public specification of each format and cite the section in a comment.
  Write down inferred behavior in `docs/formats/`.
- Report anything not drawn, or drawn differently, through `notices` with its kind and count. Never drop it silently.
- Never commit drawings (received files or public samples). Open them from the `ZUMEN_SAMPLES` folder.
  Tests build the smallest drawing that pins a rule, in the format package's `tests/`.
- Before adding a runtime dependency, check its license (no GPL or AGPL) and that it works in browsers,
  and record the reason in `docs/architecture.md`.
- Dependency directions: `@zumenjs/react` → `@zumenjs/viewer` → `@zumenjs/core`, and each format package →
  `@zumenjs/core` only. Keep format knowledge and the DOM out of `@zumenjs/core` (its build has no DOM types), and keep
  format packages from importing each other. What `zumen-core` also has goes to `@zumenjs/core`, and what only browsers
  need (drawing, input, hit testing) to `@zumenjs/viewer`.
  DWG has no TypeScript reader: `zumen-dwg` reads it in Rust into the AutoCAD drawing model of `zumen-dxf` and draws it
  with `dxf_to_scene`, the one format crate that depends on another format crate. `zumen-convert`, which maps SXF and
  JWW into that model for writing, is not a format crate and depends on several of them. Browsers read every format
  with `@zumenjs/reader`, which wraps the WebAssembly build of `bindings/wasm` (`packages/reader/wasm`, built by
  `scripts/wasm.mjs`, not in Git) and depends only on `@zumenjs/core`; its format detection (`detectDrawing`) repeats
  the rules of the Rust readers so that it runs before the module loads, and a test compares the two. The TypeScript
  readers (`@zumenjs/sxf`, `@zumenjs/jww`, `@zumenjs/dxf`) stay as the reference that writes the golden files and are
  not published.
- Parsing (`parse*`, `read*` and everything below them) must not depend on the DOM, React or the renderer, and uses
  only the types and `NoticeList` from `@zumenjs/core`. The `*ToScene` conversions use the geometry and text helpers.
- Interaction (the view, selection, measurement, input handlers and tools) belongs in the engine, `DrawingViewer` of
  `@zumenjs/viewer`. React packages have hooks only, which create an engine, read its state and events, and read files
  (`@zumenjs/react`).
- The packages ship no CSS and no text for people: labels, buttons, panels and their wording come from the
  application, and colors from CSS custom properties (`--zumen-*`). Complete interfaces are samples in the playground,
  written only against the public API; when a sample needs something the API lacks, add it to the API.
- Do not add formats or components whose publication needs a third party's permission (such as ST-Bridge, whose
  rights belong to buildingSMART Japan) or whose license is not permissive (such as MPL, LGPL or GPL), without a
  decision in `docs/decisions/` ([decision 0013](docs/decisions/0013-st-bridge-out-of-zumen.md)).
- Never put client names, client drawings or private repository details in this repository.
- The Rust readers (`crates/`) are ports of the TypeScript readers and must return the same scene (`zumen-dwg`, which has
  no TypeScript reader, is checked against the DXF that AutoCAD writes for the same drawing). Keep the structure
  and order of operations, format user-visible numbers and sort with `zumen_core::js` (`js::sort_by` orders like V8),
  bound recursion and loops on malformed input with limits that both readers share, add entities through
  `zumen_core::limits::admit` and call `limits::tick` in loops over input records (applications set their own limits
  on bytes, entities and time), and after changing a TypeScript
  reader run `pnpm run golden` and fix the Rust side until `cargo test -p zumen` passes. Never change a golden file by hand.
- Entity ids are a contract ([Entity ids](docs/architecture.md#entity-ids)): the same bytes give the same ids in
  every version. `crates/zumen/tests/ids.rs` compares them with `crates/zumen/tests/ids.json`, which
  `pnpm run golden` does not rewrite. When a fixture's bytes change or a fixture is added, check its ids and record
  them with `ZUMEN_RECORD_IDS=1 cargo test -p zumen --test ids`; a reader change that renames the entities of
  unchanged bytes needs a decision first.
- Rust runtime dependencies follow the same rules as npm ones (no GPL or AGPL, works in WebAssembly) and are recorded
  with the reason in `docs/architecture.md`. The license texts of the crates that are linked into the Python extension
  and into the WebAssembly reader of `@zumenjs/dwg` are in the two `THIRD_PARTY_LICENSES.txt` files. After a change of
  `Cargo.lock` that adds, removes or updates a crate, run `pnpm run notices` and commit them; `check:rust` fails if
  they are out of date.
- Work on a branch cut from the latest `origin/main` and open a pull request for every change; do not push to `main`.
  Run `pnpm run check` before pushing, then squash-merge once CI passes, unless the user asked to stop at review.
  Delete the branch after merging. Commit messages and pull request titles follow Conventional Commits.
- Record design decisions (a choice between alternatives and its reasons) in `docs/decisions/`, in English,
  as its README describes.
- Development ports come from `config/local-ports.json` (the playground uses 24100). Repositories of the zumenjs
  organization use 24100–24199, so they can run next to other projects on the same machine.
- Keep drawings for manual checks in a folder outside the repository and pass it as `ZUMEN_SAMPLES`. Keep
  machine-specific files such as `.claude/launch.json` out of Git by listing them in `.git/info/exclude`.
