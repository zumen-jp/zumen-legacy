import { decodeMif } from "./tags.js";

/**
 * TEXT の制御コード（%%c など）と MTEXT の書式コード（\P、\H など）を読み、同じ書式の文字の並び（run）にする。
 * 書式コードは AutoCAD の「Format Multiline Text in an Alternate Text Editor」の一覧と、
 * ezdxf（MIT）の tools/text.py の MTextParser の解釈に合わせた。
 */

export interface TextRun {
  text: string;
  /** 文字の高さ（図面の単位） */
  height: number;
  widthFactor: number;
  /** 斜体の角度（度） */
  oblique: number;
  /** ACI（数値）か #rrggbb。undefined は図形の色 */
  color?: number | string;
  /** \F で変えたフォントのファイル（txt、romans.shx など）か、\f で変えた TrueType のフォント名 */
  font?: { name: string; truetype: boolean };
  underline: boolean;
  overline: boolean;
  strike: boolean;
  /** 積み重ね（\S）。kind は ^（公差）、/（横線の分数）、#（斜めの分数） */
  stack?: { upper: string; lower: string; kind: string };
}

export type ParagraphAlign = "left" | "center" | "right";

export interface TextParagraph {
  runs: TextRun[];
  /** \pxq の段落の揃え。undefined は配置点（71）に従う */
  align?: ParagraphAlign;
}

interface Style {
  height: number;
  widthFactor: number;
  oblique: number;
  color?: number | string;
  font?: { name: string; truetype: boolean };
  underline: boolean;
  overline: boolean;
  strike: boolean;
}

/** TEXT・MTEXT に共通の %% コード。%%c は直径、%%d は度、%%p はプラスマイナス */
function specialCharacter(code: string): string | null {
  switch (code.toLowerCase()) {
    case "c":
      return "Ø";
    case "d":
      return "°";
    case "p":
      return "±";
    case "%":
      return "%";
  }
  return null;
}

function hex(rgb: number): string {
  return `#${(rgb & 0xffffff).toString(16).padStart(6, "0")}`;
}

/**
 * TEXT・ATTRIB の文字列を run にする。%%u・%%o・%%k は下線・上線・取り消し線の切り替え、
 * %%nnn は文字コード nnn の文字（DXF Reference「TEXT」、AutoCAD の「Control Codes and Special Characters」）、
 * 「^ 」は ^、^I（タブ）は 4 つの空白。1 行の文字に改行はないので、ほかのキャレット表記（^J など）はそのまま残す。
 */
export function parseTextCodes(text: string, height: number, widthFactor: number, oblique: number): TextRun[] {
  const runs: TextRun[] = [];
  const style: Style = { height, widthFactor, oblique, underline: false, overline: false, strike: false };
  let buffer = "";
  const flush = () => {
    if (buffer) runs.push({ ...style, text: buffer });
    buffer = "";
  };
  for (let i = 0; i < text.length; i++) {
    const ch = text[i] as string;
    if (ch === "^" && (text[i + 1] === " " || text[i + 1] === "I")) {
      // テキストの DXF は ^ を「^ 」、タブを ^I と書く（DXF Reference「About ASCII Control Characters in DXF Files」）。
      // タブは MTEXT と同じく 4 つの空白にする
      buffer += text[i + 1] === " " ? "^" : "    ";
      i++;
      continue;
    }
    if (ch !== "%" || text[i + 1] !== "%" || i + 2 >= text.length) {
      buffer += ch;
      continue;
    }
    const code = text[i + 2] as string;
    const lower = code.toLowerCase();
    if (lower === "u" || lower === "o" || lower === "k") {
      flush();
      if (lower === "u") style.underline = !style.underline;
      else if (lower === "o") style.overline = !style.overline;
      else style.strike = !style.strike;
      i += 2;
      continue;
    }
    const digits = /^\d{3}/.exec(text.slice(i + 2));
    if (digits) {
      buffer += String.fromCharCode(Number(digits[0]));
      i += 4;
      continue;
    }
    const special = specialCharacter(code);
    if (special !== null) {
      buffer += special;
      i += 2;
      continue;
    }
    buffer += ch;
  }
  flush();
  return runs;
}

/** \H・\W の値。末尾の x は今の値に対する倍率 */
function relative(value: string, current: number): number {
  const trimmed = value.trim();
  const isRelative = /x$/i.test(trimmed);
  const number = Number.parseFloat(isRelative ? trimmed.slice(0, -1) : trimmed);
  if (!Number.isFinite(number) || number <= 0) return current;
  return isRelative ? current * number : number;
}

/**
 * MTEXT の文字列を段落と run にする。height・widthFactor・oblique は図形と文字スタイルの値。
 * 段組み・タブ位置・字間（\T）・行の中の縦の揃え（\A）は読み飛ばす。
 */
