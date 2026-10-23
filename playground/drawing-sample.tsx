import {
  createMarkup,
  type MarkupDocument,
  type MarkupDrawing,
  type Scene,
  type SceneEntity,
  type Underlay,
} from "@zumenjs/core";
import { useDrawingViewer, useMarkupEvent, useMarkupLayer, useViewerEvent, useViewerState } from "@zumenjs/react";
import type { DrawingViewer, UnderlayRenderer } from "@zumenjs/viewer";
import { useEffect, useId, useMemo, useRef, useState } from "react";
import { LayerList } from "./layer-list.js";
import { MarkupList, MarkupToolbar } from "./markup-sample.js";
import "./drawing-sample.css";

export interface DrawingSampleProps {
  scene: Scene;
  /** 図面の名前。見出しと読み上げに使う */
  label?: string;
  /** 表示（DXF のモデル空間・レイアウトなど）を選んだとき。選べる表示が 2 つ以上ある図面で選ぶ欄を出す */
  onViewChange?: (view: string) => void;
  /** 図面の underlay（下敷きの画像・PDF のページ）を描く処理。省略すると underlay は描かない */
  underlayRenderer?: UnderlayRenderer | undefined;
  /** マークアップを付ける図面（ファイルのバイトの SHA-256 と形式）。null の間はマークアップの道具を出さない */
  markupDrawing?: MarkupDrawing | null;
}

const NO_ENTITIES: readonly SceneEntity[] = [];
const NO_LAYERS: ReadonlySet<number> = new Set();

function formatLength(value: number): string {
  if (!Number.isFinite(value)) return "-";
  const digits = value >= 1000 ? 0 : value >= 10 ? 1 : 2;
  return value.toLocaleString("ja-JP", { minimumFractionDigits: digits, maximumFractionDigits: digits });
}

/** 倍率の表示。1% 以上は 1% 単位、それより小さい倍率（実寸のモデル空間を全体表示したときなど）は有効数字 2 桁 */
function formatZoom(zoom: number): string {
  if (zoom >= 0.01) return `${Math.round(zoom * 100)}%`;
  return zoom > 0 ? `${Number((zoom * 100).toPrecision(2))}%` : "";
}

/**
 * 2D の図面の画面のサンプル。@zumenjs/react のフックと @zumenjs/viewer の公開された API だけで、操作盤・レイヤの一覧・
 * 属性・状態の欄を作る。文言と CSS（drawing-sample.css）はこちらが持ち、図面の表示と操作はエンジンが受け持つ
 */
