"use client";

import type { Scene, SceneEntity } from "@zumenjs/core";
import { type CSSProperties, useEffect, useId, useMemo, useRef, useState } from "react";
import { LayerList } from "./layer-list.js";
import { CAD_VIEWER_CSS } from "./styles.js";
import { useDrawingViewer } from "./use-drawing-viewer.js";

export interface CadViewerProps {
  scene: Scene;
  /** 背景。省略時は図面の想定に合わせる（SXF は黒） */
  background?: "dark" | "light";
  /** 要素を選んだとき（選択を外したときは null） */
  onSelect?: (entity: SceneEntity | null) => void;
  className?: string;
  style?: CSSProperties;
  /** 図面の名前。画面の読み上げに使う */
  label?: string;
  /**
   * 表示（DXF のモデル空間・レイアウトなど）を選んだとき。渡すと、選べる表示が 2 つ以上ある図面で選ぶ欄を出す。
   * 選んだ id を useCadFile の options.view に渡して読み直す
   */
  onViewChange?: (view: string) => void;
}

function formatLength(value: number): string {
  if (!Number.isFinite(value)) return "-";
  const digits = value >= 1000 ? 0 : value >= 10 ? 1 : 2;
  return value.toLocaleString("ja-JP", { minimumFractionDigits: digits, maximumFractionDigits: digits });
}

/**
 * 図面を表示し、拡大・移動・レイヤの切り替え・要素の確認・距離の計測を行う完成品の画面。
 * 表示と操作は @zumenjs/core の DrawingViewer で、この部品は操作盤・レイヤ・属性・状態の欄を受け持つ。
 */
export function CadViewer({ scene, background, onSelect, className, style, label, onViewChange }: CadViewerProps) {
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
  const { ref, viewer, state } = useDrawingViewer(scene, { theme: background, label: canvasLabel, onSelect });
  const { mode, theme, lineWeights, hiddenLayers, selected, measurement, distance, cursor, zoom } = state;
  const current = state.scene;

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
    <section
      ref={rootRef}
      className={["cadv", className].filter(Boolean).join(" ")}
      style={style}
      aria-labelledby={titleId}
    >
      <style href="zumenjs-react" precedence="default">
        {CAD_VIEWER_CSS}
      </style>
      <div className="cadv-toolbar">
        <h2 id={titleId} style={{ margin: 0, fontSize: 14, fontWeight: 700 }}>
          {name}
        </h2>
        {onViewChange && scene.views && scene.views.length > 1 ? (
          <div className="cadv-group">
            <label htmlFor={viewId} className="cadv-label">
              表示
            </label>
            <select
              id={viewId}
              className="cadv-select"
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
        <fieldset className="cadv-group">
          <legend className="cadv-visually-hidden">表示の範囲</legend>
          <button type="button" className="cadv-button" onClick={() => viewer?.fit()} title="全体を表示（0）">
            全体
          </button>
          <button type="button" className="cadv-button" onClick={() => viewer?.zoomBy(1.25)} title="拡大（+）">
            拡大
          </button>
          <button type="button" className="cadv-button" onClick={() => viewer?.zoomBy(0.8)} title="縮小（-）">
            縮小
          </button>
        </fieldset>
        <fieldset className="cadv-group">
          <legend className="cadv-visually-hidden">操作</legend>
          <button
            type="button"
            className="cadv-button"
            aria-pressed={mode === "select"}
            onClick={() => viewer?.setMode("select")}
          >
            選択
          </button>
          <button
            type="button"
            className="cadv-button"
            aria-pressed={mode === "measure"}
            onClick={() => viewer?.setMode("measure")}
          >
            計測
          </button>
        </fieldset>
        <fieldset className="cadv-group">
          <legend className="cadv-visually-hidden">見え方</legend>
          <button
            type="button"
            className="cadv-button"
            aria-pressed={lineWeights}
            onClick={() => viewer?.setLineWeights(!lineWeights)}
          >
            線の太さ
          </button>
          <button
            type="button"
            className="cadv-button"
            aria-pressed={theme === "light"}
            onClick={() => viewer?.setTheme(theme === "dark" ? "light" : "dark")}
          >
            白背景
          </button>
          <button
            type="button"
            className="cadv-button"
            aria-expanded={showLayers}
            aria-controls={layersId}
            onClick={() => setShowLayers((value) => !value)}
          >
            レイヤ
          </button>
          <button
            type="button"
            className="cadv-button"
            aria-expanded={showProperties}
            aria-controls={propertiesId}
            onClick={() => setShowProperties((value) => !value)}
          >
            属性
          </button>
        </fieldset>
      </div>
      <div className="cadv-body">
        <aside id={layersId} className="cadv-panel cadv-panel-start" hidden={!showLayers} aria-label="レイヤ">
          <h3>レイヤ（{current.layers.length}）</h3>
          <div className="cadv-row">
            <button type="button" className="cadv-button" onClick={() => viewer?.setHiddenLayers([])}>
              すべて表示
            </button>
            <button
              type="button"
              className="cadv-button"
              onClick={() => viewer?.setHiddenLayers(current.layers.map((layer) => layer.id))}
            >
              すべて隠す
            </button>
          </div>
          {current.layers.length === 0 ? <p className="cadv-hint">レイヤはありません。</p> : null}
          <LayerList
            layers={current.layers}
            hidden={hiddenLayers}
            counts={layerCounts}
            idPrefix={layersId}
            onChange={(ids, visible) => viewer?.setLayersVisible(ids, visible)}
          />
        </aside>
        <div ref={ref} className="cadv-stage" />
        <aside id={propertiesId} className="cadv-panel cadv-panel-end" hidden={!showProperties} aria-label="属性">
          {selected ? (
            <>
              <h3>{selected.type}</h3>
              <dl className="cadv-properties">
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
              <dl className="cadv-properties">
                {current.metadata.map((property) => (
                  <PropertyRow
                    key={`${property.label}:${property.value}`}
                    label={property.label}
                    value={property.value}
                  />
                ))}
                <PropertyRow label="要素" value={`${current.entities.length.toLocaleString("ja-JP")} 件`} />
              </dl>
              <p className="cadv-hint">図形を選ぶと属性を表示します。</p>
            </>
          )}
          {current.notices.length > 0 ? (
            <details className="cadv-notices">
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
      <div className="cadv-status">
        <span>
          表示{" "}
          <strong>
            {zoom >= 0.01 ? `${Math.round(zoom * 100)}%` : zoom > 0 ? `${Number((zoom * 100).toPrecision(2))}%` : ""}
          </strong>
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
          {mode === "measure" ? (
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
            `選択中: ${selected.type} ${selected.id}`
          ) : null}
        </output>
      </div>
    </section>
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