export function parseMText(raw: string, height: number, widthFactor: number, oblique: number): TextParagraph[] {
  const paragraphs: TextParagraph[] = [{ runs: [] }];
  let style: Style = { height, widthFactor, oblique, underline: false, overline: false, strike: false };
  let align: ParagraphAlign | undefined;
  const groups: Style[] = [];
  let buffer = "";
  const current = () => paragraphs[paragraphs.length - 1] as TextParagraph;
  const flush = () => {
    if (buffer) current().runs.push({ ...style, text: buffer });
    buffer = "";
  };
  const newParagraph = () => {
    flush();
    paragraphs.push(align ? { runs: [], align } : { runs: [] });
  };
  /** i から ; までの値（; は含まない）と、; の次の位置 */
  const valueAt = (i: number): [string, number] => {
    const end = raw.indexOf(";", i);
    if (end < 0) return [raw.slice(i), raw.length];
    return [raw.slice(i, end), end + 1];
  };
  let i = 0;
  while (i < raw.length) {
    const ch = raw[i] as string;
    if (ch === "{") {
      flush();
      groups.push({ ...style });
      i++;
      continue;
    }
    if (ch === "}") {
      flush();
      style = groups.pop() ?? style;
      i++;
      continue;
    }
    if (ch === "^" && i + 1 < raw.length) {
      // キャレット表記（^I はタブ、^J は改行、^ と空白は ^ そのもの）
      const next = raw[i + 1] as string;
      if (next === " ") buffer += "^";
      else if (next === "I") buffer += "    ";
      else if (next === "J" || next === "M") newParagraph();
      else buffer += " ";
      i += 2;
      continue;
    }
    if (ch === "%" && raw[i + 1] === "%" && i + 2 < raw.length) {
      const special = specialCharacter(raw[i + 2] as string);
      if (special !== null) {
        buffer += special;
        i += 3;
        continue;
      }
    }
    if (ch !== "\\" || i + 1 >= raw.length) {
      buffer += ch;
      i++;
      continue;
    }
    const command = raw[i + 1] as string;
    switch (command) {
      case "\\":
      case "{":
      case "}":
        buffer += command;
        i += 2;
        break;
      case "P":
      case "X":
      case "N":
        newParagraph();
        i += 2;
        break;
      case "~":
        buffer += "\u00a0";
        i += 2;
        break;
      case "U":
      case "u": {
        const match = /^\+([0-9A-Fa-f]{4})/.exec(raw.slice(i + 2, i + 7));
        if (match) {
          buffer += String.fromCodePoint(Number.parseInt(match[1] as string, 16));
          i += 7;
        } else {
          buffer += command;
          i += 2;
        }
        break;
      }
      case "M":
      case "m": {
        const match = /^\+([1-5])([0-9A-Fa-f]{4})/.exec(raw.slice(i + 2, i + 8));
        const decoded = match ? decodeMif(match[1] as string, match[2] as string) : null;
        if (match && decoded !== null) {
          buffer += decoded;
          i += 8;
        } else {
          buffer += command;
          i += 2;
        }
        break;
      }
      case "L":
      case "l":
      case "O":
      case "o":
      case "K":
      case "k": {
        flush();
        const on = command === command.toUpperCase();
        const lower = command.toLowerCase();
        style = {
          ...style,
          ...(lower === "l" ? { underline: on } : lower === "o" ? { overline: on } : { strike: on }),
        };
        i += 2;
        break;
      }
      case "H":
      case "W":
      case "Q":
      case "C":
      case "c":
      case "F":
      case "f": {
        flush();
        const [value, next] = valueAt(i + 2);
        style = { ...style };
        if (command === "H") style.height = relative(value, style.height);
        else if (command === "W") style.widthFactor = relative(value, style.widthFactor);
        else if (command === "Q") {
          const angle = Number.parseFloat(value);
          if (Number.isFinite(angle)) style.oblique = angle;
        } else if (command === "C") {
          const aci = Number.parseInt(value, 10);
          // 256 は BYLAYER（図形の色に戻す）
          if (Number.isFinite(aci)) style.color = aci === 256 ? undefined : aci;
        } else if (command === "c") {
          // 値は 0xBBGGRR の並び（ezdxf の parse_rgb_color と同じ）
          const bgr = Number.parseInt(value, 10);
          if (Number.isFinite(bgr)) {
            const r = bgr & 0xff;
            const g = (bgr >> 8) & 0xff;
            const b = (bgr >> 16) & 0xff;
            style.color = hex((r << 16) | (g << 8) | b);
          }
        } else {
          const name = value.split("|")[0]?.trim();
          style.font = name ? { name, truetype: command === "f" } : undefined;
        }
        i = next;
        break;
      }
      case "T":
      case "A": {
        const [, next] = valueAt(i + 2);
        i = next;
        break;
      }
      case "p": {
        const [value, next] = valueAt(i + 2);
        const match = /q([lcrjd])/.exec(value);
        if (match) {
          const kind = match[1];
          align = kind === "c" ? "center" : kind === "r" ? "right" : kind === "l" ? "left" : undefined;
          // 段落の先頭の \p は、その段落の揃えにする
          if (buffer === "" && current().runs.length === 0) {
            if (align) current().align = align;
            else delete current().align;
          }
        }
        i = next;
        break;
      }
      case "S": {
        flush();
        // \S上^下; の区切り（^ / #）。\ の後の文字は区切りとみなさない
        let j = i + 2;
        let upper = "";
        let lower = "";
        let kind = "";
        let escaped = false;
        for (; j < raw.length; j++) {
          const c = raw[j] as string;
          if (!escaped && c === ";") break;
          if (!escaped && c === "\\") {
            escaped = true;
            continue;
          }
          if (!escaped && !kind && (c === "^" || c === "/" || c === "#")) {
            kind = c;
            escaped = false;
            continue;
          }
          if (kind) lower += c;
          else upper += c;
          escaped = false;
        }
        current().runs.push({ ...style, text: "", stack: { upper: upper.trim(), lower: lower.trim(), kind } });
        i = j + 1;
        break;
      }
      default:
        buffer += command;
        i += 2;
    }
  }
  flush();
  return paragraphs;
}

/** MTEXT の書式コードを除いた文字列。段落は改行、積み重ねは「上/下」 */
export function mtextPlainText(raw: string): string {
  return parseMText(raw, 1, 1, 0)
    .map((paragraph) =>
      paragraph.runs.map((run) => (run.stack ? `${run.stack.upper}/${run.stack.lower}` : run.text)).join(""),
    )
    .join("\n");
}

export { hex as rgbHex };
