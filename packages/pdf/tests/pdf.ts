/**
 * テストの PDF を組み立てる。objects[i] がオブジェクト i + 1 の中身で、1 を文書カタログにする。
 * 中身は ASCII だけにして、文字の数をバイトの位置にする（相互参照表、ISO 32000-1:2008, 7.5.4）
 */
export function buildPdf(objects: readonly string[]): Uint8Array {
  let text = "%PDF-1.7\n";
  const offsets: number[] = [];
  objects.forEach((body, index) => {
    offsets.push(text.length);
    text += `${index + 1} 0 obj\n${body}\nendobj\n`;
  });
  const xref = text.length;
  text += `xref\n0 ${objects.length + 1}\n0000000000 65535 f \n`;
  for (const offset of offsets) text += `${String(offset).padStart(10, "0")} 00000 n \n`;
  text += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
  return new TextEncoder().encode(text);
}

/** ストリームのオブジェクト（7.3.8）。entries は辞書に足す項目 */
export function stream(content: string, entries = ""): string {
  return `<< ${entries} /Length ${content.length} >>\nstream\n${content}\nendstream`;
}

/**
 * 3 ページの図面。中身の座標（ページの左上を原点に y が下向きの pt）で書くと、
 * 1 ページ目（200 × 100 pt）:
 * - x 0〜100 が赤、x 100〜200 が青、その上に y 90〜100 の帯が緑
 * - optional content group A（オブジェクト 6、既定で表示）の黄色の正方形 x 20〜40、y 40〜60
 * - optional content group B（オブジェクト 7、既定で非表示）の水色の正方形 x 160〜180、y 40〜60
 * - 外観を持つ四角の注釈の紫 x 60〜80、y 20〜40
 * 2 ページ目（200 × 100 pt を /Rotate 90 で回し、100 × 200 pt）:
 * - y 0〜100 が赤、y 100〜200 が青、その上に x 0〜10 の帯が緑
 * 3 ページ目（100 × 100 pt）:
 * - 左下の x 0〜10、y 90〜100 だけが赤で、ほかは何も描かない
 */
export function drawingPdf(): Uint8Array {
  const page1 = [
    "1 0 0 rg 0 0 100 100 re f",
    "0 0 1 rg 100 0 100 100 re f",
    "0 1 0 rg 0 0 200 10 re f",
    "/OC /A BDC 1 1 0 rg 20 40 20 20 re f EMC",
    "/OC /B BDC 0 1 1 rg 160 40 20 20 re f EMC",
  ].join("\n");
  const page2 = ["1 0 0 rg 0 0 100 100 re f", "0 0 1 rg 100 0 100 100 re f", "0 1 0 rg 0 0 200 10 re f"].join("\n");
  return buildPdf([
    "<< /Type /Catalog /Pages 2 0 R /OCProperties << /OCGs [6 0 R 7 0 R] /D << /ON [6 0 R] /OFF [7 0 R] /Order [6 0 R 7 0 R] >> >> >>",
    "<< /Type /Pages /Kids [3 0 R 4 0 R 11 0 R] /Count 3 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] /Resources << /Properties << /A 6 0 R /B 7 0 R >> >> /Contents 5 0 R /Annots [9 0 R] >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] /Rotate 90 /Contents 8 0 R >>",
    stream(page1),
    "<< /Type /OCG /Name (A) >>",
    "<< /Type /OCG /Name (B) >>",
    stream(page2),
    "<< /Type /Annot /Subtype /Square /Rect [60 60 80 80] /F 4 /AP << /N 10 0 R >> >>",
    stream("1 0 1 rg 0 0 20 20 re f", "/Type /XObject /Subtype /Form /BBox [0 0 20 20]"),
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 12 0 R >>",
    stream("1 0 0 rg 0 0 10 10 re f"),
  ]);
}