export function DrawingSample({
  scene,
  label,
  onViewChange,
  underlayRenderer,
  markupDrawing = null,
}: DrawingSampleProps) {
  const rootRef = useRef<HTMLElement>(null);
  const titleId = useId();
  const viewId = useId();
  const layersId = useId();
  const propertiesId = useId();
  const [showLayers, setShowLayers] = useState(true);
  const [showProperties, setShowProperties] = useState(true);

  const name = label ?? scene.sheet?.name ?? "図面";
  const sheetLabel = scene.sheet ? `${scene.sheet.size}` : "用紙なし";
  const canvasLabel = `${name}。${sheetLabel}、${scene.entities.length.toLocaleString("ja-JP")} 要素。矢印キーで移動、+ と - で拡大・縮小、0 で全体表示`;
  const { ref, viewer } = useDrawingViewer(scene, { label: canvasLabel });
  const current = useViewerState(viewer, (state) => state.scene) ?? scene;
  const tool = useViewerState(viewer, (state) => state.tool) ?? "select";
  const background = useViewerState(viewer, (state) => state.background) ?? scene.background;
  const lineWeights = useViewerState(viewer, (state) => state.lineWeights) ?? true;
  const hiddenLayers = useViewerState(viewer, (state) => state.hiddenLayers) ?? NO_LAYERS;
  const selection = useViewerState(viewer, (state) => state.selection) ?? NO_ENTITIES;
  // 属性の欄は、最後に選んだ図形を示す
  const selected = selection[selection.length - 1] ?? null;
  // 描き手が描けなかった underlay。描き手を替えたら数え直す
  const [failed, setFailed] = useState<ReadonlySet<Underlay>>(new Set());
  useEffect(() => {
    viewer?.setUnderlayRenderer(underlayRenderer ?? null);
    setFailed(new Set());
  }, [viewer, underlayRenderer]);
  useViewerEvent(viewer, "underlay", ({ underlay, error }) => {
    if (error !== null) setFailed((underlays) => new Set(underlays).add(underlay));
  });
  const failedCount = current.underlays?.filter((underlay) => failed.has(underlay)).length ?? 0;

  // マークアップの文書。図面のファイルが替わったら、その図面の空の文書にし、層の change で受けた文書を持つ
  const [markup, setMarkup] = useState<MarkupDocument | null>(null);
  useEffect(() => {
    setMarkup(markupDrawing ? createMarkup(markupDrawing) : null);
  }, [markupDrawing]);
  const markupLayer = useMarkupLayer(viewer, { document: markup, author: "playground" });
  useMarkupEvent(markupLayer, "change", (event) => setMarkup(event.document));

  const layerCounts = useMemo(() => {
    const counts = new Map<number, number>();
    for (const entity of current.entities) {
      const own = entity.layers[entity.layers.length - 1];
      if (own !== undefined) counts.set(own, (counts.get(own) ?? 0) + 1);
    }
    return counts;
  }, [current]);

  // 幅の狭い画面では、最初はレイヤ・属性の欄を閉じて図面を広く見せる
  useEffect(() => {
    if ((rootRef.current?.getBoundingClientRect().width ?? 1024) < 760) {
      setShowLayers(false);
      setShowProperties(false);
    }
  }, []);

  return (
    <section ref={rootRef} className="sample" aria-labelledby={titleId}>
      <div className="sample-toolbar">
        <h2 id={titleId} style={{ margin: 0, fontSize: 14, fontWeight: 700 }}>
          {name}
        </h2>
        {onViewChange && scene.views && scene.views.length > 1 ? (
          <div className="sample-group">
            <label htmlFor={viewId} className="sample-label">
              表示
            </label>
            <select
              id={viewId}
              className="sample-select"
              value={scene.view ?? scene.views[0]?.id}
              onChange={(event) => onViewChange(event.currentTarget.value)}
            >
              {scene.views.map((view) => (
                <option key={view.id} value={view.id}>
                  {view.name}
                </option>
              ))}
            </select>
          </div>
        ) : null}
        <fieldset className="sample-group">
          <legend className="sample-visually-hidden">表示の範囲</legend>
          <button type="button" className="sample-button" onClick={() => viewer?.fit()} title="全体を表示（0）">
            全体
          </button>
          <button type="button" className="sample-button" onClick={() => viewer?.zoomBy(1.25)} title="拡大（+）">
            拡大
          </button>
          <button type="button" className="sample-button" onClick={() => viewer?.zoomBy(0.8)} title="縮小（-）">
            縮小
          </button>
          <button type="button" className="sample-button" onClick={() => viewer?.rotateBy(-90)} title="左に 90 度回す">
            左に回転
          </button>
          <button type="button" className="sample-button" onClick={() => viewer?.rotateBy(90)} title="右に 90 度回す">
            右に回転
          </button>
        </fieldset>
        <fieldset className="sample-group">
          <legend className="sample-visually-hidden">操作</legend>
          <button
            type="button"
            className="sample-button"
            aria-pressed={tool === "select"}
            onClick={() => viewer?.setTool("select")}
            title="クリックで選ぶ。Shift を押しながらクリックすると選択に加える"
          >
            選択
          </button>
          <button
            type="button"
            className="sample-button"
            aria-pressed={tool === "measure"}
            onClick={() => viewer?.setTool("measure")}
          >
            計測
          </button>
        </fieldset>
        {markupDrawing ? (
          <MarkupToolbar
            viewer={viewer}
            layer={markupLayer}
            drawing={markupDrawing}
            name={label ?? "drawing"}
            onOpen={setMarkup}
          />
        ) : null}
        <fieldset className="sample-group">
          <legend className="sample-visually-hidden">見え方</legend>
          <button
            type="button"
            className="sample-button"
            aria-pressed={lineWeights}
            onClick={() => viewer?.setLineWeights(!lineWeights)}
          >
            線の太さ
          </button>
          <button
            type="button"
            className="sample-button"
            aria-pressed={background === "light"}
            onClick={() => viewer?.setBackground(background === "dark" ? "light" : "dark")}
          >
            白背景
          </button>
          <button
            type="button"
            className="sample-button"
            aria-expanded={showLayers}
            aria-controls={layersId}
            onClick={() => setShowLayers((value) => !value)}
          >
            レイヤ
          </button>
          <button
            type="button"
            className="sample-button"
            aria-expanded={showProperties}
            aria-controls={propertiesId}
            onClick={() => setShowProperties((value) => !value)}
          >
            属性
          </button>
        </fieldset>
      </div>
      <div className="sample-body">
        <aside id={layersId} className="sample-panel sample-panel-start" hidden={!showLayers} aria-label="レイヤ">
          <h3>レイヤ（{current.layers.length}）</h3>
          <div className="sample-row">
            <button type="button" className="sample-button" onClick={() => viewer?.setHiddenLayers([])}>
              すべて表示
            </button>
            <button
              type="button"
              className="sample-button"
              onClick={() => viewer?.setHiddenLayers(current.layers.map((layer) => layer.id))}
            >
              すべて隠す
            </button>
          </div>
          {current.layers.length === 0 ? <p className="sample-hint">レイヤはありません。</p> : null}
          <LayerList
            layers={current.layers}
            hidden={hiddenLayers}
            counts={layerCounts}
            idPrefix={layersId}
            onChange={(ids, visible) => viewer?.setLayersVisible(ids, visible)}
          />
        </aside>
        <div ref={ref} className="sample-stage" />
        <aside id={propertiesId} className="sample-panel sample-panel-end" hidden={!showProperties} aria-label="属性">
          {markupDrawing ? (
            <MarkupList
              viewer={viewer}
              layer={markupLayer}
              sameDrawing={markup?.drawing.sha256 === markupDrawing.sha256}
            />
          ) : null}
          {selected ? (
            <>
              <h3>{selected.type}</h3>
              <dl className="sample-properties">
                {selected.properties.map((property) => (
                  <PropertyRow
                    key={`${property.label}:${property.value}`}
                    label={property.label}
                    value={property.value}
                  />
                ))}
              </dl>
            </>
          ) : (
            <>
              <h3>図面の情報</h3>
              <dl className="sample-properties">
                {current.metadata.map((property) => (
                  <PropertyRow
                    key={`${property.label}:${property.value}`}
                    label={property.label}
                    value={property.value}
                  />
                ))}
                <PropertyRow label="要素" value={`${current.entities.length.toLocaleString("ja-JP")} 件`} />
                {current.underlays?.length ? (
                  <PropertyRow
                    label="下敷き"
                    value={`${current.underlays.length.toLocaleString("ja-JP")} 件${
                      !underlayRenderer
                        ? "（描いていません）"
                        : failedCount > 0
                          ? `（描けないもの ${failedCount.toLocaleString("ja-JP")} 件）`
                          : ""
                    }`}
                  />
                ) : null}
              </dl>
              <p className="sample-hint">図形を選ぶと属性を表示します。</p>
            </>
          )}
          {current.notices.length > 0 ? (
            <details className="sample-notices">
              <summary>読み取りの注意（{current.notices.length}）</summary>
              <ul>
                {current.notices.map((notice) => (
                  <li key={notice.code}>
                    {notice.message}（{notice.count.toLocaleString("ja-JP")} 件）
                  </li>
                ))}
              </ul>
            </details>
          ) : null}
        </aside>
      </div>
      <StatusBar viewer={viewer} />
    </section>
  );
}

