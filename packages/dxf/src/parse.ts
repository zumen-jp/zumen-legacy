import { NoticeList } from "@zumenjs/core";
import type {
  DxfAttdef,
  DxfAttrib,
  DxfBlock,
  DxfDimension,
  DxfDimStyle,
  DxfDocument,
  DxfEntity,
  DxfEntityBase,
  DxfHatch,
  DxfHatchEdge,
  DxfHatchPath,
  DxfImage,
  DxfInsert,
  DxfLayer,
  DxfLayout,
  DxfLinetype,
  DxfMLeader,
  DxfMLeaderBlock,
  DxfMLeaderContext,
  DxfMLeaderLeader,
  DxfMLeaderLine,
  DxfMLeaderMText,
  DxfMLine,
  DxfMLineStyle,
  DxfMText,
  DxfPatternLine,
  DxfPolyline,
  DxfSpace,
  DxfSpline,
  DxfTextBase,
  DxfTextStyle,
  DxfViewport,
  Vec2,
  Vec3,
} from "./model.js";
import { mtextPlainText } from "./mtext.js";
import { type DxfTags, decodeEscapes, readTags } from "./tags.js";

/** 図形のタグのうち、102 の組・拡張データ（1001 以降）・埋め込みの図形（101 以降）を除いた位置 */
type Body = number[];

interface Owner {
  space: DxfSpace;
  block?: string;
}

const MODEL_SPACE = "*MODEL_SPACE";
const PAPER_SPACE = "*PAPER_SPACE";

function isModelBlock(name: string): boolean {
  const upper = name.toUpperCase();
  return upper === MODEL_SPACE || upper === "$MODEL_SPACE";
}

function isPaperBlock(name: string): boolean {
  const upper = name.toUpperCase();
  return upper.startsWith(PAPER_SPACE) || upper === "$PAPER_SPACE";
}

/** 図形の種類ごとに読む図形。ここにない種類は件数だけを skipped に数える */
const READ_TYPES = new Set([
  "LINE",
  "POINT",
  "CIRCLE",
  "ARC",
  "ELLIPSE",
  "LWPOLYLINE",
  "POLYLINE",
  "SPLINE",
  "HELIX",
  "TEXT",
  "ATTRIB",
  "ATTDEF",
  "MTEXT",
  "INSERT",
  "DIMENSION",
  "ARC_DIMENSION",
  "LARGE_RADIAL_DIMENSION",
  "HATCH",
  "MPOLYGON",
  "SOLID",
  "TRACE",
  "3DFACE",
  "LEADER",
  "MULTILEADER",
  "MLEADER",
  "MLINE",
  "VIEWPORT",
  "XLINE",
  "RAY",
  "IMAGE",
  "WIPEOUT",
  "ACAD_TABLE",
]);

/** MULTILEADER の色の生の値の BYBLOCK（上位 1 バイトが種類: 0xC0 BYLAYER、0xC1 BYBLOCK、0xC2 RGB、0xC3 ACI） */
const BY_BLOCK_RAW_COLOR = 0xc1000000 | 0;

/** MULTILEADER・MLINE で、Y・Z（+10・+20）が続く点の X のグループコード */
const MLEADER_POINT_CODES: ReadonlySet<number> = new Set([10, 11, 12, 13, 14, 15, 16, 110, 111, 112]);
const MLINE_POINT_CODES: ReadonlySet<number> = new Set([10, 11, 12, 13]);

class Parser {
  private readonly codes: Int32Array;
  private readonly values: readonly (string | number)[];
  private readonly length: number;
  private i = 0;
  readonly doc: DxfDocument;
  /** BLOCK_RECORD のハンドル → ブロックの名前 */
  private readonly blockRecords = new Map<string, string>();
  private readonly layoutRecords: { handle: string; block_record_handle: string; layout: DxfLayout }[] = [];
  private readonly sortTables: { owner: string; handles: Record<string, string> }[] = [];
  private hasPaperEntities = false;

  constructor(
    tags: DxfTags,
    private readonly notices: NoticeList,
    version: string,
    binary: boolean,
    encoding: string,
    encodingSource: DxfDocument["encoding_source"],
    codepage: string | undefined,
  ) {
    this.codes = tags.codes;
    this.values = tags.values;
    this.length = tags.length;
    this.doc = {
      format: "dxf",
      version,
      binary,
      encoding,
      encoding_source: encodingSource,
      header: {
        insunits: 0,
        codepage,
        ltscale: 1,
        psltscale: 1,
        dimscale: 1,
        dimasz: 0.18,
        dimtxt: 0.18,
        tilemode: 1,
        variables: {},
      },
      layers: [],
      linetypes: [],
      styles: [],
      dimstyles: [],
      mline_styles: [],
      blocks: [],
      layouts: [],
      entities: [],
      // 名前（ブロック・図形の種類）をキーにするので、__proto__ などが Object の既定の値と重ならないようにする
      sort_handles: Object.create(null) as DxfDocument["sort_handles"],
      skipped: Object.create(null) as DxfDocument["skipped"],
      notices: [],
    };
  }

  private code(k: number): number {
    return this.codes[k] as number;
  }

  private num(k: number): number {
    const value = this.values[k];
    return typeof value === "number" ? value : Number(value) || 0;
  }

  private raw(k: number): string {
    const value = this.values[k];
    return typeof value === "string" ? value : String(value);
  }

  /** 文字列の値。\U+XXXX・\M+nXXXX を文字にする */
  private str(k: number): string {
    return decodeEscapes(this.raw(k));
  }

  private is(k: number, code: number, value: string): boolean {
    return k < this.length && this.code(k) === code && this.raw(k).trim().toUpperCase() === value;
  }

  /** k より後の、グループコード 0 の位置 */
  private nextZero(k: number): number {
    let j = k + 1;
    while (j < this.length && this.code(j) !== 0) j++;
    return j;
  }

  parse(): DxfDocument {
    let sections = 0;
    while (this.i < this.length) {
      if (this.is(this.i, 0, "EOF")) break;
      if (!this.is(this.i, 0, "SECTION")) {
        this.i++;
        continue;
      }
      sections++;
      const name = this.i + 1 < this.length && this.code(this.i + 1) === 2 ? this.raw(this.i + 1).trim() : "";
      this.i += 2;
      switch (name.toUpperCase()) {
        case "HEADER":
          this.header();
          break;
        case "TABLES":
          this.tables();
          break;
        case "BLOCKS":
          this.blocks();
          break;
        case "ENTITIES":
          this.entitiesSection();
          break;
        case "OBJECTS":
          this.objects();
          break;
        default:
          this.skipSection();
      }
    }
    if (sections === 0) this.notices.add("warning", "no-section", "DXF のセクションが見つかりませんでした");
    this.finish();
    this.doc.notices = this.notices.toArray();
    return this.doc;
  }

  private skipSection(): void {
    while (this.i < this.length && !this.is(this.i, 0, "ENDSEC")) {
      if (this.is(this.i, 0, "SECTION") || this.is(this.i, 0, "EOF")) return;
      this.i++;
    }
    this.i++;
  }

  // --- HEADER ---------------------------------------------------------------

  private header(): void {
    const variables = this.doc.header.variables;
    while (this.i < this.length && !this.is(this.i, 0, "ENDSEC")) {
      if (this.code(this.i) !== 9) {
        if (this.is(this.i, 0, "SECTION")) return;
        this.i++;
        continue;
      }
      const name = this.raw(this.i).trim();
      this.i++;
      const values: { code: number; value: string | number }[] = [];
      while (this.i < this.length && this.code(this.i) !== 9 && this.code(this.i) !== 0) {
        values.push({ code: this.code(this.i), value: this.values[this.i] as string | number });
        this.i++;
      }
      const first = values[0];
      if (!first) continue;
      if (first.code >= 10 && first.code <= 39 && values.length > 1) {
        variables[name] = values.map((entry) => (typeof entry.value === "number" ? entry.value : Number(entry.value)));
      } else {
        variables[name] = typeof first.value === "string" ? decodeEscapes(first.value) : first.value;
      }
    }
    this.i++;
    const header = this.doc.header;
    const number = (name: string, fallback: number): number => {
      const value = variables[name];
      return typeof value === "number" && Number.isFinite(value) ? value : fallback;
    };
    const point = <T extends Vec2 | Vec3>(name: string, size: 2 | 3): T | undefined => {
      const value = variables[name];
      if (!Array.isArray(value)) return undefined;
      const out = [value[0] ?? 0, value[1] ?? 0, value[2] ?? 0].slice(0, size);
      return out as T;
    };
    header.insunits = number("$INSUNITS", 0);
    const measurement = variables.$MEASUREMENT;
    if (typeof measurement === "number") header.measurement = measurement;
    header.ltscale = number("$LTSCALE", 1);
    header.psltscale = number("$PSLTSCALE", 1);
    const textsize = variables.$TEXTSIZE;
    if (typeof textsize === "number") header.textsize = textsize;
    header.dimscale = number("$DIMSCALE", 1);
    const metric = header.measurement === 1;
    header.dimasz = number("$DIMASZ", metric ? 2.5 : 0.18);
    header.dimtxt = number("$DIMTXT", metric ? 2.5 : 0.18);
    header.tilemode = number("$TILEMODE", 1);
    header.extmin = point<Vec3>("$EXTMIN", 3);
    header.extmax = point<Vec3>("$EXTMAX", 3);
    header.limmin = point<Vec2>("$LIMMIN", 2);
    header.limmax = point<Vec2>("$LIMMAX", 2);
    header.plimmin = point<Vec2>("$PLIMMIN", 2);
    header.plimmax = point<Vec2>("$PLIMMAX", 2);
  }

