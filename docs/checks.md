# Checks against CAD standards

`zumen_core::checks` checks a drawing against the rules of a CAD standard: layer names, colors, line types, line
widths, text heights, characters that should not be used, the sheet size and orientation, and scales. The rules are
data, a JSON rule set, so that a built-in standard and an organization's own rules use the same engine
([decision 0015](decisions/0015-analysis-in-zumen-core.md)). The checks read the scene, with the stable keys and the
shapes drawn that every format has, so they work the same on SXF, JWW, DXF and DWG.

| Surface | Use |
| --- | --- |
| Rust | `RuleSet::built_in("mlit-cad-2025")`, `RuleSet::from_json(text)`, `check_scene(&scene, &rules)` |
| Python | `scene.check(rules)` (every value), `zumen.tools.check_drawing(scene, rules=…)` (bounded JSON, `CheckInfo`) |
| Command line | `zumen check plan.sfc [--rules mlit-cad-2025 \| rules.json]`, which fails when a rule of level error is broken |
| MCP | `check_drawing` |

## Results

Each rule gives:

- `checked`: how many layers, entities, sheets or scales it checked.
- `unchecked`: how many it could not check because the drawing does not give the value. A rule without findings
  passes only for what it checked; `checked: 0` means that the drawing had nothing it could check.
- `findings`: one for each value that breaks the rule (a layer name, a color, a line type, a width, a text height, a
  character, a sheet size, a scale), with a message, the number of entities (or layers, sheets and scales) that have
  it, their ids and the names of their layers. Rust keeps the order in which the values first appear in the drawing;
  `zumen.tools.check_drawing` puts the most common value first and bounds the findings and ids
  (`max_findings`, `max_ids`).

A drawing passes (`passed`) when no rule of level `error` is broken; `warning` and `info` are reported but do not fail
the drawing or the command.

## Rule sets

```json
{
  "name": "office",
  "title": "Our drafting rules",
  "source": "https://example.com/rules",
  "rules": [
    { "id": "layers", "kind": "layer-name", "level": "error", "patterns": ["A-*", "S-*"], "maxWidth": 31 },
    { "id": "frame", "kind": "color", "layers": ["*-TTL*"], "colors": ["#ffffff", "#000000"] },
    { "id": "text", "kind": "text-height", "heights": [2.5, 3.5, 5], "tolerance": 0.05 }
  ]
}
```

Every rule has an `id` (unique in the set), a `kind`, a `level` (`error`, `warning` by default, or `info`) and may have
a `reference` (the section of the standard) and a `description`. The rules of entities (`color`, `line-type`,
`line-width`, `text-height` and `characters`) may have `layers`, patterns of the layer names whose entities they
check (an entity is checked when its own layer or a layer of the placement it sits in matches). A rule that lacks the
fields of its kind, or has fields of another kind, is refused with the reason.

Patterns of layer names are like shell patterns: `*` any characters, `?` one character, `[SDCM]` one of the characters,
`[A-Z]` a range and `[!0-9]` any other character. They are case-sensitive.

| `kind` | Fields | What it checks |
| --- | --- | --- |
| `layer-name` | `patterns`, `maxWidth` (at least one) | The name of every layer that holds an entity matches one of the patterns and is at most `maxWidth` long, counting ASCII and half-width katakana as 1 and other characters as 2 |
| `color` | `colors` (`#rrggbb`) | The colors drawn (lines, fills, texts, points; the opacity is not compared). The color the format assigns by layer or block is resolved, as drawn |
| `line-type` | `names` | The `lineType` property, compared without case. A value such as `実線（1）` (SXF: the name and its number) or `BYLAYER（DASHED）` (DXF: the line type taken from the layer) matches by the whole value, the part before the brackets or the part inside them. A value that is only a number (JWW pen styles) names no line type and is unchecked |
| `line-width` | `widths` (mm), `tolerance` (0.001) | The `lineWidth` property (`raw` in millimeters). DXF and DWG entities that take their width from the layer or block, or have the default width, give no number and are unchecked |
| `text-height` | `heights` (mm), `tolerance` (0.01) | The height of every text part on the sheet, as the drawing gives it: the height of the character frame in SXF and JWW, and the height of capitals in DXF and DWG. Each text primitive carries its declared `characterHeight` separately from the drawing height (`height`, the glyph em). This stays with dimension texts when their parts are merged. A drawing without a sheet (DXF and DWG model space) has no height on paper and its texts are unchecked |
| `characters` | `ranges` (`2116`, `FF61-FF9F`, hexadecimal code points) | The characters of texts and of layer names |
| `sheet-size` | `sizes` (`{name, width, height}` in mm), `tolerance` (1) | The sheet, in either orientation. A drawing without a sheet is unchecked |
| `orientation` | `orientation` (`landscape`, `portrait`) | The longer side of the sheet |
| `scale` | `scales` (N), `mantissas` (m) | Every scale 1:N: N is one of `scales`, or m × 10ⁿ for an integer n. The scales are the `scale` property of entities (JWW layer groups, DXF and DWG viewports) and the scales written in the `scale` field of the drawing (`1:100`, `1/50`, `S=1：200` in the SXF title block attributes) |

