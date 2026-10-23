# 0013: Keep zumen free of formats that need permission and of non-permissive dependencies, and move ST-Bridge out

> English reference preserved when Japanese became the primary documentation. See [0013-st-bridge-out-of-zumen.md](0013-st-bridge-out-of-zumen.md) for the Japanese guide and current policy.

| Item | Value |
| --- | --- |
| Date | 2026-10-20 |
| Status | Accepted |

## Context

- zumen is about to go public under MIT. [Decision 0001](0001-st-bridge-in-zumen.md) kept an ST-Bridge reader and 3D
  view in zumen as `@zumenjs/stb`, and left the terms for publishing an implementation of the ST-Bridge specification
  to be confirmed with buildingSMART Japan before the release ([releasing.md](../releasing.md)).
- buildingSMART Japan holds the rights to ST-Bridge. Its web page and the specification do not state terms for
  implementations. STB-KIT (Taisei, MPL-2.0) says that it publishes its implementation under a license agreement with
  buildingSMART Japan, so publishing an implementation needs such an agreement, which zumen does not have.
- Reading and writing every file that an application handles was planned next: every version of the drawing formats,
  ST-Bridge in Rust with writing, and IFC through ifc-lite, which is MPL-2.0.

## Options

1. **Apply for a license and keep ST-Bridge in zumen.** The release waits for the answer, and zumen's terms would
   depend on an agreement with a third party.
2. **Keep the code in zumen and only hold back the package.** Making the repository public publishes the
   implementation anyway.
3. **Move ST-Bridge to a private repository**, and keep zumen to what can be published without anyone's permission
   and with permissive dependencies only.

## Decision

Option 3.

- `@zumenjs/stb` (the reader, `StructureModel`, the three.js view and the React component), its notes, its
  verification record and the playground sample move to a private repository, from commit `1f3ccdf`.
  The open pull request #24 (the `StructureViewer` engine) moves there with its whole diff.
  [Decision 0001](0001-st-bridge-in-zumen.md) is superseded.
- zumen adds no format whose publication needs a third party's permission, and no runtime dependency whose license is
  not permissive (MIT, Apache-2.0, BSD, ISC, Zlib and the like): no MPL, LGPL or GPL. IFC through ifc-lite (MPL-2.0)
  is not added either.
- Either may come back later as our own implementation, or with a permission, through a new decision.
- The work goes on with the drawing formats: every version of SXF, JWW, DXF and DWG, read and written.

## Consequences

- zumen can be published without waiting for buildingSMART Japan.
- The playground opens drawings only. Plan and elevation drawings of structural models, planned in decision 0001,
  are not part of zumen.
- `AGENTS.md` and the release checklist name the rule, so that a contribution does not bring such a format or
  dependency back unnoticed.
- That repository builds and tests on its own (pnpm, TypeScript, Vitest, Biome). Its browser tests are not set up there
  yet.

## References

- [STB-KIT](https://github.com/taisei-oss/stb-kit) README, section 3 (the license agreement with buildingSMART Japan),
  read on 2026-10-20
- [ifc-lite](https://github.com/LTplus-AG/ifc-lite) (MPL-2.0), read on 2026-10-20