  // --- TABLES ---------------------------------------------------------------

  private tables(): void {
    while (this.i < this.length && !this.is(this.i, 0, "ENDSEC")) {
      if (this.is(this.i, 0, "SECTION") || this.is(this.i, 0, "EOF")) return;
      if (this.code(this.i) !== 0) {
        this.i++;
        continue;
      }
      const type = this.raw(this.i).trim().toUpperCase();
      const end = this.nextZero(this.i);
      const body = this.body(this.i + 1, end);
      switch (type) {
        case "LAYER":
          this.doc.layers.push(this.layer(body, this.i + 1, end));
          break;
        case "LTYPE":
          this.doc.linetypes.push(this.linetype(body));
          break;
        case "STYLE":
          this.doc.styles.push(this.style(this.i + 1, end, body));
          break;
        case "DIMSTYLE":
          this.doc.dimstyles.push(this.dimstyle(this.i + 1, end));
          break;
        case "BLOCK_RECORD": {
          let handle = "";
          let name = "";
          for (const k of body) {
            if (this.code(k) === 5) handle = this.raw(k).trim().toUpperCase();
            else if (this.code(k) === 2) name = this.str(k);
          }
          if (handle && name) this.blockRecords.set(handle, name);
          break;
        }
      }
      this.i = end;
    }
    this.i++;
  }

  private handleOf(body: Body, code = 5): string {
    for (const k of body) if (this.code(k) === code) return this.raw(k).trim().toUpperCase();
    return "";
  }

  private layer(body: Body, start: number, end: number): DxfLayer {
    const layer: DxfLayer = {
      handle: this.handleOf(body),
      name: "",
      color_index: 7,
      frozen: false,
      off: false,
      locked: false,
      plot: true,
      linetype: "CONTINUOUS",
      lineweight: -3,
    };
    for (const k of body) {
      switch (this.code(k)) {
        case 2:
          layer.name = this.str(k);
          break;
        case 70: {
          const flags = this.num(k);
          layer.frozen = (flags & 1) !== 0;
          layer.locked = (flags & 4) !== 0;
          break;
        }
        case 62: {
          // 色が負のレイヤは非表示（DXF Reference「LAYER」）
          const color = this.num(k);
          layer.off = color < 0;
          layer.color_index = Math.abs(color) || 7;
          break;
        }
        case 420:
          layer.true_color = this.num(k) & 0xffffff;
          break;
        case 6:
          layer.linetype = this.str(k);
          break;
        case 290:
          layer.plot = this.num(k) !== 0;
          break;
        case 370:
          layer.lineweight = this.num(k);
          break;
      }
    }
    // レイヤの透過度は拡張データ AcCmTransparency の 1071 に入る
    for (let k = start; k < end; k++) {
      if (this.code(k) === 1001 && this.raw(k).trim() === "AcCmTransparency") {
        if (k + 1 < end && this.code(k + 1) === 1071) layer.transparency = this.num(k + 1);
        break;
      }
    }
    return layer;
  }

  private linetype(body: Body): DxfLinetype {
    const linetype: DxfLinetype = {
      handle: this.handleOf(body),
      name: "",
      description: "",
      pattern_length: 0,
      dashes: [],
      complex: false,
    };
    for (const k of body) {
      switch (this.code(k)) {
        case 2:
          linetype.name = this.str(k);
          break;
        case 3:
          linetype.description = this.str(k);
          break;
        case 40:
          linetype.pattern_length = this.num(k);
          break;
        case 49:
          linetype.dashes.push(this.num(k));
          break;
        case 74:
          if (this.num(k) !== 0) linetype.complex = true;
          break;
      }
    }
    return linetype;
  }

  private style(start: number, end: number, body: Body): DxfTextStyle {
    const style: DxfTextStyle = {
      handle: this.handleOf(body),
      name: "",
      font: "",
      bigfont: "",
      height: 0,
      width: 1,
      oblique: 0,
      flags: 0,
      generation_flags: 0,
    };
    for (const k of body) {
      switch (this.code(k)) {
        case 2:
          style.name = this.str(k);
          break;
        case 3:
          style.font = this.str(k);
          break;
        case 4:
          style.bigfont = this.str(k);
          break;
        case 40:
          style.height = this.num(k);
          break;
        case 41:
          style.width = this.num(k) || 1;
          break;
        case 50:
          style.oblique = this.num(k);
          break;
        case 70:
          style.flags = this.num(k);
          break;
        case 71:
          style.generation_flags = this.num(k);
          break;
      }
    }
    // TrueType のフォント名は拡張データ ACAD の 1000 に入る
    for (let k = start; k < end; k++) {
      if (this.code(k) === 1001 && this.raw(k).trim().toUpperCase() === "ACAD") {
        if (k + 1 < end && this.code(k + 1) === 1000) style.font_family = this.str(k + 1);
        break;
      }
    }
    return style;
  }

  private dimstyle(start: number, end: number): DxfDimStyle {
    const style: DxfDimStyle = { handle: "", name: "", dimscale: 1, dimasz: 0, dimtxt: 0 };
    for (let k = start; k < end; k++) {
      switch (this.code(k)) {
        case 105:
          style.handle = this.raw(k).trim().toUpperCase();
          break;
        case 2:
          style.name = this.str(k);
          break;
        case 40:
          style.dimscale = this.num(k);
          break;
        case 41:
          style.dimasz = this.num(k);
          break;
        case 140:
          style.dimtxt = this.num(k);
          break;
      }
    }
    return style;
  }

  // --- BLOCKS ---------------------------------------------------------------

  private blocks(): void {
    while (this.i < this.length && !this.is(this.i, 0, "ENDSEC")) {
      if (this.is(this.i, 0, "SECTION") || this.is(this.i, 0, "EOF")) return;
      if (!this.is(this.i, 0, "BLOCK")) {
        this.i++;
        continue;
      }
      const end = this.nextZero(this.i);
      const body = this.body(this.i + 1, end);
      const block: DxfBlock = {
        handle: this.handleOf(body),
        name: "",
        base_point: [0, 0, 0],
        flags: 0,
        anonymous: false,
        layout: false,
      };
      for (const k of body) {
        switch (this.code(k)) {
          case 2:
            block.name = this.str(k);
            break;
          case 70:
            block.flags = this.num(k);
            break;
          case 10:
            block.base_point[0] = this.num(k);
            break;
          case 20:
            block.base_point[1] = this.num(k);
            break;
          case 30:
            block.base_point[2] = this.num(k);
            break;
          case 1:
            if (this.raw(k)) block.xref_path = this.str(k);
            break;
          case 4:
            if (this.raw(k)) block.description = this.str(k);
            break;
          case 330:
            block.block_record_handle = this.raw(k).trim().toUpperCase();
            break;
        }
      }
      if (!block.name) {
        for (const k of body) if (this.code(k) === 3) block.name = this.str(k);
      }
      block.anonymous = (block.flags & 1) !== 0 || block.name.startsWith("*");
      block.layout = isModelBlock(block.name) || isPaperBlock(block.name);
      this.doc.blocks.push(block);
      this.i = end;
      const owner: Owner = isModelBlock(block.name)
        ? { space: "model" }
        : { space: isPaperBlock(block.name) ? "paper" : "block", block: block.name };
      this.entityList(owner, "ENDBLK");
      if (this.is(this.i, 0, "ENDBLK")) this.i = this.nextZero(this.i);
    }
    this.i++;
  }