## Built-in rule sets

### `mlit-cad-2025`: CAD製図基準（令和7年12月）

The CAD drafting standard of the Ministry of Land, Infrastructure, Transport and Tourism of Japan for civil works,
edition of December 2025 (CAD製図基準・同解説, 土木 202512-01,
[cad8.pdf](https://www.cals-ed.go.jp/mg/wp-content/uploads/cad8.pdf)). The rule set is
[`crates/zumen-core/rules/mlit-cad-2025.json`](../crates/zumen-core/rules/mlit-cad-2025.json).

| Rule | Level | Section | Rule |
| --- | --- | --- | --- |
| `layer-name` | error | 1-5-7, 表 1-4 | `[SDCM]-<object>` with an optional `-<user region>`, the object one of TTL, BGD, BMK, STR, BYP, MTR, DCR, DOC, SUV, and ORD only with C (発注用レイヤ); at most 256 half-width characters |
| `color` | warning | 1-5-9, 解説表 1-4 | The 16 colors with their RGB values: black, red, green, blue, yellow, magenta, cyan, white, 牡丹, 茶, 橙, 薄緑, 明青, 青紫, 明灰, 暗灰 |
| `line-type` | warning | 1-5-10, 表 1-6 | The 15 line types of JIS Z 8312:1999, by their Japanese names and the names of the SXF predefined line types (`continuous`, `dashed` …) |
| `line-width` | warning | 1-5-10 (3) | 0.13, 0.18, 0.25, 0.35, 0.5, 0.7, 1, 1.4 and 2 mm |
| `text-height` | warning | 1-5-11 (2) | 1.8, 2.5, 3.5, 5, 7, 10, 14 and 20 mm |
| `characters` | warning | 1-5-11 解説 (2) | No machine-dependent characters (機種依存文字): half-width katakana, circled numbers, Roman numerals, parenthesized and circled ideographs, the CJK compatibility block (㍉, ㎡ …), 〝〟, №, ℡, ∑, ∟, ∮, ⊿ |
| `sheet-size` | warning | 1-4-1 | A0 to A4 (A-series, the standard size; the extended sizes of 解説表 1-1 are allowed where A sizes do not fit, so they are reported for review) |
| `orientation` | info | 1-4-2 | Landscape (portrait is allowed for tall structures after consultation) |
| `scale` | info | 1-4-5 | 1:A with A = 1, 2, 5 × 10ⁿ (preferred) or 1.5, 2.5, 3, 4, 6 × 10ⁿ (JIS Z 8314:1998, as the explanation of 1-4-5 cites) |

Rules of the standard that the checks do not cover yet:

- The thickness of lines by sheet size and use (表 1-7: thin, thick and extra thick lines, dimension lines and
  borders), the border and its margins (1-4-3), and the size and fields of the title block (1-4-4). They need the frame
  and the title block, which ruled tables can find (`zumen_core::tables`).
- The file names (1-5-2) and the fonts of texts (JIS Z 8313).
- Whether a text uses only the characters of JIS X 0208 (使用文字 of the electronic delivery guidelines): the rule set
  lists the machine-dependent characters seen most in drawings, not every character outside JIS X 0208.