/**
 * 状態の欄。倍率とポインターの下の座標はポインターの速さで変わるので、エンジンのイベントで受け、この欄だけを描き直す
 */
function StatusBar({ viewer }: { viewer: DrawingViewer | null }) {
  const [zoom, setZoom] = useState(0);
  const [cursor, setCursor] = useState<readonly [number, number] | null>(null);
  const tool = useViewerState(viewer, (state) => state.tool);
  const measurement = useViewerState(viewer, (state) => state.measurement) ?? null;
  const distance = useViewerState(viewer, (state) => state.distance) ?? null;
  const selection = useViewerState(viewer, (state) => state.selection) ?? NO_ENTITIES;
  const selected = selection[selection.length - 1];
  useViewerEvent(viewer, "move", ({ view }) => setZoom(view.zoom));
  useViewerEvent(viewer, "cursor", ({ point }) => setCursor(point));
  // 最初の表示範囲は、イベントを受け始める前に決まることがある
  useEffect(() => {
    if (viewer && viewer.getView().width > 0) setZoom(viewer.getView().zoom);
  }, [viewer]);

  return (
    <div className="sample-status">
      <span>
        表示 <strong>{formatZoom(zoom)}</strong>
      </span>
      <span>
        {cursor ? (
          <>
            用紙 X <strong>{formatLength(cursor[0])}</strong> Y <strong>{formatLength(cursor[1])}</strong> mm
          </>
        ) : (
          "図面の上で座標を表示"
        )}
      </span>
      <output aria-live="polite">
        {tool === "measure" ? (
          distance ? (
            <>
              距離 <strong>{formatLength(distance.paper)} mm</strong>（用紙）
              {distance.real !== null ? (
                <>
                  ・<strong>{formatLength(distance.real)} mm</strong>（実寸、{distance.placement}）
                </>
              ) : null}
            </>
          ) : measurement ? (
            "終点を選んでください"
          ) : (
            "始点を選んでください（端点・頂点に吸着）"
          )
        ) : selected ? (
          selection.length > 1 ? (
            `選択中: ${selection.length.toLocaleString("ja-JP")} 件（最後に選んだもの: ${selected.type} ${selected.id}）`
          ) : (
            `選択中: ${selected.type} ${selected.id}`
          )
        ) : null}
      </output>
    </div>
  );
}

function PropertyRow({ label, value }: { label: string; value: string }) {
  return (
    <>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </>
  );
}