  // --- ENTITIES -------------------------------------------------------------

  private entitiesSection(): void {
    this.entityList(null, "ENDSEC");
    this.i++;
  }

  /** 図形を順に読む。owner が null なら ENTITIES セクション（67 と 330 でモデル・紙空間を決める） */
  private entityList(owner: Owner | null, terminator: string): void {
    while (this.i < this.length) {
      if (this.code(this.i) !== 0) {
        this.i++;
        continue;
      }
      const type = this.raw(this.i).trim().toUpperCase();
      if (type === terminator || type === "ENDSEC" || type === "EOF" || type === "SECTION") return;
      const start = this.i;
      const end = this.nextZero(start);
      this.i = end;
      if (type === "SEQEND" || type === "VERTEX" || type === "ATTRIB") {
        // POLYLINE・INSERT に続かない頂点・属性
        if (type === "ATTRIB") this.push(this.attribute(start, end, "ATTRIB", owner));
        continue;
      }
      if (!READ_TYPES.has(type)) {
        this.doc.skipped[type] = (this.doc.skipped[type] ?? 0) + 1;
        continue;
      }
      try {
        this.push(this.entity(type, start, end, owner));
      } catch (error) {
        this.notices.add(
          "warning",
          "bad-entity",
          `読めない図形を飛ばしました（${error instanceof Error ? error.message : String(error)}）`,
        );
      }
    }
  }

  private push(entity: DxfEntity | null): void {
    if (!entity) return;
    if (entity.space === "paper") this.hasPaperEntities = true;
    this.doc.entities.push(entity);
  }

  /** 102 の組（{ACAD_REACTORS など）、拡張データ、埋め込みの図形を除いたタグの位置 */
  private body(start: number, end: number): Body {
    const out: Body = [];
    for (let k = start; k < end; k++) {
      const code = this.code(k);
      if (code === 102) {
        if (this.raw(k).trim().startsWith("{")) {
          while (k + 1 < end && !(this.code(k + 1) === 102 && this.raw(k + 1).trim() === "}")) k++;
          k++;
        }
        continue;
      }
      if (code >= 1000 || code === 101) break;
      out.push(k);
    }
    return out;
  }

  /**
   * 共通の属性。R13 以降は AcDbEntity の中の値だけを使う（ACAD_TABLE のセルの色などと取り違えない）。
   * 厚さ（39）と押し出し方向（210）は図形の種類の側にあるので、全体から読む。
   */
  private base<T extends string>(type: T, body: Body, owner: Owner | null): DxfEntityBase & { type: T } {
    const entity: DxfEntityBase & { type: T } = {
      type,
      handle: "",
      space: owner?.space ?? "model",
      layer: "0",
      linetype: "BYLAYER",
      color_index: 256,
      lineweight: -1,
      linetype_scale: 1,
      invisible: false,
    };
    if (owner?.block) entity.block = owner.block;
    let subclass = "";
    let marked = false;
    let paper = false;
    let extrusion: Vec3 | undefined;
    for (const k of body) {
      const code = this.code(k);
      if (code === 100) {
        subclass = this.raw(k).trim();
        marked = true;
        continue;
      }
      if (code === 5 && !entity.handle) {
        entity.handle = this.raw(k).trim().toUpperCase();
        continue;
      }
      if (code === 330 && entity.owner === undefined && !marked) {
        entity.owner = this.raw(k).trim().toUpperCase();
        continue;
      }
      if (code === 39) {
        entity.thickness = this.num(k);
        continue;
      }
      if (code === 210 || code === 220 || code === 230) {
        extrusion ??= [0, 0, 1];
        extrusion[code === 210 ? 0 : code === 220 ? 1 : 2] = this.num(k);
        continue;
      }
      if (marked && subclass !== "AcDbEntity") continue;
      switch (code) {
        case 8:
          entity.layer = this.str(k);
          break;
        case 6:
          entity.linetype = this.str(k);
          break;
        case 62:
          entity.color_index = this.num(k);
          break;
        case 420:
          entity.true_color = this.num(k) & 0xffffff;
          break;
        case 440:
          entity.transparency = this.num(k);
          break;
        case 370:
          entity.lineweight = this.num(k);
          break;
        case 48:
          entity.linetype_scale = this.num(k) || 1;
          break;
        case 60:
          entity.invisible = this.num(k) !== 0;
          break;
        case 67:
          paper = this.num(k) === 1;
          break;
      }
    }
    if (extrusion && !(extrusion[0] === 0 && extrusion[1] === 0 && extrusion[2] === 1)) entity.extrusion = extrusion;
    if (!owner) {
      // ENTITIES セクションはモデル空間と、現在のレイアウトの紙空間（*Paper_Space）を持つ
      const ownerName = entity.owner ? this.blockRecords.get(entity.owner) : undefined;
      if (paper || (ownerName && isPaperBlock(ownerName))) {
        entity.space = "paper";
        entity.block = ownerName && isPaperBlock(ownerName) ? ownerName : "*Paper_Space";
      }
    }
    return entity;
  }

  private point(body: Body, x: number, size: 2 | 3 = 3): Vec3 {
    const out: Vec3 = [0, 0, 0];
    for (const k of body) {
      const code = this.code(k);
      if (code === x) out[0] = this.num(k);
      else if (code === x + 10) out[1] = this.num(k);
      else if (size === 3 && code === x + 20) out[2] = this.num(k);
    }
    return out;
  }

  private has(body: Body, code: number): boolean {
    for (const k of body) if (this.code(k) === code) return true;
    return false;
  }

  private number(body: Body, code: number, fallback: number): number {
    for (const k of body) if (this.code(k) === code) return this.num(k);
    return fallback;
  }

  private text(body: Body, code: number, fallback = ""): string {
    for (const k of body) if (this.code(k) === code) return this.str(k);
    return fallback;
  }

  private entity(type: string, start: number, end: number, owner: Owner | null): DxfEntity | null {
    const body = this.body(start + 1, end);
    switch (type) {
      case "LINE":
        return { ...this.base("LINE", body, owner), start: this.point(body, 10), end: this.point(body, 11) };
      case "POINT":
        return {
          ...this.base("POINT", body, owner),
          location: this.point(body, 10),
          x_axis_angle: this.number(body, 50, 0),
        };
      case "CIRCLE":
        return {
          ...this.base("CIRCLE", body, owner),
          center: this.point(body, 10),
          radius: this.number(body, 40, 0),
        };
      case "ARC":
        return {
          ...this.base("ARC", body, owner),
          center: this.point(body, 10),
          radius: this.number(body, 40, 0),
          start_angle: this.number(body, 50, 0),
          end_angle: this.number(body, 51, 360),
        };
      case "ELLIPSE":
        return {
          ...this.base("ELLIPSE", body, owner),
          center: this.point(body, 10),
          major_axis: this.point(body, 11),
          ratio: this.number(body, 40, 1),
          start_param: this.number(body, 41, 0),
          end_param: this.number(body, 42, Math.PI * 2),
        };
      case "LWPOLYLINE":
        return this.lwpolyline(body, owner);
      case "POLYLINE":
        return this.polyline(body, owner);
      case "SPLINE":
      case "HELIX":
        return this.spline(type, body, owner);
      case "TEXT":
        return { ...this.base("TEXT", body, owner), ...this.textBase(body, 73) };
      case "ATTDEF":
        return this.attribute(start, end, "ATTDEF", owner);
      case "MTEXT":
        return this.mtext(body, owner);
      case "INSERT":
        return this.insert(body, owner);
      case "DIMENSION":
      case "ARC_DIMENSION":
      case "LARGE_RADIAL_DIMENSION":
        return this.dimension(type, body, owner);
      case "HATCH":
      case "MPOLYGON":
        return this.hatch(type, body, owner);
      case "SOLID":
      case "TRACE":
      case "3DFACE": {
        const vtx2 = this.point(body, 12);
        const entity = {
          ...this.base(type, body, owner),
          vtx0: this.point(body, 10),
          vtx1: this.point(body, 11),
          vtx2,
          vtx3: this.has(body, 13) ? this.point(body, 13) : ([...vtx2] as Vec3),
        };
        return type === "3DFACE" ? { ...entity, invisible_edges: this.number(body, 70, 0) } : entity;
      }
      case "LEADER": {
        const points: Vec3[] = [];
        for (const k of body) {
          const code = this.code(k);
          if (code === 10) points.push([this.num(k), 0, 0]);
          else if (code === 20 && points.length > 0) (points[points.length - 1] as Vec3)[1] = this.num(k);
          else if (code === 30 && points.length > 0) (points[points.length - 1] as Vec3)[2] = this.num(k);
        }
        return {
          ...this.base("LEADER", body, owner),
          points,
          path_type: this.number(body, 72, 0),
          annotation_type: this.number(body, 73, 3),
          has_arrowhead: this.number(body, 71, 1) !== 0,
          dimstyle: this.text(body, 3, "STANDARD"),
        };
      }
      case "MULTILEADER":
      case "MLEADER":
        return this.mleader(type, body, owner);
      case "MLINE":
        return this.mline(body, owner);
      case "VIEWPORT":
        return this.viewport(start, end, body, owner);
      case "XLINE":
      case "RAY":
        return { ...this.base(type, body, owner), start: this.point(body, 10), unit_vector: this.point(body, 11) };
      case "IMAGE":
      case "WIPEOUT":
        return this.image(type, body, owner);
      case "ACAD_TABLE": {
        // 表は AcDbBlockReference の部分（ブロックの名前と位置）だけを読む
        const reference = this.subclassBody(body, "AcDbBlockReference");
        return {
          ...this.base("ACAD_TABLE", body, owner),
          name: this.text(reference, 2),
          insert: this.point(reference, 10),
          xscale: this.number(reference, 41, 1),
          yscale: this.number(reference, 42, 1),
          zscale: this.number(reference, 43, 1),
          rotation: this.number(reference, 50, 0),
        };
      }
    }
    return null;
  }

