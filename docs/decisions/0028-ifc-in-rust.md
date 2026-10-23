# 0028: Read IFC building models with our own reader in Rust, on the ISO 10303-21 syntax shared with SXF, and keep ST-Bridge out until buildingSMART Japan agrees

| Item | Value |
| --- | --- |
| Date | 2026-10-23 |
| Status | Proposed |

## Context

- [Decision 0013](0013-st-bridge-out-of-zumen.md) kept ST-Bridge outside zumen and did not add IFC
  through ifc-lite (MPL-2.0). Either "may come back later as our own implementation, or with a permission, through a new
  decision". [#109](https://github.com/zumenjs/zumen/issues/109) asks for that decision.
- Building confirmation in Japan has started to take BIM:
  - **BIM図面審査**, since April 2026: the applicant submits PDF drawings written from the BIM model under the
    input-output standard (入出力基準), with the IFC data, to the CDE for building confirmation. The reviewer uses the IFC
    only to understand the shape; it is not reviewed.
  - **BIMデータ審査**, from spring 2029 (MLIT's basic concept of 2026-09-18): the IFC data itself is reviewed on the CDE,
    shown for each review item (ビュー審査) and through results that programs compute from it (デジタル審査, with AI
    assistance). An IFC data creation standard (IFCデータ作成基準) will set the least shape and the properties that each
    review item needs; some items may need a regulation model (法規モデル) of the site's limits, such as setback planes;
    the IFC is to be kept and readable for a long time. MLIT hopes that software vendors develop IFC export that meets
    the standard and functions that verify the legal conformity of BIM models.
  - Its schedule publishes the review method of each item and the IFC specification around fiscal 2027, a guideline
    draft and a demo of the CDE after them, and the CDE in fiscal 2028. The items considered first include floor plans
    with the use and floor area of each room, daylight and ventilation, smoke exhaust and emergency lighting, fire
    compartments, the floor area ratio, the building coverage and height limits.
- The BIM programs used in Japan (Revit, Archicad, GLOOBE, Vectorworks and others) have no public specification of
  their native files ([#119](https://github.com/zumenjs/zumen/issues/119)). The open format that they all export is IFC.
- Components checked on 2026-10-23 (npm and crates.io):

| Component | License | Notes |
| --- | --- | --- |
| web-ifc | MPL-2.0 | The IFC engine of That Open |
| `@thatopen/components`, `@thatopen/fragments` | MIT | web-ifc is a peer dependency |
| ifc-lite (`ifc-lite-core`, `@ifc-lite/parser`) | MPL-2.0 | Rust and WebAssembly, IFC2X3 to IFC4X3 and IFC5 |
| IfcOpenShell | LGPL-3.0-or-later | C++ and Python |
| xeokit-sdk | AGPL-3.0 | |
| openbimrs (`openbim-step`, `ifc-schema`, `ifc-geometry` …) | AGPL-3.0-or-later | Rust, first published in 2026-08 |
| ifc_rs | MIT | Rust, alpha, last published on 2024-12-04 |
| oxideav-ifc | MIT | Rust, three releases since 2026-06 |

- **Rights.** IFC4.3 ADD2 is ISO 16739-1:2024. buildingSMART International publishes the IFC documentation under
  CC BY-ND 4.0, states that prior written consent is required to use IFC or the documentation in situations that the
  license does not cover, and that structural changes to the specification or its semantic definitions are not allowed.
  Neither that statement nor buildingSMART's page on IFC says anything about software that reads or writes IFC files.
  buildingSMART's own IFC Validation Service is published under MIT and runs on IfcOpenShell, and IFC readers are
  developed in the open by many parties, such as those above. ST-Bridge differs: STB-KIT, its published implementation,
  states a license agreement with buildingSMART Japan.
- IFC files (IFC-SPF) are ISO 10303-21, the syntax of SXF P21. `crates/zumen-sxf/src/p21_reader.rs` is a general lexer
  and parser of the DATA section, and `p21_edit.rs` edits P21 in place. Today `zumen::detect` takes every ISO 10303-21
  file that is not SFC for SXF P21, so an IFC file reads as an empty drawing with the `no-sheet` notice
  ([#124](https://github.com/zumenjs/zumen/issues/124)).
- A building model is 3D, in model units with Z up; `Scene` is 2D in paper millimeters, as
  [decision 0001](0001-st-bridge-in-zumen.md) found for ST-Bridge. A 3D engine for building models needs its own
  contract, while following the embedding conventions of `DrawingViewer`.

## Options

1. **Leave models out.** zumen serves the PDF half of the BIM drawing review, and applications pair it with one of the
   IFC libraries above, all MPL, LGPL or AGPL, or an unmaintained one.
2. **Read IFC through an existing library.** The fastest start, against decision 0013.
3. **Our own reader in Rust, in zumen.** It shares the ISO 10303-21 syntax with SXF, and the notices, limits, id
   contract, bindings and WebAssembly build of every format. Plans of storeys become scenes, so everything that works on
   drawings works on them.
4. **Our own reader in a new repository of the organization.** zumen stays about drawings, but the syntax, `zumen-core`
   and the bindings would have to be published as crates first or copied, plans cut from a model would still need
   `Scene`, the renderers and the drafting kit of zumen, and format detection would be split between two projects.

## Decision

Option 3.

- **What is read.** IFC-SPF files (`.ifc`) of IFC2X3 TC1, IFC4 ADD2 TC1 and IFC4X3 ADD2. `.ifcZIP` (a ZIP archive of
  one IFC-SPF file) follows once the reader works; ifcXML is not planned, and IFC5 (IFCX, in development at
  buildingSMART) is watched.
- **Two crates.** `zumen-step` holds the ISO 10303-21 syntax (the header and DATA sections, values, the string escapes
  and the table of instances) and knows no schema. It is taken out of `zumen-sxf`, which keeps its scenes, ids and
  notices ([#125](https://github.com/zumenjs/zumen/issues/125)). `zumen-ifc` is the format crate
  ([#127](https://github.com/zumenjs/zumen/issues/127)). `zumen-step` is not a format crate: `zumen-sxf` and `zumen-ifc`
  depend on it, and format crates still do not depend on each other. `AGENTS.md` and the architecture notes gain the
  rule with #125, as they did for `zumen-convert`.
- **Rust only**, as DWG ([decision 0003](0003-dwg-in-rust.md)) and PDF ([decision 0020](0020-pdf-drawings-in-rust.md)).
  The reader is checked by the smallest synthetic IFC files that pin a rule (units, placements, string escapes,
  properties inherited from types), written by its tests; by the scenes of `@zumenjs/reader` against the native reader,
  as for DWG; by the exports of the BIM programs used in Japan in `ZUMEN_SAMPLES`
  ([#128](https://github.com/zumenjs/zumen/issues/128)); and by a native fuzz example on damaged copies. IfcOpenShell
  may be run locally to compare, as ezdxf is for DXF, and is never a dependency of a package.
- **The model.** The spatial structure (project, site, building, storeys, spaces), the elements and their types,
  property and quantity sets, materials, classifications, relationships (aggregation, containment, voids and fills,
  type assignment, groups), units and georeferencing (`IfcMapConversion`). The API that returns it and the format-neutral
  3D model scene are decided in [#126](https://github.com/zumenjs/zumen/issues/126), and the geometry kernel in
  [#129](https://github.com/zumenjs/zumen/issues/129). From the model come 3D meshes
  ([#130](https://github.com/zumenjs/zumen/issues/130)) and storey plans as scenes
  ([#131](https://github.com/zumenjs/zumen/issues/131)).
- **Ids.** An element is named by its GlobalId (22 characters). Instances without a GlobalId, such as grid axes, are
  named by their instance number in the file (`#120`). A storey plan draws each element as one entity named by the
  element's GlobalId, so that ids stay unique in a scene, as the parts of a DXF dimension are merged into it. The same
  bytes give the same ids, as for every format ([Entity ids](../architecture.md#entity-ids)), and
  `crates/zumen/tests/ids.json` records them. Programs that keep GlobalIds when they export again also let revisions be
  matched by id.
- **How the specification is used.** zumen implements IFC from the published documentation, and each reader cites the
  version and the clause of the entity it reads, as the other readers cite their specifications. It does not copy the
  documentation's text, figures or tables, and does not ship its EXPRESS or XSD schemas, or code generated from them.
  Each entity that the reader interprets is read by code written by hand, as `zumen-sxf` reads the AP202 entities of
  SXF: the names, the order of the attributes and their types are what reading the files needs. Entities it does not
  interpret are counted in `notices` by type. If validation against a whole schema is needed, a new record decides how.
- **Permission.** zumen treats publishing such an implementation as not needing buildingSMART's permission: the license
  covers the documentation, which says nothing about implementations, and buildingSMART publishes its own validation
  service under MIT on an open implementation. "IFC" names the format and never claims certification; IFC and
  buildingSMART are trademarks of buildingSMART International. If buildingSMART International or buildingSMART Japan
  says otherwise, this record is revisited.
- **Detection.** IFC is detected by the schema that the header's `FILE_SCHEMA` names. The SXF reader stops taking STEP
  files of other schemas first ([#124](https://github.com/zumenjs/zumen/issues/124)), and `detectDrawing` repeats both
  rules, with the test that compares the two languages.
- **What this record leaves to others.** ST-Bridge stays out (decision 0013) while buildingSMART Japan is asked for its
  terms ([#140](https://github.com/zumenjs/zumen/issues/140)); if it agrees, a record brings it back on the 3D model
  scene of #126. Whether zumen reads J-LandXML is [#142](https://github.com/zumenjs/zumen/issues/142), and where a
  building model for authoring lives and what it holds is [#139](https://github.com/zumenjs/zumen/issues/139).

## Consequences

- zumen's scope grows from drawings to the building models that come with them. The roadmap of the README gains the
  item now, and the README's description changes when the reader lands. The work is tracked in
  [#143](https://github.com/zumenjs/zumen/issues/143), under the roadmap of the organization,
  [#144](https://github.com/zumenjs/zumen/issues/144).
- Storey plans are scenes, so the viewer, PNG, SVG and PDF, measurement, closed regions, the comparison of revisions,
  markup, checks and the MCP tools work on models without a second implementation.
- The WebAssembly module grows with the reader and the geometry. #128 measures it, and
  [#133](https://github.com/zumenjs/zumen/issues/133) decides from the measurement whether models get a module of their
  own, so that applications that open drawings only do not download them.
- The rules of the BIM data review are not published yet. Areas, checks against rule sets and the review items
  ([#135](https://github.com/zumenjs/zumen/issues/135), [#136](https://github.com/zumenjs/zumen/issues/136),
  [#137](https://github.com/zumenjs/zumen/issues/137)) stay general until MLIT publishes them, and follow its schedule.
- Editing IFC in place ([#134](https://github.com/zumenjs/zumen/issues/134)) follows
  [decision 0012](0012-edit-in-place-and-write-every-format.md) on `zumen-step`, as `zumen-sxf` already does for P21.
- A contribution that adds the schema files, generated code or text of the documentation goes against this record.

## References

- ISO 16739-1:2024; [IFC 4.3 ADD2](https://ifc43-docs.standards.buildingsmart.org/) and its
  [cover and license](https://github.com/buildingSMART/IFC4.x-development/blob/master/content/cover.md), and
  [buildingSMART's page on IFC](https://github.com/buildingSMART/technical.buildingsmart.org/blob/main/Industry-Foundation-Classes-(IFC).md),
  read on 2026-10-23
- ISO 10303-21: the header section (`FILE_SCHEMA`) and the encoding of strings
- [buildingSMART IFC Validation Service](https://github.com/buildingSMART/validate) (MIT), read on 2026-10-23
- MLIT, [press release of 2026-09-18](https://www.mlit.go.jp/report/press/house05_hh_001151.html) on the basic concept
  for the BIM data review, with its [text](https://www.mlit.go.jp/report/press/content/002023661.pdf),
  [review items and their order](https://www.mlit.go.jp/report/press/content/002023662.pdf) and
  [schedule](https://www.mlit.go.jp/report/press/content/002023663.pdf); the
  [briefings on the BIM drawing review](https://www.mlit.go.jp/jutakukentiku/build/jutakukentiku_house_fr_000238.html);
  [the 24th meeting of the building BIM environment subcommittee, document 1](https://www.mlit.go.jp/jutakukentiku/build/content/001989777.pdf)
- [STB-KIT](https://github.com/taisei-oss/stb-kit) README, section 3, as cited in decision 0013
- [Decision 0001](0001-st-bridge-in-zumen.md), [decision 0003](0003-dwg-in-rust.md),
  [decision 0012](0012-edit-in-place-and-write-every-format.md), [decision 0013](0013-st-bridge-out-of-zumen.md),
  [decision 0020](0020-pdf-drawings-in-rust.md)
