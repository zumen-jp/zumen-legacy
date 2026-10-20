import {
  BufferAttribute,
  BufferGeometry,
  Color,
  DirectionalLight,
  DoubleSide,
  Float32BufferAttribute,
  HemisphereLight,
  LineBasicMaterial,
  LineDashedMaterial,
  LineSegments,
  type Material,
  Mesh,
  MeshLambertMaterial,
  OrthographicCamera,
  PerspectiveCamera,
  Points,
  PointsMaterial,
  Raycaster,
  Scene,
  Vector2,
  Vector3,
  WebGLRenderer,
} from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { CSS2DObject, CSS2DRenderer } from "three/addons/renderers/CSS2DRenderer.js";
import { isMemberVisible, type MemberFilter } from "../filter.js";
import { type GridAxis, isArcAxis, type StructureMember, type StructureModel, type Vec3 } from "../types.js";
import { type PickTarget, pickTarget } from "./pick.js";
import type { StructurePalette } from "./style.js";
import { addFace, addLine, addSolid, createSink, type MeshSink } from "./tessellate.js";

export type Viewpoint = "iso" | "top" | "front" | "side";
export type ColorMode = "kind" | "structure";
export type Projection = "perspective" | "orthographic";

/** 描き方の切り替え */
export interface RenderOptions {
  colorMode: ColorMode;
  /** 稜線 */
  edges: boolean;
  /** スラブ・壁を半透明にする */
  translucent: boolean;
  axes: boolean;
  stories: boolean;
  /** 通り芯・階の名前 */
  labels: boolean;
  nodes: boolean;
}

/** 描く色。WebGL に渡す色は 0xRRGGBB、名前の層に使う色は CSS の色 */
export interface RenderColors {
  palette: StructurePalette;
  background: number;
  selection: number;
  hover: number | null;
  edge: number;
  line: number;
  axis: number;
  story: number;
  node: number;
  kinds: Readonly<Record<string, number>>;
  structures: Readonly<Record<string, number>>;
  structureFallback: number;
}

/** カメラ。座標はモデルの座標（mm） */
export interface CameraState {
  position: Vec3;
  target: Vec3;
  /** 注視点で見える高さ（mm） */
  span: number;
}

type LayerName = "frame" | "panel" | "face";

interface Layer {
  geometry: BufferGeometry;
  mesh: Mesh;
  edges: LineSegments;
  fullIndex: Uint32Array;
  positions: Float32Array;
  colors: Uint8Array;
}

interface Entry {
  member: StructureMember;
  index: number;
  layer: LayerName | null;
  vertexStart: number;
  vertexCount: number;
  indexStart: number;
  indexCount: number;
  edgeStart: number;
  edgeCount: number;
  lineStart: number;
  lineCount: number;
  min: [number, number, number];
  max: [number, number, number];
  visible: boolean;
}

const VIEW_DIRECTIONS: Record<Viewpoint, Vec3> = {
  iso: [-0.62, -1, 0.72],
  top: [0, -0.0005, 1],
  front: [0, -1, 0],
  side: [1, 0, 0],
};

const PANEL_KINDS = new Set(["slab", "wall"]);
const FOV = 40;

function unit(v: Vec3): Vector3 {
  return new Vector3(v[0], v[1], v[2]).normalize();
}

/** 色の配列（頂点ごとの RGB、0〜255）に部材の色を書く */
function paint(colors: Uint8Array, start: number, count: number, color: number) {
  const r = (color >> 16) & 0xff;
  const g = (color >> 8) & 0xff;
  const b = color & 0xff;
  for (let i = start; i < start + count; i++) {
    colors[i * 3] = r;
    colors[i * 3 + 1] = g;
    colors[i * 3 + 2] = b;
  }
}

/** 名前の層の要素に付ける見た目。外の CSS がなくても読めるように、すべて要素に書く */
const LABEL_FONT = '600 11px/1 system-ui, -apple-system, "Hiragino Sans", "Noto Sans JP", sans-serif';

/**
 * three.js で構造モデルを描く（StructureViewer の内部）。
 * 部材は種類（線材・面材・荷重面）ごとに 1 つの形状にまとめ、表示の切り替えは三角形の番号の並びを作り直して行う。
 */