  /** 指定のサブクラスの中のタグ。サブクラスの印がない図面（R12）は全体 */
  private subclassBody(body: Body, name: string): Body {
    let current = "";
    let marked = false;
    const out: Body = [];
    for (const k of body) {
      if (this.code(k) === 100) {
        current = this.raw(k).trim();
        marked = true;
        continue;
      }
      if (current === name) out.push(k);
    }
    return marked ? out : body;
  }

  private textBase(body: Body, valignCode: 73 | 74): DxfTextBase {
    const text: DxfTextBase = {
      text: this.text(body, 1),
      insert: this.point(body, 10),
      height: this.number(body, 40, 0),
      rotation: this.number(body, 50, 0),
      oblique: this.number(body, 51, 0),
      width: this.number(body, 41, 1) || 1,
      style: this.text(body, 7, "STANDARD"),
      halign: this.number(body, 72, 0),
      valign: this.number(body, valignCode, 0),
      text_generation_flag: this.number(body, 71, 0),
    };
    if (this.has(body, 11)) text.align_point = this.point(body, 11);
    return text;
  }

  private attribute<T extends "ATTRIB" | "ATTDEF">(
    start: number,
    end: number,
    type: T,
    owner: Owner | null,
  ): T extends "ATTRIB" ? DxfAttrib : DxfAttdef {
    const body = this.body(start + 1, end);
    // ATTRIB・ATTDEF の縦の揃えは AcDbAttribute の 74（73 は文字数の上限）
    const base = { ...this.base(type, body, owner), ...this.textBase(body, 74) };
    const tag = this.text(body, 2);
    const flags = this.number(body, 70, 0);
    const entity =
      type === "ATTDEF"
        ? { ...base, type: "ATTDEF" as const, tag, prompt: this.text(body, 3), attribute_flags: flags }
        : { ...base, type: "ATTRIB" as const, tag, attribute_flags: flags };
    return entity as T extends "ATTRIB" ? DxfAttrib : DxfAttdef;
  }

  private lwpolyline(body: Body, owner: Owner | null): DxfEntity {
    const points: Vec3[] = [];
    const bulges: number[] = [];
    const widths: Vec2[] = [];
    let flags = 0;
    let constWidth: number | undefined;
    let elevation: number | undefined;
    for (const k of body) {
      const code = this.code(k);
      const last = points.length - 1;
      switch (code) {
        case 70:
          flags = this.num(k);
          break;
        case 43:
          constWidth = this.num(k);
          break;
        case 38:
          elevation = this.num(k);
          break;
        case 10:
          points.push([this.num(k), 0, 0]);
          bulges.push(0);
          widths.push([0, 0]);
          break;
        case 20:
          if (last >= 0) (points[last] as Vec3)[1] = this.num(k);
          break;
        case 40:
          if (last >= 0) (widths[last] as Vec2)[0] = this.num(k);
          break;
        case 41:
          if (last >= 0) (widths[last] as Vec2)[1] = this.num(k);
          break;
        case 42:
          if (last >= 0) bulges[last] = this.num(k);
          break;
      }
    }
    if (elevation !== undefined) for (const point of points) point[2] = elevation;
    const entity: DxfEntity = {
      ...this.base("LWPOLYLINE", body, owner),
      points,
      bulges,
      widths,
      closed: (flags & 1) !== 0,
      flags,
    };
    if (constWidth !== undefined && constWidth !== 0) entity.const_width = constWidth;
    if (elevation !== undefined) entity.elevation = elevation;
    return entity;
  }

  /** POLYLINE に続く VERTEX を SEQEND まで読む（DXF Reference「POLYLINE」「VERTEX」） */
  private polyline(body: Body, owner: Owner | null): DxfPolyline {
    const flags = this.number(body, 70, 0);
    const elevation = this.number(body, 30, 0);
    const defaultStart = this.number(body, 40, 0);
    const defaultEnd = this.number(body, 41, 0);
    const polyline: DxfPolyline = {
      ...this.base("POLYLINE", body, owner),
      points: [],
      bulges: [],
      widths: [],
      vertex_flags: [],
      closed: (flags & 1) !== 0,
      flags,
    };
    if (flags & 16) {
      polyline.m_count = this.number(body, 71, 0);
      polyline.n_count = this.number(body, 72, 0);
    }
    const faces: number[][] = [];
    const is3d = (flags & (8 | 16 | 64)) !== 0;
    while (this.i < this.length && this.is(this.i, 0, "VERTEX")) {
      const start = this.i;
      const end = this.nextZero(start);
      this.i = end;
      const vertex = this.body(start + 1, end);
      const vertexFlags = this.number(vertex, 70, 0);
      if (flags & 64 && (vertexFlags & 128) !== 0 && (vertexFlags & 64) === 0) {
        // ポリフェースメッシュの面の頂点の番号（71〜74）
        const face: number[] = [];
        for (const code of [71, 72, 73, 74]) {
          const index = this.number(vertex, code, 0);
          if (index !== 0) face.push(index);
        }
        faces.push(face);
        continue;
      }
      const point = this.point(vertex, 10);
      if (!is3d) point[2] = point[2] || elevation;
      polyline.points.push(point);
      polyline.bulges.push(this.number(vertex, 42, 0));
      polyline.widths.push([this.number(vertex, 40, defaultStart), this.number(vertex, 41, defaultEnd)]);
      polyline.vertex_flags.push(vertexFlags);
    }
    if (this.is(this.i, 0, "SEQEND")) this.i = this.nextZero(this.i);
    if (faces.length > 0) polyline.faces = faces;
    return polyline;
  }

  private spline(type: "SPLINE" | "HELIX", body: Body, owner: Owner | null): DxfSpline {
    const own = type === "HELIX" ? this.subclassBody(body, "AcDbSpline") : body;
    const flags = this.number(own, 70, 0);
    const spline: DxfSpline = {
      ...this.base(type, body, owner),
      degree: this.number(own, 71, 3),
      flags,
      closed: (flags & 1) !== 0,
      periodic: (flags & 2) !== 0,
      rational: (flags & 4) !== 0,
      knots: [],
      control_points: [],
      weights: [],
      fit_points: [],
    };
    for (const k of own) {
      const code = this.code(k);
      switch (code) {
        case 40:
          spline.knots.push(this.num(k));
          break;
        case 41:
          spline.weights.push(this.num(k));
          break;
        case 10:
          spline.control_points.push([this.num(k), 0, 0]);
          break;
        case 20:
        case 30: {
          const last = spline.control_points[spline.control_points.length - 1];
          if (last) last[code === 20 ? 1 : 2] = this.num(k);
          break;
        }
        case 11:
          spline.fit_points.push([this.num(k), 0, 0]);
          break;
        case 21:
        case 31: {
          const last = spline.fit_points[spline.fit_points.length - 1];
          if (last) last[code === 21 ? 1 : 2] = this.num(k);
          break;
        }
      }
    }
    if (this.has(own, 12)) spline.start_tangent = this.point(own, 12);
    if (this.has(own, 13)) spline.end_tangent = this.point(own, 13);
    return spline;
  }

