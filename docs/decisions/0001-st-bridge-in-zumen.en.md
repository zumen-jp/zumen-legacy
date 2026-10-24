# 0001: Keep ST-Bridge structural models in zumen as a TypeScript package

> English reference preserved when Japanese became the primary documentation. See [0001-st-bridge-in-zumen.md](0001-st-bridge-in-zumen.md) for the Japanese guide and current policy.

| Item | Value |
| --- | --- |
| Date | 2026-10-19 |
| Status | Superseded by [0013](0013-st-bridge-out-of-zumen.md) |

## Context

An ST-Bridge viewer was written next to zumen, with the same layering: it reads `.stb` files of versions 2.0 and 2.1,
builds a 3D structural model and shows it with three.js and React. The first plan was to move it to a separate
repository of the organization and to port its reader to Rust for Python and WebAssembly, as zumen does
for drawings.

Checked on 2026-10-19:

- The viewer is about 5,800 lines of TypeScript with one test file. zumen has about 14,300 lines of TypeScript and
  3,600 lines of Rust.
- zumen's format packages turn every drawing into a 2D `Scene` in paper millimeters. A structural model is 3D (global
  millimeters, Z up) and does not fit `readDrawing`.
- No browser library for ST-Bridge was found on npm. [stb-kit](https://github.com/taisei-oss/stb-kit) by Taisei
  Corporation (MPL-2.0, Python with a Rust extension, 0.1.0b4 on 2026-10-14) reads, validates and converts ST-Bridge
  (to IFC and PLY) and has a simple 3D view built with three-d and egui that can also run as WebAssembly. Its roadmap
  does not plan a full viewer, and a TypeScript version is only a future consideration.

## Options

- **A separate repository with a Rust core**: zumen stays about 2D drawings. Stars, issues, CI, release steps and
  documentation are split, the planned plan and elevation drawings would depend on `zumen-core` across repositories,
  and a Python reader would overlap stb-kit.
- **A package in zumen, kept in TypeScript**: stars and issues stay in one place, plan and elevation drawings can use
  `Scene` inside the workspace, and the planned MCP server can offer structural tools next to the drawing tools.
  zumen's scope grows from 2D drawings to structural models, with a second, 3D line of packages.
- **Not publishing it**: no work, but the only ST-Bridge viewer that web applications can embed stays unused.

## Decision

Add the viewer to zumen as `@zumenjs/stb`, in TypeScript, with the entry points `.` (parsing), `./three` (the three.js
view) and `./react` (the React component). Do not port it to Rust. When Python needs ST-Bridge, first check whether
stb-kit is enough.

## Consequences

- `@zumenjs/stb` depends on no other zumen package. Plan and elevation drawings, when they are added, convert the model
  to a `Scene` and make it depend on `@zumenjs/core` only.
- three.js is an optional peer dependency of `@zumenjs/stb` only. The playground loads the 3D view when a structural
  model is opened, so drawings do not load three.js.
- ST-Bridge belongs to buildingSMART Japan. stb-kit states that it quotes the specification under a license agreement
  with buildingSMART Japan. `@zumenjs/stb` cites section numbers, does not copy the text of the specification and does
  not ship the specification or its XML Schema. The terms are not stated on the buildingSMART Japan web page, so confirm
  them with buildingSMART Japan before publishing. stb-kit also states that it may contain GUI design patents, including
  pending ones, held by Taisei Corporation; their content has not been checked.
- Revisit this decision if stb-kit publishes a viewer that web applications can embed, as a JavaScript or WebAssembly
  package.

## References

- [How ST-Bridge is handled](0013-st-bridge-out-of-zumen.md) (Japanese) and [the architecture notes](../architecture.md)
- [buildingSMART Japan, structural design subcommittee](https://www.building-smart.or.jp/meeting/buildall/structural-design/)
- stb-kit's [README](https://github.com/taisei-oss/stb-kit/blob/main/README.md),
  [ROADMAP](https://github.com/taisei-oss/stb-kit/blob/main/ROADMAP.md),
  [ARCHITECTURE](https://github.com/taisei-oss/stb-kit/blob/main/ARCHITECTURE.md) and
  [IP](https://github.com/taisei-oss/stb-kit/blob/main/IP.md), read on 2026-10-19