export class StructureRenderer {
  readonly canvas: HTMLCanvasElement;
  /** 通り芯・階の名前を置く層 */
  readonly labelLayer: HTMLElement;
  private readonly renderer: WebGLRenderer;
  private readonly labelRenderer: CSS2DRenderer;
  private readonly scene = new Scene();
  private readonly perspective = new PerspectiveCamera(FOV, 1, 1, 1e7);
  private readonly orthographic = new OrthographicCamera(-1, 1, 1, -1, 1, 1e7);
  private camera: PerspectiveCamera | OrthographicCamera = this.perspective;
  private controls: OrbitControls;
  private readonly light = new DirectionalLight(0xffffff, 1.6);
  private readonly hemisphere = new HemisphereLight(0xffffff, 0x8d8a84, 1.5);
  private readonly raycaster = new Raycaster();
  private frame = 0;
  private width = 0;
  private height = 0;
  private orthoHalfHeight = 1000;
  private model: StructureModel | null = null;
  private origin: Vec3 = [0, 0, 0];
  private radius = 1000;
  private entries: Entry[] = [];
  private readonly byKey = new Map<string, Entry>();
  /** 見えている部材の選択の対象。絞り込みを変えたときに作り直す */
  private targets: PickTarget[] = [];
  private layers = new Map<LayerName, Layer>();
  private lineMembers: { geometry: BufferGeometry; object: LineSegments; positions: Float32Array } | null = null;
  private axisObjects: { line: LineSegments; labels: CSS2DObject[] } | null = null;
  private storyObjects: { lines: LineSegments; labels: CSS2DObject[] } | null = null;
  private nodePoints: Points | null = null;
  private selection: { mesh: Mesh; edges: LineSegments } | null = null;
  private hover: { mesh: Mesh; edges: LineSegments } | null = null;
  private hoveredKey: string | null = null;
  private filter: MemberFilter = {};
  /** 絞り込んだ通り芯。同じグループの他の通り芯は描かない */
  private activeAxis: GridAxis | null = null;
  private options: RenderOptions;
  private colors: RenderColors;
  private readonly materials = {
    frame: new MeshLambertMaterial({
      vertexColors: true,
      polygonOffset: true,
      polygonOffsetFactor: 1,
      polygonOffsetUnits: 1,
    }),
    panel: new MeshLambertMaterial({
      vertexColors: true,
      polygonOffset: true,
      polygonOffsetFactor: 1,
      polygonOffsetUnits: 1,
    }),
    face: new MeshLambertMaterial({
      vertexColors: true,
      side: DoubleSide,
      transparent: true,
      opacity: 0.3,
      depthWrite: false,
    }),
    edge: new LineBasicMaterial({ transparent: true, opacity: 0.55 }),
    panelEdge: new LineBasicMaterial({ transparent: true, opacity: 0.35 }),
    line: new LineBasicMaterial(),
    axis: new LineDashedMaterial({ dashSize: 600, gapSize: 200, transparent: true, opacity: 0.8 }),
    story: new LineBasicMaterial({ transparent: true, opacity: 0.45 }),
    node: new PointsMaterial({ size: 4, sizeAttenuation: false }),
    highlight: new MeshLambertMaterial({ polygonOffset: true, polygonOffsetFactor: -1, polygonOffsetUnits: -4 }),
    highlightEdge: new LineBasicMaterial({ depthTest: false, transparent: true, opacity: 0.9 }),
    hover: new MeshLambertMaterial({ polygonOffset: true, polygonOffsetFactor: -1, polygonOffsetUnits: -4 }),
    hoverEdge: new LineBasicMaterial({ depthTest: false, transparent: true, opacity: 0.9 }),
  };
  /** カメラが動いたとき */
  onChange: (() => void) | null = null;
  /** 利用者がカメラを動かし始めたとき・終えたとき（OrbitControls の start・end） */
  onStart: (() => void) | null = null;
  onEnd: (() => void) | null = null;
  /** カメラの操作（OrbitControls）を作ったとき。投影を替えると作り直す */
  onControls: ((controls: OrbitControls) => void) | null = null;

  constructor(
    private readonly container: HTMLElement,
    options: RenderOptions,
    colors: RenderColors,
  ) {
    this.options = options;
    this.colors = colors;
    this.renderer = new WebGLRenderer({ antialias: true, preserveDrawingBuffer: false });
    this.renderer.setPixelRatio(Math.min(globalThis.devicePixelRatio || 1, 2));
    this.canvas = this.renderer.domElement;
    Object.assign(this.canvas.style, {
      position: "absolute",
      inset: "0",
      width: "100%",
      height: "100%",
      display: "block",
    });
    container.appendChild(this.canvas);
    this.labelRenderer = new CSS2DRenderer();
    this.labelLayer = this.labelRenderer.domElement;
    Object.assign(this.labelLayer.style, {
      position: "absolute",
      inset: "0",
      pointerEvents: "none",
      overflow: "hidden",
    });
    this.labelLayer.setAttribute("aria-hidden", "true");
    container.appendChild(this.labelLayer);

    for (const camera of [this.perspective, this.orthographic]) camera.up.set(0, 0, 1);
    this.perspective.add(this.light);
    this.light.position.set(0.3, 0.6, 1);
    this.scene.add(this.perspective, this.orthographic, this.hemisphere);
    this.controls = this.createControls();
    this.applyColors();
  }

  private createControls(): OrbitControls {
    const controls = new OrbitControls(this.camera, this.canvas);
    controls.screenSpacePanning = true;
    controls.zoomToCursor = true;
    controls.keyPanSpeed = 20;
    controls.addEventListener("change", () => {
      this.syncLight();
      this.requestRender();
      this.onChange?.();
    });
    controls.addEventListener("start", () => this.onStart?.());
    controls.addEventListener("end", () => this.onEnd?.());
    this.onControls?.(controls);
    return controls;
  }

  /** 今のカメラの操作 */
  getControls(): OrbitControls {
    return this.controls;
  }

  private syncLight() {
    // 光を常にカメラの側から当てる（向きは一定、カメラの子にしている）
    if (this.camera === this.orthographic && this.light.parent !== this.orthographic) this.orthographic.add(this.light);
    if (this.camera === this.perspective && this.light.parent !== this.perspective) this.perspective.add(this.light);
  }

  /** 表示領域の大きさに合わせる。大きさが変わったら true */
  resize(): boolean {
    const rect = this.container.getBoundingClientRect();
    const width = Math.max(1, Math.round(rect.width));
    const height = Math.max(1, Math.round(rect.height));
    if (width === this.width && height === this.height) return false;
    this.width = width;
    this.height = height;
    this.renderer.setSize(this.width, this.height, false);
    this.labelRenderer.setSize(this.width, this.height);
    this.updateProjection();
    this.requestRender();
    return true;
  }