  private mtext(body: Body, owner: Owner | null): DxfMText {
    // 250 文字を超える文字列は 3 に分けて書き、最後の 1 が残り（DXF Reference「MTEXT」）
    let raw = "";
    let tail = "";
    for (const k of body) {
      if (this.code(k) === 3) raw += this.raw(k);
      else if (this.code(k) === 1) tail = this.raw(k);
    }
    raw += tail;
    const entity: DxfMText = {
      ...this.base("MTEXT", body, owner),
      text: mtextPlainText(raw),
      raw_text: raw,
      insert: this.point(body, 10),
      char_height: this.number(body, 40, 0),
      rect_width: this.number(body, 41, 0),
      attachment_point: this.number(body, 71, 1),
      drawing_direction: this.number(body, 72, 1),
      rotation: this.number(body, 50, 0),
      style: this.text(body, 7, "STANDARD"),
      line_spacing_style: this.number(body, 73, 1),
      line_spacing_factor: this.number(body, 44, 1),
      background_flags: this.number(body, 90, 0),
    };
    if (this.has(body, 11)) entity.text_direction = this.point(body, 11);
    if (this.has(body, 63)) entity.background_color_index = this.number(body, 63, 0);
    if (this.has(body, 421)) entity.background_true_color = this.number(body, 421, 0) & 0xffffff;
    if (this.has(body, 45)) entity.background_scale = this.number(body, 45, 1.5);
    return entity;
  }

  /** INSERT と、66 が 1 なら続く ATTRIB を SEQEND まで読む */
  private insert(body: Body, owner: Owner | null): DxfInsert {
    const insert: DxfInsert = {
      ...this.base("INSERT", body, owner),
      name: this.text(body, 2),
      insert: this.point(body, 10),
      xscale: this.number(body, 41, 1),
      yscale: this.number(body, 42, 1),
      zscale: this.number(body, 43, 1),
      rotation: this.number(body, 50, 0),
      column_count: Math.max(1, this.number(body, 70, 1)),
      row_count: Math.max(1, this.number(body, 71, 1)),
      column_spacing: this.number(body, 44, 0),
      row_spacing: this.number(body, 45, 0),
      attribs: [],
    };
    if (this.number(body, 66, 0) === 1) {
      while (this.i < this.length && this.is(this.i, 0, "ATTRIB")) {
        const start = this.i;
        const end = this.nextZero(start);
        this.i = end;
        // 属性は配置と同じ空間・ブロックに属する
        insert.attribs.push(this.attribute(start, end, "ATTRIB", { space: insert.space, block: insert.block }));
      }
      if (this.is(this.i, 0, "SEQEND")) this.i = this.nextZero(this.i);
    }
    return insert;
  }

  private dimension(type: DxfDimension["type"], body: Body, owner: Owner | null): DxfDimension {
    const entity: DxfDimension = {
      ...this.base(type, body, owner),
      anonymous_block_name: this.text(body, 2),
      dimtype: this.number(body, 70, 0),
      defpoint: this.point(body, 10),
      text_midpoint: this.point(body, 11),
      text: this.text(body, 1),
      dimstyle: this.text(body, 3, "STANDARD"),
    };
    if (this.has(body, 12)) entity.insert = this.point(body, 12);
    if (this.has(body, 13)) entity.defpoint2 = this.point(body, 13);
    if (this.has(body, 14)) entity.defpoint3 = this.point(body, 14);
    if (this.has(body, 15)) entity.defpoint4 = this.point(body, 15);
    if (this.has(body, 16)) entity.defpoint5 = this.point(body, 16);
    if (this.has(body, 42)) entity.actual_measurement = this.number(body, 42, 0);
    if (this.has(body, 71)) entity.attachment_point = this.number(body, 71, 5);
    if (this.has(body, 53)) entity.text_rotation = this.number(body, 53, 0);
    if (this.has(body, 51)) entity.horizontal_direction = this.number(body, 51, 0);
    if (this.has(body, 50)) entity.angle = this.number(body, 50, 0);
    if (this.has(body, 52)) entity.oblique_angle = this.number(body, 52, 0);
    return entity;
  }

  /** HATCH は境界・模様の値を決まった順に書く（DXF Reference「HATCH」「Boundary Path Data」「Pattern Data」） */
  private hatch(type: "HATCH" | "MPOLYGON", body: Body, owner: Owner | null): DxfHatch {
    const own = this.subclassBody(body, type === "HATCH" ? "AcDbHatch" : "AcDbMPolygon");
    const hatch: DxfHatch = {
      ...this.base(type, body, owner),
      pattern_name: "",
      solid_fill: false,
      associative: false,
      hatch_style: 0,
      pattern_type: 1,
      pattern_angle: 0,
      pattern_scale: 1,
      pattern_double: false,
      elevation: 0,
      paths: [],
      pattern_lines: [],
    };
    let k = 0;
    const at = (offset = 0) => own[k + offset] as number;
    const peek = () => (k < own.length ? this.code(at()) : -1);
    const take = () => {
      const index = at();
      k++;
      return index;
    };
    // 境界の前: 標高（10・20・30）、名前（2）、塗りつぶし（70）、関連付け（71）、境界の数（91）
    let paths = 0;
    while (k < own.length) {
      const code = peek();
      if (code === 91) {
        paths = this.num(take());
        break;
      }
      const index = take();
      // MPOLYGON は 70 が版、71 が塗りつぶし（DXF Reference「HATCH」の MPolygon の注記）
      if (code === 30) hatch.elevation = this.num(index);
      else if (code === 2) hatch.pattern_name = this.str(index);
      else if (code === 70 && type === "HATCH") hatch.solid_fill = this.num(index) === 1;
      else if (code === 71 && type === "HATCH") hatch.associative = this.num(index) === 1;
      else if (code === 71) hatch.solid_fill = this.num(index) === 1;
    }
    for (let p = 0; p < paths && k < own.length; p++) {
      while (k < own.length && peek() !== 92) k++;
      if (k >= own.length) break;
      const flags = this.num(take());
      const path: DxfHatchPath = { flags, closed: true, points: [] };
      if (flags & 2) {
        let hasBulge = false;
        let count = 0;
        while (k < own.length && peek() !== 93) {
          const index = take();
          if (this.code(index) === 72) hasBulge = this.num(index) !== 0;
          else if (this.code(index) === 73) path.closed = this.num(index) !== 0;
        }
        if (k < own.length) count = this.num(take());
        const bulges: number[] = [];
        for (let v = 0; v < count && k < own.length; v++) {
          if (peek() !== 10) break;
          const x = this.num(take());
          const y = peek() === 20 ? this.num(take()) : 0;
          let bulge = 0;
          if (hasBulge && peek() === 42) bulge = this.num(take());
          path.points.push([x, y, hatch.elevation]);
          bulges.push(bulge);
        }
        if (bulges.some((value) => value !== 0)) path.bulges = bulges;
      } else {
        while (k < own.length && peek() !== 93) k++;
        const count = k < own.length ? this.num(take()) : 0;
        const edges: DxfHatchEdge[] = [];
        for (let e = 0; e < count && k < own.length; e++) {
          if (peek() !== 72) break;
          const edge = this.hatchEdge(this.num(take()), own, () => peek(), take);
          if (edge) edges.push(edge);
        }
        path.edges = edges;
      }
      // 境界の元の図形への参照（97・330）
      if (peek() === 97) {
        const sources = this.num(take());
        for (let s = 0; s < sources && peek() === 330; s++) take();
      }
      hatch.paths.push(path);
    }
    // 境界の後: 種類（75・76）、角度・尺度（52・41）、二重（77）、模様の線（78 と 53・43〜46・79・49）
    while (k < own.length) {
      const code = peek();
      if (code === 78) {
        const count = this.num(take());
        for (let l = 0; l < count && k < own.length; l++) {
          while (k < own.length && peek() !== 53) {
            if (peek() === 47 || peek() === 98 || peek() === 450) break;
            k++;
          }
          if (peek() !== 53) break;
          const line: DxfPatternLine = { angle: this.num(take()), base: [0, 0], offset: [0, 0], dashes: [] };
          while (k < own.length) {
            const lineCode = peek();
            if (lineCode === 43) line.base[0] = this.num(take());
            else if (lineCode === 44) line.base[1] = this.num(take());
            else if (lineCode === 45) line.offset[0] = this.num(take());
            else if (lineCode === 46) line.offset[1] = this.num(take());
            else if (lineCode === 79) take();
            else if (lineCode === 49) line.dashes.push(this.num(take()));
            else break;
          }
          hatch.pattern_lines.push(line);
        }
        continue;
      }
      const index = take();
      switch (code) {
        case 75:
          hatch.hatch_style = this.num(index);
          break;
        case 76:
          hatch.pattern_type = this.num(index);
          break;
        case 52:
          hatch.pattern_angle = this.num(index);
          break;
        case 41:
          hatch.pattern_scale = this.num(index);
          break;
        case 77:
          hatch.pattern_double = this.num(index) !== 0;
          break;
        case 450:
          if (this.num(index) !== 0) hatch.gradient = "";
          break;
        case 421:
          if (hatch.gradient !== undefined) {
            hatch.gradient_colors ??= [];
            hatch.gradient_colors.push(this.num(index) & 0xffffff);
          }
          break;
        case 470:
          if (hatch.gradient !== undefined) hatch.gradient = this.str(index);
          break;
      }
    }
    return hatch;
  }

