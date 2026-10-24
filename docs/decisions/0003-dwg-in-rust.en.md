# 0003: Read DWG in Rust only, with our own decoder, into the AutoCAD drawing model of zumen-dxf

> English reference preserved when Japanese became the primary documentation. See [0003-dwg-in-rust.md](0003-dwg-in-rust.md) for the Japanese guide and current policy.

| Item | Value |
| --- | --- |
| Date | 2026-10-19 |
| Status | Accepted |

## Context

The fourth item of the roadmap is DWG, read into the AutoCAD drawing model that DXF already uses (`DxfDocument`) and
drawn by the same conversion (`dxfToScene` in TypeScript, `dxf_to_scene` in Rust). The architecture notes planned a
TypeScript package `@zumenjs/dwg` as the one exception to the rule that format packages do not import each other, and
they also state that the Rust code will become the single implementation of the readers: the browser packages switch
to the WebAssembly build format by format, once the Rust reader returns the same scenes. Until now every reader was
written in TypeScript first and ported to Rust, value for value.

DWG differs from the other formats in ways that matter for this choice:

- It is read at the bit level: compressed numbers, handles of up to 8 bytes, three container layouts (R13 to R2000;
  R2004 and R2010 to R2018 with encrypted headers and compressed pages; R2007 with Reed-Solomon coding and another
  compression), and from R2007 on, UTF-16 strings and handles in separate streams inside each object.
- Autodesk publishes no specification. The Open Design Alliance publishes the reverse-engineered
  [Open Design Specification for .dwg files](https://opendesign.com/files/guestdownloads/OpenDesign_Specification_for_.dwg_files.pdf)
  (version 5.4.1, "use these materials at your own risk"), which covers R13 to R2018 (AC1012 to AC1032).
- There is no TypeScript DWG reader to keep equal, and the drawing side already exists in both languages.

Components checked on 2026-10-19:

| Component | License | Notes |
| --- | --- | --- |
| [LibreDWG](https://www.gnu.org/software/libredwg/), [`libredwg-sys`](https://crates.io/crates/libredwg-sys) | GPL-3.0-or-later | Excluded by our rules (no GPL or AGPL) |
| [ezdwg](https://github.com/monozukuri-ai/ezdwg) 0.16.3 (commit d5e4336) | MIT | Python with a Rust core (`ezdwg-core`, depends only on `encoding_rs`). Reads AC1012 to AC1032. `ezdwg-core` is not on crates.io, and the reading of tables, blocks, layers, line types, dimensions and layouts lives in its PyO3 crate, which a browser build cannot use. `DxfDocument` already uses its attribute names |
| [acadrust](https://crates.io/crates/acadrust) 0.6.5 | MPL-2.0 | Reads and writes R13 to R2018. File-level copyleft, a large dependency that also writes, and the license of zumen is not decided yet |
| [ACadSharp](https://github.com/DomCR/ACadSharp) | MIT | C#, not usable from Rust or browsers; a reference for what the specification leaves out |

## Options

- **TypeScript first, then a Rust port**, as for JWW, SXF and DXF: `packages/dwg` reads DWG into `DxfDocument` and uses
  `dxfToScene`, and `zumen-dwg` repeats it value for value. The browser gets DWG without WebAssembly, but a large binary
  decoder is written twice and kept equal bit for bit, 64-bit handles need `BigInt` in TypeScript, and the TypeScript
  code is thrown away when the browser switches to WebAssembly.
- **Rust only, on ezdwg's core** (a git dependency or vendored MIT code): little decoding to write, but the high-level
  reading would still have to be rewritten outside its PyO3 crate, a git dependency cannot be published to crates.io,
  its 0.x API changes weekly, and its loops, limits and failures would not follow the rules that our readers share
  (bounded work on malformed input, everything left out reported as notices).
- **Rust only, on acadrust**: a complete reader, but a copyleft dependency whose own object model would be converted
  to ours, before the license of zumen is chosen.
- **Rust only, with our own decoder** that follows the ODA specification and produces `DxfDocument` directly.

## Decision

Read DWG only in Rust, in a new crate `crates/zumen-dwg` with our own decoder:

- `read_dwg(bytes)` returns a `zumen_dxf::DxfDocument` (`format` `"dwg"`), and the `DWG` reader draws it with
  `zumen_dxf::dxf_to_scene`. `zumen-dwg` depends on `zumen-core` and `zumen-dxf`; this is the exception to the rule that
  format crates depend only on `zumen-core`, which the architecture notes planned for the DWG reader.
- The decoder follows the ODA specification 5.4.1 and cites its sections in comments. Where the specification is silent
  or does not match the files, the behavior is checked against the same drawing saved as DXF and against the MIT
  implementations above, and written down in `docs/formats/dwg.md`. LibreDWG's source is not read.
- The first version reads R2000 (AC1015) to R2018 (AC1032): the header variables, classes and object map, the tables
  (layers, line types, text styles, dimension styles, block records), block definitions, layouts, SORTENTSTABLE,
  MLINESTYLE and the entities that `DxfDocument` holds. What it does not read is reported in `notices` by kind and
  count, as in DXF.
- There is no TypeScript DWG reader. Browsers read DWG through the WebAssembly build (`bindings/wasm`), which already
  detects the format and returns scene JSON; `@zumenjs/dwg` will be a thin package around it.
- The reader follows the rules of the other Rust readers: no panics on any input, recursion and loops bounded by
  limits, and nothing left out silently.

## Consequences

- With no TypeScript reader to compare with, a DWG is checked against the same drawing saved as DXF: both become
  `DxfDocument`s and scenes that should agree. Unit tests build the smallest DWG that pins a rule, in Rust.
- `dxf_to_scene` and `dxfToScene` label a scene by `doc.format` (`"dwg"`, `DWG AC1032`), in both languages, so that
  they stay equal.
- Opening DWG in the playground and in `@zumenjs/react` needs the WebAssembly build in the browser, which is a
  separate change; until then DWG is available in Rust, the `zumen` command, Python and the WebAssembly package.
  The WebAssembly build grows by the size of the decoder.
- The decoder is ours to maintain. Versions after AC1032 and entities that the specification does not describe need
  work and samples. R13 and R14 (AC1012, AC1014) use the R2000 container and can be added later.
- `AGENTS.md` and the architecture notes now describe the DWG exception on the Rust side.

## References

- [Open Design Specification for .dwg files](https://opendesign.com/files/guestdownloads/OpenDesign_Specification_for_.dwg_files.pdf),
  version 5.4.1, read on 2026-10-19
- [How DXF is handled](../formats/dxf.md#the-autocad-drawing-model) and [the architecture notes](../architecture.md)
- ezdwg's [README](https://github.com/monozukuri-ai/ezdwg/blob/main/README.md) and
  [Cargo.toml](https://github.com/monozukuri-ai/ezdwg/blob/main/Cargo.toml), commit d5e4336, read on 2026-10-19

## Implementation update (2026-10-20)

The reader now includes R13 and R14 using the existing R2000 container. See
[R13 and R14](../formats/dwg.md#r13-and-r14) for the version-specific fields and verification limits.