  get size(): { width: number; height: number } {
    return { width: this.width, height: this.height };
  }

  private updateProjection() {
    const aspect = this.width / this.height;
    this.perspective.aspect = aspect;
    this.perspective.updateProjectionMatrix();
    const h = this.orthoHalfHeight;
    this.orthographic.left = -h * aspect;
    this.orthographic.right = h * aspect;
    this.orthographic.top = h;
    this.orthographic.bottom = -h;
    this.orthographic.updateProjectionMatrix();
  }

  requestRender(): void {
    if (this.frame) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = 0;
      this.renderer.render(this.scene, this.camera);
      this.labelRenderer.render(this.scene, this.camera);
    });
  }

  /** 色を替える */
  setColors(colors: RenderColors): void {
    const kindsChanged =
      colors.structureFallback !== this.colors.structureFallback ||
      Object.keys(colors.kinds).some((key) => colors.kinds[key] !== this.colors.kinds[key]) ||
      Object.keys(colors.structures).some((key) => colors.structures[key] !== this.colors.structures[key]) ||
      Object.keys(this.colors.structures).some((key) => !(key in colors.structures));
    const labelsChanged = (["axis", "labelText", "labelBackground", "storyLabelText"] as const).some(
      (key) => colors.palette[key] !== this.colors.palette[key],
    );
    const hoverChanged = colors.hover !== this.colors.hover;
    this.colors = colors;
    this.applyColors();
    if (kindsChanged) this.paintAll();
    if (labelsChanged) this.styleLabels();
    if (hoverChanged) this.setHover(this.hoveredKey);
    this.requestRender();
  }

  private applyColors() {
    const c = this.colors;
    this.scene.background = new Color(c.background);
    this.materials.edge.color.set(c.edge);
    this.materials.panelEdge.color.set(c.edge);
    this.materials.line.color.set(c.line);
    this.materials.axis.color.set(c.axis);
    this.materials.story.color.set(c.story);
    this.materials.node.color.set(c.node);
    this.materials.highlight.color.set(c.selection);
    this.materials.highlight.emissive.set(c.selection).multiplyScalar(0.35);
    this.materials.highlightEdge.color.set(c.selection);
    if (c.hover !== null) {
      this.materials.hover.color.set(c.hover);
      this.materials.hover.emissive.set(c.hover).multiplyScalar(0.35);
      this.materials.hoverEdge.color.set(c.hover);
    }
  }

  /** 表示するモデルを入れ替える。カメラは全体が見える 3D の向きにする */
  setModel(model: StructureModel, filter: MemberFilter = {}, activeAxis: GridAxis | null = null): void {
    this.clearModel();
    this.model = model;
    this.filter = filter;
    this.activeAxis = activeAxis;
    const b = model.bounds;
    this.origin = [
      Math.round((b.min[0] + b.max[0]) / 2),
      Math.round((b.min[1] + b.max[1]) / 2),
      Math.round((b.min[2] + b.max[2]) / 2),
    ];
    this.radius = Math.max(Math.hypot(b.max[0] - b.min[0], b.max[1] - b.min[1], b.max[2] - b.min[2]) / 2, 500);

    const sinks: Record<LayerName, MeshSink> = {
      frame: createSink(this.origin),
      panel: createSink(this.origin),
      face: createSink(this.origin),
    };
    const lineSink = createSink(this.origin);
    model.members.forEach((member, index) => {
      const layerName: LayerName | null =
        member.solids.length > 0
          ? PANEL_KINDS.has(member.kind)
            ? "panel"
            : "frame"
          : member.faces.length > 0
            ? "face"
            : null;
      const sink = layerName ? sinks[layerName] : null;
      const entry: Entry = {
        member,
        index,
        layer: layerName,
        vertexStart: sink ? sink.positions.length / 3 : 0,
        vertexCount: 0,
        indexStart: sink ? sink.indices.length : 0,
        indexCount: 0,
        edgeStart: sink ? sink.edges.length / 3 : 0,
        edgeCount: 0,
        lineStart: lineSink.edges.length / 3,
        lineCount: 0,
        min: [
          member.bounds.min[0] - this.origin[0],
          member.bounds.min[1] - this.origin[1],
          member.bounds.min[2] - this.origin[2],
        ],
        max: [
          member.bounds.max[0] - this.origin[0],
          member.bounds.max[1] - this.origin[1],
          member.bounds.max[2] - this.origin[2],
        ],
        visible: true,
      };
      if (sink) {
        for (const solid of member.solids) addSolid(sink, solid);
        for (const face of member.faces) addFace(sink, face);
        entry.vertexCount = sink.positions.length / 3 - entry.vertexStart;
        entry.indexCount = sink.indices.length - entry.indexStart;
        entry.edgeCount = sink.edges.length / 3 - entry.edgeStart;
      }
      for (const line of member.lines) addLine(lineSink, line);
      entry.lineCount = lineSink.edges.length / 3 - entry.lineStart;
      this.entries.push(entry);
      this.byKey.set(member.key, entry);
    });

    for (const name of ["frame", "panel", "face"] as const) {
      const sink = sinks[name];
      if (sink.positions.length === 0) continue;
      const positions = sink.positions.toArray();
      const geometry = new BufferGeometry();
      geometry.setAttribute("position", new BufferAttribute(positions, 3));
      geometry.setAttribute("normal", new BufferAttribute(sink.normals.toArray(), 3));
      const colors = new Uint8Array(positions.length);
      geometry.setAttribute("color", new BufferAttribute(colors, 3, true));
      const fullIndex = sink.indices.toArray();
      geometry.setIndex(new BufferAttribute(fullIndex, 1));
      geometry.computeBoundingSphere();
      const mesh = new Mesh(geometry, this.materials[name]);
      mesh.renderOrder = name === "frame" ? 0 : name === "panel" ? 1 : 2;
      const edgeGeometry = new BufferGeometry();
      edgeGeometry.setAttribute("position", new BufferAttribute(sink.edges.toArray(), 3));
      const edges = new LineSegments(edgeGeometry, name === "frame" ? this.materials.edge : this.materials.panelEdge);
      edges.renderOrder = 3;
      this.scene.add(mesh, edges);
      this.layers.set(name, { geometry, mesh, edges, fullIndex, positions, colors });
    }
    if (lineSink.edges.length > 0) {
      const positions = lineSink.edges.toArray();
      const geometry = new BufferGeometry();
      geometry.setAttribute("position", new BufferAttribute(positions, 3));
      const object = new LineSegments(geometry, this.materials.line);
      this.scene.add(object);
      this.lineMembers = { geometry, object, positions };
    }
    const nodePositions = new Float32Array(model.nodes.length * 3);
    model.nodes.forEach((node, i) => {
      nodePositions[i * 3] = node.position[0] - this.origin[0];
      nodePositions[i * 3 + 1] = node.position[1] - this.origin[1];
      nodePositions[i * 3 + 2] = node.position[2] - this.origin[2];
    });
    const nodeGeometry = new BufferGeometry();
    nodeGeometry.setAttribute("position", new BufferAttribute(nodePositions, 3));
    this.nodePoints = new Points(nodeGeometry, this.materials.node);
    this.scene.add(this.nodePoints);

    const dash = Math.max(this.radius / 120, 150);
    this.materials.axis.dashSize = dash * 2.5;
    this.materials.axis.gapSize = dash;
    this.paintAll();
    this.applyFilter();
    this.viewFrom("iso");
  }

  private clearModel() {
    for (const layer of this.layers.values()) {
      layer.geometry.dispose();
      layer.edges.geometry.dispose();
      this.scene.remove(layer.mesh, layer.edges);
    }
    this.layers.clear();
    if (this.lineMembers) {
      this.lineMembers.geometry.dispose();
      this.scene.remove(this.lineMembers.object);
      this.lineMembers = null;
    }
    if (this.nodePoints) {
      this.nodePoints.geometry.dispose();
      this.scene.remove(this.nodePoints);
      this.nodePoints = null;
    }
    this.removeAxes();
    this.removeStories();
    this.selection = this.removeHighlight(this.selection);
    this.hover = this.removeHighlight(this.hover);
    this.hoveredKey = null;
    this.entries = [];
    this.byKey.clear();
    this.targets = [];
    this.model = null;
  }

  private paintAll() {
    for (const entry of this.entries) this.paintEntry(entry);
    for (const layer of this.layers.values()) {
      const attribute = layer.geometry.getAttribute("color") as BufferAttribute;
      attribute.needsUpdate = true;
    }
  }

  private paintEntry(entry: Entry) {
    if (!entry.layer) return;
    const layer = this.layers.get(entry.layer);
    if (!layer) return;
    const member = entry.member;
    const color =
      this.options.colorMode === "kind"
        ? (this.colors.kinds[member.kind] ?? this.colors.structureFallback)
        : (this.colors.structures[member.structure] ?? this.colors.structureFallback);
    paint(layer.colors, entry.vertexStart, entry.vertexCount, color);
  }

  /** 部材の絞り込み（種類・階・通り芯） */
  setFilter(filter: MemberFilter, activeAxis: GridAxis | null = null): void {
    this.filter = filter;
    this.activeAxis = activeAxis;
    this.applyFilter();
  }

  /** 部材が見えているか（絞り込みの結果） */
  isVisible(key: string): boolean {
    return this.byKey.get(key)?.visible ?? false;
  }

  private applyFilter() {
    if (!this.model) return;
    // 見える部材の三角形・稜線の番号を数えてから、型付き配列に詰める
    const triangleCounts = new Map<LayerName, number>();
    const edgeCounts = new Map<LayerName, number>();
    let lineCount = 0;
    this.targets = [];
    for (const entry of this.entries) {
      entry.visible = isMemberVisible(entry.member, this.filter);
      if (!entry.visible) continue;
      if (entry.layer) {
        triangleCounts.set(entry.layer, (triangleCounts.get(entry.layer) ?? 0) + entry.indexCount);
        edgeCounts.set(entry.layer, (edgeCounts.get(entry.layer) ?? 0) + entry.edgeCount);
      }
      lineCount += entry.lineCount;
      const layer = entry.layer ? this.layers.get(entry.layer) : undefined;
      this.targets.push({
        index: entry.index,
        positions: layer?.positions ?? null,
        indices: layer?.fullIndex ?? null,
        indexStart: entry.indexStart,
        indexCount: entry.indexCount,
        lines: entry.lineCount > 0 ? (this.lineMembers?.positions ?? null) : null,
        lineStart: entry.lineStart,
        lineCount: entry.lineCount,
        min: entry.min,
        max: entry.max,
      });
    }
    for (const [name, layer] of this.layers) {
      const indices = new Uint32Array(triangleCounts.get(name) ?? 0);
      const edges = new Uint32Array(edgeCounts.get(name) ?? 0);
      let i = 0;
      let e = 0;
      for (const entry of this.entries) {
        if (!entry.visible || entry.layer !== name) continue;
        indices.set(layer.fullIndex.subarray(entry.indexStart, entry.indexStart + entry.indexCount), i);
        i += entry.indexCount;
        for (let k = 0; k < entry.edgeCount; k++) edges[e++] = entry.edgeStart + k;
      }
      layer.geometry.setIndex(new BufferAttribute(indices, 1));
      layer.edges.geometry.setIndex(new BufferAttribute(edges, 1));
    }
    if (this.lineMembers) {
      const lines = new Uint32Array(lineCount);
      let l = 0;
      for (const entry of this.entries) {
        if (!entry.visible) continue;
        for (let k = 0; k < entry.lineCount; k++) lines[l++] = entry.lineStart + k;
      }
      this.lineMembers.geometry.setIndex(new BufferAttribute(lines, 1));
    }
    this.buildAxes();
    this.buildStories();
    this.applyOptions();
  }

  setOptions(options: Partial<RenderOptions>): void {
    const previous = this.options;
    this.options = { ...this.options, ...options };
    if (this.options.colorMode !== previous.colorMode) this.paintAll();
    this.applyOptions();
  }

  private applyOptions() {
    const o = this.options;
    for (const [name, layer] of this.layers) layer.edges.visible = o.edges || name === "face";
    this.materials.panel.transparent = o.translucent;
    this.materials.panel.opacity = o.translucent ? 0.42 : 1;
    this.materials.panel.depthWrite = !o.translucent;
    this.materials.panel.needsUpdate = true;
    if (this.axisObjects) {
      this.axisObjects.line.visible = o.axes;
      for (const label of this.axisObjects.labels) label.visible = o.axes && o.labels;
    }
    if (this.storyObjects) {
      this.storyObjects.lines.visible = o.stories;
      for (const label of this.storyObjects.labels) label.visible = o.stories && o.labels;
    }
    if (this.nodePoints) this.nodePoints.visible = o.nodes;
    this.requestRender();
  }

  /** 通り芯を描く高さ。階を 1 つに絞ったときはその階、それ以外は最も低い階 */
  private axisLevel(): number {
    const model = this.model;
    if (!model) return 0;
    const stories = this.filter.stories;
    if (stories && stories.size === 1) {
      const id = [...stories][0];
      const story = model.stories.find((item) => item.id === id);
      if (story) return story.height;
    }
    return model.stories[0]?.height ?? model.bounds.min[2];
  }

  private label(text: string, kind: "axis" | "story", position: Vector3): CSS2DObject {
    const element = this.container.ownerDocument.createElement("div");
    element.dataset.zumenLabel = kind;
    element.textContent = text;
    this.styleLabel(element);
    const object = new CSS2DObject(element);
    object.position.copy(position);
    return object;
  }

  private styleLabel(element: HTMLElement) {
    const palette = this.colors.palette;
    const common = {
      boxSizing: "border-box",
      font: LABEL_FONT,
      color: palette.labelText,
      whiteSpace: "nowrap",
      userSelect: "none",
    };
    if (element.dataset.zumenLabel === "axis") {
      Object.assign(element.style, common, {
        minWidth: "22px",
        padding: "4px 5px",
        border: `1.5px solid ${palette.axis}`,
        borderRadius: "999px",
        background: palette.labelBackground,
        textAlign: "center",
      });
    } else {
      Object.assign(element.style, common, {
        padding: "2px 6px",
        borderRadius: "4px",
        background: `color-mix(in srgb, ${palette.labelBackground} 85%, transparent)`,
        color: palette.storyLabelText,
      });
    }
  }

  private styleLabels() {
    for (const objects of [this.axisObjects?.labels, this.storyObjects?.labels]) {
      for (const object of objects ?? []) this.styleLabel(object.element);
    }
  }

  private removeAxes() {
    if (!this.axisObjects) return;
    this.axisObjects.line.geometry.dispose();
    this.scene.remove(this.axisObjects.line, ...this.axisObjects.labels);
    for (const label of this.axisObjects.labels) label.element.remove();
    this.axisObjects = null;
  }

  private buildAxes() {
    this.removeAxes();
    const model = this.model;
    if (!model || model.axes.length === 0) return;
    const z = this.axisLevel() - this.origin[2];
    const [ox, oy] = this.origin;
    const points: number[] = [];
    const labels: CSS2DObject[] = [];
    const push = (a: [number, number], b: [number, number]) =>
      points.push(a[0] - ox, a[1] - oy, z, b[0] - ox, b[1] - oy, z);
    const active = this.activeAxis;
    for (const axis of model.axes) {
      if (active && axis.group === active.group && axis.key !== active.key) continue;
      if (isArcAxis(axis)) {
        const steps = Math.max(8, Math.ceil(Math.abs(axis.endAngle - axis.startAngle) / 5));
        let previous: [number, number] | null = null;
        for (let i = 0; i <= steps; i++) {
          const a = ((axis.startAngle + ((axis.endAngle - axis.startAngle) * i) / steps) * Math.PI) / 180;
          const p: [number, number] = [
            axis.center[0] + axis.radius * Math.cos(a),
            axis.center[1] + axis.radius * Math.sin(a),
          ];
          if (previous) push(previous, p);
          previous = p;
        }
        const a = (axis.startAngle * Math.PI) / 180;
        labels.push(
          this.label(
            axis.name,
            "axis",
            new Vector3(
              axis.center[0] + axis.radius * Math.cos(a) - ox,
              axis.center[1] + axis.radius * Math.sin(a) - oy,
              z,
            ),
          ),
        );
        continue;
      }
      push([axis.start[0], axis.start[1]], [axis.end[0], axis.end[1]]);
      labels.push(this.label(axis.name, "axis", new Vector3(axis.start[0] - ox, axis.start[1] - oy, z)));
      if (axis.kind !== "radial") {
        labels.push(this.label(axis.name, "axis", new Vector3(axis.end[0] - ox, axis.end[1] - oy, z)));
      }
    }
    const geometry = new BufferGeometry();
    geometry.setAttribute("position", new Float32BufferAttribute(points, 3));
    const line = new LineSegments(geometry, this.materials.axis);
    line.computeLineDistances();
    this.scene.add(line, ...labels);
    this.axisObjects = { line, labels };
    this.applyOptions();
  }

  private removeStories() {
    if (!this.storyObjects) return;
    this.storyObjects.lines.geometry.dispose();
    this.scene.remove(this.storyObjects.lines, ...this.storyObjects.labels);
    for (const label of this.storyObjects.labels) label.element.remove();
    this.storyObjects = null;
  }

  private buildStories() {
    this.removeStories();
    const model = this.model;
    if (!model || model.stories.length === 0) return;
    const b = model.bounds;
    const margin = Math.max(this.radius * 0.06, 500);
    const x0 = b.min[0] - margin - this.origin[0];
    const x1 = b.max[0] + margin - this.origin[0];
    const y0 = b.min[1] - margin - this.origin[1];
    const y1 = b.max[1] + margin - this.origin[1];
    const points: number[] = [];
    const labels: CSS2DObject[] = [];
    // 階を 1 つに絞ったときは、その階だけを示す
    const only = this.filter.stories?.size === 1 ? this.filter.stories : null;
    for (const story of model.stories) {
      if (only && !only.has(story.id)) continue;
      const z = story.height - this.origin[2];
      points.push(x0, y0, z, x1, y0, z, x1, y0, z, x1, y1, z, x1, y1, z, x0, y1, z, x0, y1, z, x0, y0, z);
      const text = `${story.name}${story.levelName && story.levelName !== story.name ? `（${story.levelName}）` : ""}`;
      labels.push(this.label(text, "story", new Vector3(x0, y0, z)));
    }
    const geometry = new BufferGeometry();
    geometry.setAttribute("position", new Float32BufferAttribute(points, 3));
    const lines = new LineSegments(geometry, this.materials.story);
    this.scene.add(lines, ...labels);
    this.storyObjects = { lines, labels };
  }

  /** 部材を選ぶ（空で選択を外す）。選んだ部材は色を変え、稜線を手前に描く */
  setSelection(keys: readonly string[]): void {
    this.selection = this.removeHighlight(this.selection);
    this.selection = this.buildHighlight(keys, this.materials.highlight, this.materials.highlightEdge, 4);
    this.requestRender();
  }

  /** ポインターの下の部材。色（RenderColors.hover）が null なら描かない */
  setHover(key: string | null): void {
    this.hoveredKey = key;
    // 何も描かないときは描き直さない（大きなモデルでは 1 回の描画が重い）
    if (!this.hover && (key === null || this.colors.hover === null)) return;
    this.hover = this.removeHighlight(this.hover);
    if (key !== null && this.colors.hover !== null)
      this.hover = this.buildHighlight([key], this.materials.hover, this.materials.hoverEdge, 6);
    this.requestRender();
  }

  /** 部材の三角形と稜線を、強調の材質で描き直す形状を作る。部材がなければ null */
  private buildHighlight(
    keys: readonly string[],
    material: MeshLambertMaterial,
    edgeMaterial: LineBasicMaterial,
    order: number,
  ): { mesh: Mesh; edges: LineSegments } | null {
    const positions: number[] = [];
    const edgePositions: number[] = [];
    let found = false;
    for (const key of keys) {
      const entry = this.byKey.get(key);
      if (!entry) continue;
      found = true;
      const layer = entry.layer ? this.layers.get(entry.layer) : undefined;
      if (layer) {
        for (let i = entry.indexStart; i < entry.indexStart + entry.indexCount; i++) {
          const v = layer.fullIndex[i] as number;
          positions.push(
            layer.positions[v * 3] as number,
            layer.positions[v * 3 + 1] as number,
            layer.positions[v * 3 + 2] as number,
          );
        }
        const edges = layer.edges.geometry.getAttribute("position") as BufferAttribute;
        for (let i = entry.edgeStart; i < entry.edgeStart + entry.edgeCount; i++)
          edgePositions.push(edges.getX(i), edges.getY(i), edges.getZ(i));
      }
      if (this.lineMembers) {
        const p = this.lineMembers.positions;
        for (let i = entry.lineStart; i < entry.lineStart + entry.lineCount; i++)
          edgePositions.push(p[i * 3] as number, p[i * 3 + 1] as number, p[i * 3 + 2] as number);
      }
    }
    if (!found) return null;
    const geometry = new BufferGeometry();
    geometry.setAttribute("position", new Float32BufferAttribute(positions, 3));
    geometry.computeVertexNormals();
    const mesh = new Mesh(geometry, material);
    mesh.renderOrder = order;
    const edgeGeometry = new BufferGeometry();
    edgeGeometry.setAttribute("position", new Float32BufferAttribute(edgePositions, 3));
    const edges = new LineSegments(edgeGeometry, edgeMaterial);
    edges.renderOrder = order + 1;
    this.scene.add(mesh, edges);
    return { mesh, edges };
  }

  private removeHighlight(highlight: { mesh: Mesh; edges: LineSegments } | null): null {
    if (!highlight) return null;
    highlight.mesh.geometry.dispose();
    highlight.edges.geometry.dispose();
    this.scene.remove(highlight.mesh, highlight.edges);
    return null;
  }

  /** 表示領域の点（左上からの CSS px）にある部材。見えている部材だけを対象にする */
  pick(px: number, py: number, pixels = 6): StructureMember | null {
    if (!this.model) return null;
    const rect = this.canvas.getBoundingClientRect();
    if (rect.width === 0 || rect.height === 0) return null;
    const ndc = new Vector2((px / rect.width) * 2 - 1, -(py / rect.height) * 2 + 1);
    this.raycaster.setFromCamera(ndc, this.camera);
    const { origin, direction } = this.raycaster.ray;
    const tolerance =
      this.camera === this.perspective
        ? (t: number) => ((2 * t * Math.tan((FOV * Math.PI) / 360)) / this.height) * pixels
        : () => ((2 * this.orthoHalfHeight) / this.orthographic.zoom / this.height) * pixels;
    const hit = pickTarget(
      { origin: [origin.x, origin.y, origin.z], direction: [direction.x, direction.y, direction.z] },
      this.targets,
      tolerance,
    );
    return hit ? (this.model.members[hit.index] ?? null) : null;
  }

  /** モデルの点（mm）が表示領域のどこに見えるか（左上からの CSS px）。カメラの後ろなら null */
  project(point: Vec3): [number, number] | null {
    const v = new Vector3(point[0] - this.origin[0], point[1] - this.origin[1], point[2] - this.origin[2]);
    this.camera.updateMatrixWorld();
    v.project(this.camera);
    if (v.z < -1 || v.z > 1) return null;
    return [((v.x + 1) / 2) * this.width, ((1 - v.y) / 2) * this.height];
  }

  /** 見えている部材の外接箱（描画の座標）。部材がなければモデル全体 */
  private visibleBox(): { min: Vector3; max: Vector3 } {
    const min = new Vector3(Number.POSITIVE_INFINITY, Number.POSITIVE_INFINITY, Number.POSITIVE_INFINITY);
    const max = new Vector3(Number.NEGATIVE_INFINITY, Number.NEGATIVE_INFINITY, Number.NEGATIVE_INFINITY);
    for (const entry of this.entries) {
      if (!entry.visible || !(entry.min[0] <= entry.max[0])) continue;
      min.min(new Vector3(...entry.min));
      max.max(new Vector3(...entry.max));
    }
    if (!(min.x <= max.x)) {
      const r = this.radius;
      return { min: new Vector3(-r, -r, -r), max: new Vector3(r, r, r) };
    }
    // 通り芯・階の線と名前も収める（端の名前が画面の外に出ないように）
    const decorations = [
      this.options.axes ? this.axisObjects?.line : null,
      this.options.stories ? this.storyObjects?.lines : null,
    ];
    for (const object of decorations) {
      if (!object) continue;
      object.geometry.computeBoundingBox();
      const box = object.geometry.boundingBox;
      if (!box || box.isEmpty()) continue;
      min.min(box.min);
      max.max(box.max);
    }
    return { min, max };
  }

  /**
   * direction の側から、見えている部材の外接箱がちょうど収まるようにカメラを置く。
   * 箱の 8 隅を画面の向きに投影して、縦・横のどちらで決まるかを調べる。
   */
  private place(direction: Vector3) {
    const { min, max } = this.visibleBox();
    const center = min.clone().add(max).multiplyScalar(0.5);
    const forward = direction.clone().negate().normalize();
    let up = new Vector3(0, 0, 1).addScaledVector(forward, -forward.z);
    if (up.lengthSq() < 1e-10) up = new Vector3(0, 1, 0).addScaledVector(forward, -forward.y);
    up.normalize();
    const right = forward.clone().cross(up).normalize();
    const aspect = this.width / this.height;
    const tanV = Math.tan((FOV * Math.PI) / 360);
    const tanH = tanV * aspect;
    let halfW = 0;
    let halfH = 0;
    let distance = 0;
    for (let i = 0; i < 8; i++) {
      const corner = new Vector3(i & 1 ? max.x : min.x, i & 2 ? max.y : min.y, i & 4 ? max.z : min.z).sub(center);
      const x = Math.abs(corner.dot(right));
      const y = Math.abs(corner.dot(up));
      const z = corner.dot(forward);
      halfW = Math.max(halfW, x);
      halfH = Math.max(halfH, y);
      distance = Math.max(distance, y / tanV - z, x / tanH - z);
    }
    const margin = 1.06;
    const size = Math.max(min.distanceTo(max), 300);
    this.controls.target.copy(center);
    if (this.camera === this.perspective) {
      const d = Math.max(distance * margin, size * 0.2);
      this.perspective.position.copy(center).addScaledVector(direction, d);
      this.perspective.near = Math.max(d / 500, 1);
      this.perspective.far = d + this.radius * 20;
      this.perspective.updateProjectionMatrix();
    } else {
      this.orthoHalfHeight = Math.max(halfH, halfW / aspect, 150) * margin;
      this.orthographic.zoom = 1;
      this.orthographic.position.copy(center).addScaledVector(direction, this.radius * 6);
      this.orthographic.near = 1;
      this.orthographic.far = this.radius * 20;
      this.updateProjection();
    }
    this.camera.lookAt(center);
    this.controls.update();
    this.requestRender();
  }

  viewFrom(viewpoint: Viewpoint): void {
    this.place(unit(VIEW_DIRECTIONS[viewpoint]));
  }

  /** 通り芯に直交する向きから見る（軸組の確認）。X 方向の通りは −Y 側、Y 方向の通りは +X 側から見る */
  viewAxis(axis: GridAxis): void {
    if (isArcAxis(axis)) {
      this.viewFrom("top");
      return;
    }
    const dx = axis.end[0] - axis.start[0];
    const dy = axis.end[1] - axis.start[1];
    let n = new Vector3(-dy, dx, 0).normalize();
    if (Math.abs(dx) >= Math.abs(dy) ? n.y > 0 : n.x < 0) n = n.negate();
    this.place(n);
  }

  /** 見えている部材の全体を、今の向きのまま収める */
  fit(): void {
    const direction = this.camera.position.clone().sub(this.controls.target).normalize();
    this.place(direction.lengthSq() > 0 ? direction : unit(VIEW_DIRECTIONS.iso));
  }

  /** 今のカメラ（モデルの座標） */
  getCamera(): CameraState {
    const p = this.camera.position;
    const t = this.controls.target;
    const [ox, oy, oz] = this.origin;
    const span =
      this.camera === this.perspective
        ? 2 * p.distanceTo(t) * Math.tan((FOV * Math.PI) / 360)
        : (2 * this.orthoHalfHeight) / this.orthographic.zoom;
    return { position: [p.x + ox, p.y + oy, p.z + oz], target: [t.x + ox, t.y + oy, t.z + oz], span };
  }

  /**
   * カメラを置く（省略したものは今のまま）。透視投影では position を渡せば span は使わず、
   * 渡さなければ注視点からの距離を span に合わせる。平行投影では span が見える高さになる
   */
  setCamera(camera: Partial<CameraState>): void {
    const [ox, oy, oz] = this.origin;
    const target = camera.target
      ? new Vector3(camera.target[0] - ox, camera.target[1] - oy, camera.target[2] - oz)
      : this.controls.target.clone();
    let position = camera.position
      ? new Vector3(camera.position[0] - ox, camera.position[1] - oy, camera.position[2] - oz)
      : this.camera.position.clone().sub(this.controls.target).add(target);
    const tan = Math.tan((FOV * Math.PI) / 360);
    if (this.camera === this.perspective) {
      if (!camera.position && camera.span !== undefined && camera.span > 0) {
        const direction = position.clone().sub(target).normalize();
        position = target.clone().addScaledVector(direction, camera.span / 2 / tan);
      }
      const distance = position.distanceTo(target);
      this.perspective.position.copy(position);
      this.perspective.near = Math.max(distance / 500, 1);
      this.perspective.far = distance + this.radius * 20;
      this.perspective.updateProjectionMatrix();
    } else {
      if (camera.span !== undefined && camera.span > 0) {
        this.orthoHalfHeight = camera.span / 2;
        this.orthographic.zoom = 1;
        this.updateProjection();
      }
      this.orthographic.position.copy(position);
    }
    this.controls.target.copy(target);
    this.camera.lookAt(target);
    this.controls.update();
    // OrbitControls は位置を球座標から計算し直すので、渡された位置に戻す（getView の値で同じカメラに戻れるように）
    this.camera.position.copy(position);
    this.camera.lookAt(target);
    this.requestRender();
  }

  get projection(): Projection {
    return this.camera === this.perspective ? "perspective" : "orthographic";
  }

  setProjection(projection: Projection): void {
    if (projection === this.projection) return;
    const target = this.controls.target.clone();
    const offset = this.camera.position.clone().sub(target);
    if (projection === "orthographic") {
      const distance = offset.length();
      this.orthoHalfHeight = distance * Math.tan((FOV * Math.PI) / 360);
      this.orthographic.zoom = 1;
      this.orthographic.position.copy(target).addScaledVector(offset.normalize(), this.radius * 6);
      this.orthographic.near = 1;
      this.orthographic.far = this.radius * 20;
      this.camera = this.orthographic;
    } else {
      const distance = this.orthoHalfHeight / this.orthographic.zoom / Math.tan((FOV * Math.PI) / 360);
      this.perspective.position.copy(target).addScaledVector(offset.normalize(), distance);
      this.perspective.near = Math.max(distance / 500, 1);
      this.perspective.far = distance + this.radius * 20;
      this.camera = this.perspective;
    }
    this.updateProjection();
    this.controls.dispose();
    this.controls = this.createControls();
    this.controls.target.copy(target);
    this.camera.lookAt(target);
    this.controls.update();
    this.syncLight();
    this.requestRender();
  }

  /** 画面の画像（PNG の data URL） */
  snapshot(): string {
    this.renderer.render(this.scene, this.camera);
    return this.canvas.toDataURL("image/png");
  }

  dispose(): void {
    cancelAnimationFrame(this.frame);
    this.controls.dispose();
    this.clearModel();
    for (const material of Object.values(this.materials) as Material[]) material.dispose();
    this.renderer.dispose();
    this.canvas.remove();
    this.labelLayer.remove();
  }
}