  private hatchEdge(kind: number, own: Body, peek: () => number, take: () => number): DxfHatchEdge | null {
    const read = (codes: readonly number[]): Map<number, number> => {
      const values = new Map<number, number>();
      while (codes.includes(peek())) {
        const index = take();
        values.set(this.code(index), this.num(index));
      }
      return values;
    };
    void own;
    switch (kind) {
      case 1: {
        const v = read([10, 20, 11, 21]);
        return { kind: "line", start: [v.get(10) ?? 0, v.get(20) ?? 0], end: [v.get(11) ?? 0, v.get(21) ?? 0] };
      }
      case 2: {
        const v = read([10, 20, 40, 50, 51, 73]);
        return {
          kind: "arc",
          center: [v.get(10) ?? 0, v.get(20) ?? 0],
          radius: v.get(40) ?? 0,
          start_angle: v.get(50) ?? 0,
          end_angle: v.get(51) ?? 360,
          ccw: (v.get(73) ?? 1) !== 0,
        };
      }
      case 3: {
        const v = read([10, 20, 11, 21, 40, 50, 51, 73]);
        return {
          kind: "ellipse",
          center: [v.get(10) ?? 0, v.get(20) ?? 0],
          major_axis: [v.get(11) ?? 1, v.get(21) ?? 0],
          ratio: v.get(40) ?? 1,
          start_angle: v.get(50) ?? 0,
          end_angle: v.get(51) ?? 360,
          ccw: (v.get(73) ?? 1) !== 0,
        };
      }
      case 4: {
        const edge: DxfHatchEdge = {
          kind: "spline",
          degree: 3,
          rational: false,
          periodic: false,
          knots: [],
          control_points: [],
          weights: [],
          fit_points: [],
        };
        let knots = 0;
        let controls = 0;
        for (;;) {
          const code = peek();
          if (code === 94) edge.degree = this.num(take());
          else if (code === 73) edge.rational = this.num(take()) !== 0;
          else if (code === 74) edge.periodic = this.num(take()) !== 0;
          else if (code === 95) knots = this.num(take());
          else if (code === 96) controls = this.num(take());
          else break;
        }
        for (let n = 0; n < knots && peek() === 40; n++) edge.knots?.push(this.num(take()));
        for (let n = 0; n < controls && peek() === 10; n++) {
          const x = this.num(take());
          const y = peek() === 20 ? this.num(take()) : 0;
          edge.control_points?.push([x, y]);
          if (peek() === 42) edge.weights?.push(this.num(take()));
        }
        // 当てはめ点（97・11・21）と接線（12・13）は R2010 以降
        if (peek() === 97) {
          const fits = this.num(take());
          for (let n = 0; n < fits && peek() === 11; n++) {
            const x = this.num(take());
            const y = peek() === 21 ? this.num(take()) : 0;
            edge.fit_points?.push([x, y]);
          }
          while ([12, 22, 13, 23].includes(peek())) take();
        }
        return edge;
      }
    }
    return null;
  }

  /**
   * MULTILEADER は、CONTEXT_DATA{（300〜301）・LEADER{（302〜303）・LEADER_LINE{（304〜305）の区切りの中と外で
   * 同じグループコードを別の意味に使う（DXF Reference「Common MLeader Group Codes」「MLeader Context Data Group Codes」
   * 「MLeader Leader Node Group Codes」「MLeader Leader Line Group Codes」）。区切りを追って順に読む
   */
  private mleader(type: DxfMLeader["type"], body: Body, owner: Owner | null): DxfMLeader {
    const context: DxfMLeaderContext = {
      scale: 1,
      base_point: [0, 0, 0],
      char_height: 0,
      arrow_head_size: 0,
      landing_gap_size: 0,
      leaders: [],
    };
    const entity: DxfMLeader = {
      ...this.base(type, body, owner),
      style_handle: "",
      leader_type: 1,
      leader_line_color: BY_BLOCK_RAW_COLOR,
      leader_linetype_handle: "",
      leader_lineweight: -2,
      has_dogleg: true,
      arrow_head_handle: "",
      content_type: 2,
      has_text_frame: false,
      text_left_attachment_type: 1,
      text_right_attachment_type: 1,
      text_top_attachment_type: 9,
      text_bottom_attachment_type: 9,
      arrow_heads: [],
      block_attribs: [],
      context,
    };
    let section: "common" | "context" | "leader" | "line" = "common";
    let mtext: DxfMLeaderMText | null = null;
    let block: DxfMLeaderBlock | null = null;
    let leader: DxfMLeaderLeader | null = null;
    let line: DxfMLeaderLine | null = null;
    let breakStart: Vec3 | null = null;
    let arrowIndex = 0;
    for (const { code, k, point } of this.points(this.subclassBody(body, "AcDbMLeader"), MLEADER_POINT_CODES)) {
      const handle = () => this.raw(k).trim().toUpperCase();
      if (section === "line" && line) {
        if (code === 305) {
          leader?.lines.push(line);
          line = null;
          section = "leader";
        } else if (code === 10) line.vertices.push(point);
        else if (code === 11) breakStart = point;
        else if (code === 12 && breakStart) {
          line.breaks.push({ start: breakStart, end: point });
          breakStart = null;
        } else if (code === 91) line.index = this.num(k);
        else if (code === 92) line.color = this.num(k);
        continue;
      }
      if (section === "leader" && leader) {
        if (code === 303) {
          context.leaders.push(leader);
          leader = null;
          section = "context";
        } else if (code === 304 && this.raw(k).trim() === "LEADER_LINE{") {
          line = { vertices: [], breaks: [], index: 0, color: BY_BLOCK_RAW_COLOR };
          breakStart = null;
          section = "line";
        } else if (code === 290) leader.has_last_leader_line = this.num(k) !== 0;
        else if (code === 291) leader.has_dogleg_vector = this.num(k) !== 0;
        else if (code === 10) leader.last_leader_point = point;
        else if (code === 11) leader.dogleg_vector = point;
        else if (code === 12) breakStart = point;
        else if (code === 13 && breakStart) {
          leader.breaks.push({ start: breakStart, end: point });
          breakStart = null;
        } else if (code === 90) leader.index = this.num(k);
        else if (code === 40) leader.dogleg_length = this.num(k);
        else if (code === 271) leader.attachment_direction = this.num(k);
        continue;
      }
      if (section === "context") {
        // 290・296 の後は、内容（文字・ブロック）の値が続く。内容の値でないコードで内容が終わる（ezdxf の MLeaderContext.load と同じ）
        if (mtext && this.mleaderMText(mtext, code, k, point)) continue;
        mtext = null;
        if (block && this.mleaderBlock(block, code, k, point)) continue;
        block = null;
        if (code === 301) section = "common";
        else if (code === 302 && this.raw(k).trim() === "LEADER{") {
          leader = {
            lines: [],
            has_last_leader_line: true,
            has_dogleg_vector: true,
            last_leader_point: [0, 0, 0],
            dogleg_vector: [1, 0, 0],
            dogleg_length: 0,
            index: 0,
            attachment_direction: 0,
            breaks: [],
          };
          breakStart = null;
          section = "leader";
        } else if (code === 290 && this.num(k) === 1) {
          mtext = {
            default_content: "",
            extrusion: [0, 0, 1],
            style_handle: "",
            insert: [0, 0, 0],
            text_direction: [1, 0, 0],
            width: 0,
            line_spacing_factor: 1,
            line_spacing_style: 1,
            color: BY_BLOCK_RAW_COLOR,
            alignment: 1,
            flow_direction: 1,
            bg_color: 0,
            bg_scale_factor: 1.5,
            use_window_bg_color: false,
            has_bg_fill: false,
            column_type: 0,
          };
          context.mtext = mtext;
        } else if (code === 296 && this.num(k) === 1) {
          block = {
            block_record_handle: "",
            extrusion: [0, 0, 1],
            insert: [0, 0, 0],
            scale: [1, 1, 1],
            rotation: 0,
            color: BY_BLOCK_RAW_COLOR,
          };
          context.block = block;
        } else if (code === 40) context.scale = this.num(k);
        else if (code === 10) context.base_point = point;
        else if (code === 41) context.char_height = this.num(k);
        else if (code === 140) context.arrow_head_size = this.num(k);
        else if (code === 145) context.landing_gap_size = this.num(k);
        continue;
      }
      switch (code) {
        case 300:
          section = "context";
          break;
        case 340:
          entity.style_handle = handle();
          break;
        case 170:
          entity.leader_type = this.num(k);
          break;
        case 91:
          entity.leader_line_color = this.num(k);
          break;
        case 341:
          entity.leader_linetype_handle = handle();
          break;
        case 171:
          entity.leader_lineweight = this.num(k);
          break;
        case 291:
          entity.has_dogleg = this.num(k) !== 0;
          break;
        case 342:
          entity.arrow_head_handle = handle();
          break;
        case 172:
          entity.content_type = this.num(k);
          break;
        case 292:
          entity.has_text_frame = this.num(k) !== 0;
          break;
        case 173:
          entity.text_left_attachment_type = this.num(k);
          break;
        case 95:
          entity.text_right_attachment_type = this.num(k);
          break;
        case 272:
          entity.text_bottom_attachment_type = this.num(k);
          break;
        case 273:
          entity.text_top_attachment_type = this.num(k);
          break;
        case 94:
          arrowIndex = this.num(k);
          break;
        case 345:
          entity.arrow_heads.push({ index: arrowIndex, handle: handle() });
          break;
        case 330:
          entity.block_attribs.push({ handle: handle(), index: 0, width: 1, text: "" });
          break;
        case 177:
        case 44:
        case 302: {
          const attrib = entity.block_attribs[entity.block_attribs.length - 1];
          if (!attrib) break;
          if (code === 177) attrib.index = this.num(k);
          else if (code === 44) attrib.width = this.num(k);
          else attrib.text = this.str(k);
          break;
        }
      }
    }
    return entity;
  }

