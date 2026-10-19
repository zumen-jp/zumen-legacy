/** XML の要素。名前空間の接頭辞は外した名前で持つ */
export interface XmlElement {
  name: string;
  attributes: Record<string, string>;
  children: XmlElement[];
  /** 子要素を除いた文字の内容。空白だけの部分は持たない */
  text: string;
}

const ENTITIES: Record<string, string> = { lt: "<", gt: ">", amp: "&", quot: '"', apos: "'" };

function decodeEntities(value: string): string {
  if (!value.includes("&")) return value;
  return value.replace(/&(#x[0-9a-fA-F]+|#[0-9]+|[a-zA-Z]+);/g, (whole, body: string) => {
    if (body.startsWith("#x")) return String.fromCodePoint(Number.parseInt(body.slice(2), 16));
    if (body.startsWith("#")) return String.fromCodePoint(Number.parseInt(body.slice(1), 10));
    return ENTITIES[body] ?? whole;
  });
}

function localName(name: string): string {
  const colon = name.indexOf(":");
  return colon >= 0 ? name.slice(colon + 1) : name;
}

function isSpace(code: number): boolean {
  return code === 0x20 || code === 0x09 || code === 0x0a || code === 0x0d;
}

/** from〜to に空白以外の文字があるか */
function hasContent(text: string, from: number, to: number): boolean {
  for (let k = from; k < to; k++) if (!isSpace(text.charCodeAt(k))) return true;
  return false;
}

function lineOf(text: string, index: number): number {
  let line = 1;
  for (let i = 0; i < index && i < text.length; i++) if (text.charCodeAt(i) === 0x0a) line++;
  return line;
}

/**
 * 整形式の XML を要素の木にする。検証はしない。ST-Bridge の読み取りに要る範囲（要素・属性・文字・CDATA・
 * コメント・処理命令・DOCTYPE の読み飛ばし）だけを扱い、DOM を使わないので Node.js でも動く。
 */
export function parseXml(text: string): XmlElement {
  const stack: XmlElement[] = [];
  let root: XmlElement | null = null;
  let i = 0;
  const length = text.length;
  const fail = (message: string, at: number): never => {
    throw new Error(`XML を読めません（${lineOf(text, at)} 行目）: ${message}`);
  };
  const appendText = (value: string) => {
    const current = stack[stack.length - 1];
    if (!current) return;
    current.text += value;
  };

  while (i < length) {
    const lt = text.indexOf("<", i);
    if (lt < 0) {
      if (stack.length > 0 && hasContent(text, i, length)) appendText(decodeEntities(text.slice(i)));
      break;
    }
    if (lt > i && stack.length > 0 && hasContent(text, i, lt)) appendText(decodeEntities(text.slice(i, lt)));
    const next = text.charCodeAt(lt + 1);
    if (next === 0x3f /* ? */) {
      const end = text.indexOf("?>", lt + 2);
      if (end < 0) fail("処理命令が閉じていません", lt);
      i = end + 2;
      continue;
    }
    if (next === 0x21 /* ! */) {
      if (text.startsWith("<!--", lt)) {
        const end = text.indexOf("-->", lt + 4);
        if (end < 0) fail("コメントが閉じていません", lt);
        i = end + 3;
        continue;
      }
      if (text.startsWith("<![CDATA[", lt)) {
        const end = text.indexOf("]]>", lt + 9);
        if (end < 0) fail("CDATA が閉じていません", lt);
        appendText(text.slice(lt + 9, end));
        i = end + 3;
        continue;
      }
      // DOCTYPE。内部サブセット [ ... ] を含めて読み飛ばす
      let depth = 0;
      let j = lt + 2;
      for (; j < length; j++) {
        const c = text.charCodeAt(j);
        if (c === 0x5b) depth++;
        else if (c === 0x5d) depth--;
        else if (c === 0x3e && depth <= 0) break;
      }
      i = j + 1;
      continue;
    }
    if (next === 0x2f /* / */) {
      const end = text.indexOf(">", lt + 2);
      if (end < 0) fail("終了タグが閉じていません", lt);
      const name = localName(text.slice(lt + 2, end).trim());
      const open = stack.pop();
      if (!open || open.name !== name) fail(`終了タグ </${name}> が開始タグと対応しません`, lt);
      i = end + 1;
      continue;
    }

    // 開始タグ
    let j = lt + 1;
    while (j < length && !isSpace(text.charCodeAt(j)) && text[j] !== ">" && text[j] !== "/") j++;
    const element: XmlElement = { name: localName(text.slice(lt + 1, j)), attributes: {}, children: [], text: "" };
    if (!element.name) fail("要素名がありません", lt);
    let selfClosing = false;
    for (;;) {
      while (j < length && isSpace(text.charCodeAt(j))) j++;
      if (j >= length) fail(`<${element.name}> が閉じていません`, lt);
      const c = text[j];
      if (c === ">") {
        j++;
        break;
      }
      if (c === "/" && text[j + 1] === ">") {
        selfClosing = true;
        j += 2;
        break;
      }
      const nameStart = j;
      while (j < length && text[j] !== "=" && !isSpace(text.charCodeAt(j)) && text[j] !== ">" && text[j] !== "/") j++;
      const attributeName = text.slice(nameStart, j);
      while (j < length && isSpace(text.charCodeAt(j))) j++;
      if (text[j] !== "=") fail(`属性 ${attributeName} に値がありません`, j);
      j++;
      while (j < length && isSpace(text.charCodeAt(j))) j++;
      const quote = text[j];
      if (quote !== '"' && quote !== "'") fail(`属性 ${attributeName} の値が引用符で囲まれていません`, j);
      const valueEnd = text.indexOf(quote as string, j + 1);
      if (valueEnd < 0) fail(`属性 ${attributeName} の値が閉じていません`, j);
      if (!attributeName.startsWith("xmlns")) {
        element.attributes[localName(attributeName)] = decodeEntities(text.slice(j + 1, valueEnd));
      }
      j = valueEnd + 1;
    }
    const parent = stack[stack.length - 1];
    if (parent) parent.children.push(element);
    else if (!root) root = element;
    else fail("ルート要素が複数あります", lt);
    if (!selfClosing) stack.push(element);
    i = j;
  }
  if (stack.length > 0) fail(`<${stack[stack.length - 1]?.name}> が閉じていません`, length);
  if (!root) throw new Error("XML の要素がありません");
  return root;
}
