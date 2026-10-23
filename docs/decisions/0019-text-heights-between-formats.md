# 0019: Keep the number of a text height between DXF, DWG, SXF and JWW, and let each format draw its glyphs

| Item | Value |
| --- | --- |
| Date | 2026-10-23 |
| Status | Accepted |

## Context

- The formats state a text's size differently. SXF (文字範囲の高さ) and JWW (文字の大きさの縦) give the height of the
  character frame, and draw the glyph em to fill it. DXF and DWG give the TEXT height (code 40), which zumen-dxf takes
  as the kanji height (0.88 em) for text with full-width characters and the capital height (0.72 em) otherwise, so
  the em is 1/0.88 or 1/0.72 of the height (`zumen_core::text::em_for`, [DXF](../formats/dxf.md)).
- `zumen-convert` wrote the em as the SXF and JWW frame height, and the frame height as the em of the TEXT, so that the
  converted file drew the same glyphs. A 2.5 mm DXF text became a 2.84 mm SXF text (2.5 / 0.88), or 3.47 mm for Latin
  text (2.5 / 0.72); a 2.5 mm JWW text became a 2.2 mm DXF text.
- People enter and read the text height as a number: 2.5 in AutoCAD's height, 2.5 in Jw_cad's 文字種 and an SXF
  editor's 文字高. CAD製図基準 (国土交通省, 令和7年12月) 1-5-11 counts text heights in that number (1.8, 2.5, 3.5, 5,
  7 … mm), and so do the check `mlit-cad-2025` ([Checks](../checks.md)) and the checkers used for electronic delivery.
  A drawing spec of the drafting kit ([decision 0018](0018-drafting-kit.md)) gives `height` 2.5 for every format.
- The two descriptions of a glyph cannot agree: an SXF frame of 2.5 mm whose glyphs are as large as a 2.5 mm DXF text
  would draw them outside the frame, which the SXF specification and other SXF viewers do not do.

## Options

1. **Keep the glyphs** (as before): write the em as the frame height. The converted drawing prints the same, but every
   standard height becomes a number the standards do not list (2.84, 3.47), every converted drawing fails the text
   height rule, and DXF → SXF → DXF is exact only because both directions use the same factor.
2. **Keep the number**: the SXF frame height and the JWW size are the TEXT height, and the TEXT height is the frame
   height. Each format draws the glyphs its own way: SXF and JWW draw them 0.88 (full-width) or 0.72 (Latin) times as
   large as DXF and DWG.
3. **Keep both** by drawing SXF and JWW glyphs larger than their frame (the em at 1/0.88 of the frame). It changes how
   every SXF and JWW file is drawn, against their specifications, and other viewers would still draw them smaller.

## Decision

Option 2. The text height is the number people enter and the standards count, so a conversion keeps it, as Jw_cad and
SXF editors keep the number when they exchange DXF.

- **DXF, DWG and the drafting kit → SXF, JWW** (`text_height_box` in `zumen-convert`): the text box that the DXF scene
  draws is shrunk by 0.88 or 0.72 (`em_for` of its text) about the left end of its baseline (the start of the baseline
  for vertical text), so the frame height is the TEXT height, the baseline and the start of the text stay where they
  are, and the glyphs keep their proportions (the text is shorter by the same factor).
- **SXF, JWW → DXF** (`text_entity`): the TEXT height is the frame height, with the same baseline, start and width
  factor, so DXF draws the glyphs 1/0.88 or 1/0.72 as large as SXF and JWW.
- The two directions are inverse: DXF → SXF → DXF and JWW → DXF → JWW give back the same heights and positions.

## Consequences

- `zumen check` reports the same text heights for a drawing in every format; the drafting kit's sheets have 2.5 mm
  texts in SXF and JWW as in DXF.
- A converted drawing no longer prints the same text size: DXF → SXF or JWW draws texts smaller (Latin text 0.72
  times), and SXF or JWW → DXF draws them larger, so text written to fit a frame tightly in SXF can overlap it in DXF.
  The tests that compare the scene before and after a conversion compare texts after the same scaling.
- Texts in DXF that come from an SXF or JWW frame (via `zumen-convert`) and texts written by AutoCAD are treated alike;
  the factor depends only on whether the text has full-width characters, as in the DXF scene.

## References

- 国土交通省「CAD製図基準」令和7年12月, 1-5-11 文字
- SXF Ver.3.1 仕様書・同解説 表現仕様編: text_string_feature (文字範囲の高さ・幅)
- Jw_cad 外部変形・jwdatafmt.txt: CDataMoji (文字の大きさ)
- [Autodesk DXF Reference](https://help.autodesk.com/cloudhelp/2026/ENU/AutoCAD-DXF/files/GUID-235B22E0-A567-4CF6-92D3-38A2306D73F3.htm):
  TEXT (group code 40)