  /** CONTEXT_DATA の文字の内容の値。内容の値でなければ false */
  private mleaderMText(mtext: DxfMLeaderMText, code: number, k: number, point: Vec3): boolean {
    switch (code) {
      case 304:
        // 書式コードを含むので、\U+ などは MTEXT の読み取りが復号する
        mtext.default_content = this.raw(k);
        return true;
      case 11:
        mtext.extrusion = point;
        return true;
      case 340:
        mtext.style_handle = this.raw(k).trim().toUpperCase();
        return true;
      case 12:
        mtext.insert = point;
        return true;
      case 13:
        mtext.text_direction = point;
        return true;
      case 43:
        mtext.width = this.num(k);
        return true;
      case 45:
        mtext.line_spacing_factor = this.num(k);
        return true;
      case 170:
        mtext.line_spacing_style = this.num(k);
        return true;
      case 90:
        mtext.color = this.num(k);
        return true;
      case 171:
        mtext.alignment = this.num(k);
        return true;
      case 172:
        mtext.flow_direction = this.num(k);
        return true;
      case 91:
        mtext.bg_color = this.num(k);
        return true;
      case 141:
        mtext.bg_scale_factor = this.num(k);
        return true;
      case 291:
        mtext.use_window_bg_color = this.num(k) !== 0;
        return true;
      case 292:
        mtext.has_bg_fill = this.num(k) !== 0;
        return true;
      case 173:
        mtext.column_type = this.num(k);
        return true;
      // 回転（42、13 の向きと同じ）、高さ、背景の透過、段組みの寸法は使わない
      case 42:
      case 44:
      case 92:
      case 293:
      case 142:
      case 143:
      case 294:
      case 144:
      case 295:
        return true;
    }
    return false;
  }

  /** CONTEXT_DATA のブロックの内容の値。内容の値でなければ false */
  private mleaderBlock(block: DxfMLeaderBlock, code: number, k: number, point: Vec3): boolean {
    switch (code) {
      case 341:
        block.block_record_handle = this.raw(k).trim().toUpperCase();
        return true;
      case 14:
        block.extrusion = point;
        return true;
      case 15:
        block.insert = point;
        return true;
      case 16:
        block.scale = point;
        return true;
      case 46:
        block.rotation = this.num(k);
        return true;
      case 93:
        block.color = this.num(k);
        return true;
      // 変換の行列（47 × 16）は AutoCAD も使わない（ezdxf の BlockData.export_dxf の注記）
      case 47:
        return true;
    }
    return false;
  }

  /** タグを順に返し、点の X のコード（codes）に続く Y・Z（+10・+20）を 1 つの点にまとめる */
  private *points(own: Body, codes: ReadonlySet<number>): Generator<{ code: number; k: number; point: Vec3 }> {
    for (let i = 0; i < own.length; i++) {
      const k = own[i] as number;
      const code = this.code(k);
      const point: Vec3 = [0, 0, 0];
      if (codes.has(code)) {
        point[0] = this.num(k);
        if (i + 1 < own.length && this.code(own[i + 1] as number) === code + 10)
          point[1] = this.num(own[++i] as number);
        if (i + 1 < own.length && this.code(own[i + 1] as number) === code + 20)
          point[2] = this.num(own[++i] as number);
      }
      yield { code, k, point };
    }
  }

  /** MLINE（DXF Reference「MLINE」）。頂点（11・12・13）ごとに、要素ごとの 74・41 と 75・42 が続く */
  private mline(body: Body, owner: Owner | null): DxfMLine {
    const own = this.subclassBody(body, "AcDbMline");
    const line_params: number[][][] = [];
    const fill_params: number[][][] = [];
    const entity: DxfMLine = {
      ...this.base("MLINE", body, owner),
      style_name: "Standard",
      scale: 1,
      justification: 0,
      flags: 0,
      closed: false,
      line_count: 0,
      base_point: [0, 0, 0],
      points: [],
      vertex_directions: [],
      miter_directions: [],
      line_params,
      fill_params,
    };
    let params: number[] | null = null;
    for (const { code, k, point } of this.points(own, MLINE_POINT_CODES)) {
      switch (code) {
        case 2:
          entity.style_name = this.str(k);
          break;
        case 340:
          entity.mlinestyle_handle = this.raw(k).trim().toUpperCase();
          break;
        case 40:
          entity.scale = this.num(k);
          break;
        case 70:
          entity.justification = this.num(k);
          break;
        case 71:
          entity.flags = this.num(k);
          break;
        case 73:
          entity.line_count = this.num(k);
          break;
        case 10:
          entity.base_point = point;
          break;
        case 11:
          entity.points.push(point);
          line_params.push([]);
          fill_params.push([]);
          params = null;
          break;
        case 12:
          entity.vertex_directions.push(point);
          break;
        case 13:
          entity.miter_directions.push(point);
          break;
        case 74:
        case 75:
          params = [];
          (code === 74 ? line_params : fill_params)[line_params.length - 1]?.push(params);
          break;
        case 41:
        case 42:
          params?.push(this.num(k));
          break;
      }
    }
    entity.closed = (entity.flags & 2) !== 0;
    return entity;
  }

  private viewport(start: number, end: number, body: Body, owner: Owner | null): DxfViewport {
    const frozen: string[] = [];
    for (const k of body) if (this.code(k) === 331) frozen.push(this.raw(k).trim().toUpperCase());
    const viewport: DxfViewport = {
      ...this.base("VIEWPORT", body, owner),
      center: this.point(body, 10),
      width: this.number(body, 40, 0),
      height: this.number(body, 41, 0),
      status: this.number(body, 68, 1),
      id: this.number(body, 69, 1),
      view_center: [this.number(body, 12, 0), this.number(body, 22, 0)],
      view_target: this.point(body, 17),
      view_direction: this.has(body, 16) ? this.point(body, 16) : [0, 0, 1],
      view_height: this.number(body, 45, 0),
      view_twist_angle: this.number(body, 51, 0),
      status_flags: this.number(body, 90, 0),
      frozen_layer_handles: frozen,
    };
    const clip = this.text(body, 340);
    if (clip && clip !== "0") viewport.clipping_boundary_handle = clip.trim().toUpperCase();
    if (!this.has(body, 45)) this.viewportR12(start, end, viewport);
    return viewport;
  }

  /** R12 のビューポートは表示の値を拡張データ ACAD の MVIEW に持つ（ezdxf の viewport.py と同じ並び） */
  private viewportR12(start: number, end: number, viewport: DxfViewport): void {
    let k = start;
    while (k < end && !(this.code(k) === 1000 && this.raw(k).trim().toUpperCase() === "MVIEW")) k++;
    if (k >= end) return;
    const reals: number[] = [];
    const points: Vec3[] = [];
    for (k++; k < end; k++) {
      const code = this.code(k);
      if (code === 1010) points.push([this.num(k), 0, 0]);
      else if (code === 1020 && points.length > 0) (points[points.length - 1] as Vec3)[1] = this.num(k);
      else if (code === 1030 && points.length > 0) (points[points.length - 1] as Vec3)[2] = this.num(k);
      else if (code === 1040) reals.push(this.num(k));
      else if (code === 1002 && this.raw(k).trim() === "{" && reals.length > 0) break;
    }
    if (points[0]) viewport.view_target = points[0];
    if (points[1]) viewport.view_direction = points[1];
    viewport.view_twist_angle = reals[0] ?? 0;
    viewport.view_height = reals[1] ?? 0;
    viewport.view_center = [reals[2] ?? 0, reals[3] ?? 0];
  }

  private image(type: "IMAGE" | "WIPEOUT", body: Body, owner: Owner | null): DxfImage {
    const boundary: Vec2[] = [];
    for (const k of body) {
      if (this.code(k) === 14) boundary.push([this.num(k), 0]);
      else if (this.code(k) === 24 && boundary.length > 0) (boundary[boundary.length - 1] as Vec2)[1] = this.num(k);
    }
    const size = this.point(body, 13);
    return {
      ...this.base(type, body, owner),
      insert: this.point(body, 10),
      u_pixel: this.point(body, 11),
      v_pixel: this.point(body, 12),
      image_size: [size[0] || 1, size[1] || 1],
      boundary,
      flags: this.number(body, 70, 0),
      clipping: this.number(body, 280, 0) !== 0,
    };
  }

  // --- OBJECTS --------------------------------------------------------------

  private objects(): void {
    while (this.i < this.length && !this.is(this.i, 0, "ENDSEC")) {
      if (this.is(this.i, 0, "SECTION") || this.is(this.i, 0, "EOF")) return;
      if (this.code(this.i) !== 0) {
        this.i++;
        continue;
      }
      const type = this.raw(this.i).trim().toUpperCase();
      const end = this.nextZero(this.i);
      const body = this.body(this.i + 1, end);
      if (type === "LAYOUT") this.layout(body);
      else if (type === "SORTENTSTABLE") this.sortTable(body);
      else if (type === "MLINESTYLE") this.mlineStyle(body);
      this.i = end;
    }
    this.i++;
  }

  /** LAYOUT は PLOTSETTINGS（AcDbPlotSettings）と AcDbLayout の 2 つのサブクラスで同じコードを使う */
  private layout(body: Body): void {
    const plot = this.subclassBody(body, "AcDbPlotSettings");
    const own = this.subclassBody(body, "AcDbLayout");
    const layout: DxfLayout = {
      handle: this.handleOf(body),
      name: this.text(own, 1, "Layout"),
      tab_order: this.number(own, 71, 0),
      model: false,
      active: false,
      block: "",
      paper_width: this.number(plot, 44, 0),
      paper_height: this.number(plot, 45, 0),
      margins: [this.number(plot, 40, 0), this.number(plot, 41, 0), this.number(plot, 42, 0), this.number(plot, 43, 0)],
      paper_size: this.text(plot, 4),
      paper_units: this.number(plot, 72, 1),
      plot_rotation: this.number(plot, 73, 0),
      plot_scale: [this.number(plot, 142, 1), this.number(plot, 143, 1)],
      plot_origin: [this.number(plot, 46, 0), this.number(plot, 47, 0)],
      limmin: [this.number(own, 10, 0), this.number(own, 20, 0)],
      limmax: [this.number(own, 11, 0), this.number(own, 21, 0)],
    };
    const record = this.handleOf(own, 330);
    if (record) layout.block_record_handle = record;
    this.layoutRecords.push({ handle: layout.handle, block_record_handle: record, layout });
  }

  /** MLINESTYLE（DXF Reference「MLINESTYLE」）。要素の数（71）の後の 62 は要素の色で、前の 62 は塗りの色 */
  private mlineStyle(body: Body): void {
    const style: DxfMLineStyle = {
      handle: this.handleOf(body),
      name: "",
      flags: 0,
      description: "",
      fill_color: 256,
      start_angle: 90,
      end_angle: 90,
      elements: [],
    };
    let elements = false;
    for (const k of this.subclassBody(body, "AcDbMlineStyle")) {
      const last = style.elements[style.elements.length - 1];
      switch (this.code(k)) {
        case 2:
          style.name = this.str(k);
          break;
        case 70:
          style.flags = this.num(k);
          break;
        case 3:
          style.description = this.str(k);
          break;
        case 62:
          if (!elements) style.fill_color = this.num(k);
          else if (last) last.color = this.num(k);
          break;
        case 51:
          style.start_angle = this.num(k);
          break;
        case 52:
          style.end_angle = this.num(k);
          break;
        case 71:
          elements = true;
          break;
        case 49:
          style.elements.push({ offset: this.num(k), color: 0, linetype: "BYLAYER" });
          break;
        case 6:
          if (last) last.linetype = this.str(k);
          break;
      }
    }
    this.doc.mline_styles ??= [];
    this.doc.mline_styles.push(style);
  }

  private sortTable(body: Body): void {
    const own = this.subclassBody(body, "AcDbSortentsTable");
    let owner = "";
    let entity = "";
    const handles: Record<string, string> = Object.create(null);
    for (const k of own) {
      const code = this.code(k);
      if (code === 330 && !owner) owner = this.raw(k).trim().toUpperCase();
      else if (code === 331) entity = this.raw(k).trim().toUpperCase();
      else if (code === 5 && entity) {
        handles[entity] = this.raw(k).trim().toUpperCase();
        entity = "";
      }
    }
    if (owner) this.sortTables.push({ owner, handles });
  }

  // --- 仕上げ ---------------------------------------------------------------

  private finish(): void {
    const doc = this.doc;
    for (const { block_record_handle, layout } of this.layoutRecords) {
      const name = this.blockRecords.get(block_record_handle) ?? "";
      layout.block = name;
      layout.model = isModelBlock(name) || layout.name.toUpperCase() === "MODEL";
      layout.active = name.toUpperCase() === PAPER_SPACE;
      doc.layouts.push(layout);
    }
    // R12 には LAYOUT がない。紙空間の図形があれば 1 つのレイアウトとして持つ
    if (!doc.layouts.some((layout) => !layout.model) && this.hasPaperEntities) {
      const min = doc.header.plimmin ?? [0, 0];
      const max = doc.header.plimmax ?? [0, 0];
      doc.layouts.push({
        handle: "",
        name: "Layout1",
        tab_order: 1,
        model: false,
        active: true,
        block: "*Paper_Space",
        paper_width: 0,
        paper_height: 0,
        margins: [0, 0, 0, 0],
        paper_size: "",
        paper_units: 1,
        plot_rotation: 0,
        plot_scale: [1, 1],
        plot_origin: [0, 0],
        limmin: [min[0], min[1]],
        limmax: [max[0], max[1]],
      });
    }
    doc.layouts.sort((a, b) => a.tab_order - b.tab_order);
    for (const table of this.sortTables) {
      const name = this.blockRecords.get(table.owner);
      if (name) doc.sort_handles[name] = table.handles;
    }
    const skipped = Object.entries(doc.skipped);
    if (skipped.length > 0) {
      const total = skipped.reduce((sum, [, count]) => sum + count, 0);
      this.notices.add(
        "info",
        "skipped-types",
        `読み取りに対応していない図形を表示していません（${skipped.map(([type, count]) => `${type} ${count}`).join("、")}）`,
        total,
      );
    }
  }
}

/** DXF（テキスト・バイナリ）を AutoCAD の図面のモデルにする */
export function parseDxf(bytes: Uint8Array): DxfDocument {
  const notices = new NoticeList();
  const read = readTags(bytes, notices);
  const parser = new Parser(
    read.tags,
    notices,
    read.version,
    read.binary,
    read.encoding,
    read.encodingSource,
    read.codepage,
  );
  return parser.parse();
}
