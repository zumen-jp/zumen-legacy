import {
  type GridAxis,
  MEMBER_KINDS,
  MEMBER_LABELS,
  type MemberKind,
  type StructureMember,
  type StructureModel,
} from "@zumenjs/stb";
import { useStructureState, useStructureViewer } from "@zumenjs/stb/react";
import type { StructureGuides, Viewpoint } from "@zumenjs/stb/three";
import { useEffect, useId, useMemo, useRef, useState } from "react";
import "./structure-sample.css";

export interface StructureSampleProps {
  model: StructureModel;
  /** モデルの名前。見出しと読み上げに使う */
  label?: string;
}

type Toggle = "edges" | "translucent" | keyof StructureGuides;

const TOGGLES: { key: Toggle; label: string }[] = [
  { key: "edges", label: "稜線" },
  { key: "translucent", label: "床・壁を透かす" },
  { key: "axes", label: "通り芯" },
  { key: "stories", label: "階" },
  { key: "labels", label: "名前" },
  { key: "nodes", label: "節点" },
];

const VIEWPOINTS: { key: Viewpoint; label: string; title: string }[] = [
  { key: "iso", label: "3D", title: "斜めから見る" },
  { key: "top", label: "平面", title: "真上から見る（伏図）" },
  { key: "front", label: "正面", title: "−Y 側から見る（X 方向の軸組）" },
  { key: "side", label: "側面", title: "+X 側から見る（Y 方向の軸組）" },
];

const STRUCTURE_LEGEND = ["RC", "S", "SRC", "CFT", "DECK", "PRECAST", "PC", "LOAD", "UNDEFINED"];
const NO_MEMBERS: readonly StructureMember[] = [];
const NO_KINDS: ReadonlySet<MemberKind> = new Set();

/**
 * 構造モデル（ST-Bridge）の 3D の画面のサンプル。@zumenjs/stb/react のフックと @zumenjs/stb/three の公開された API だけで、
 * 操作盤・表示の絞り込み・属性・状態の欄を作る。文言と CSS（structure-sample.css）はこちらが持ち、モデルの表示・選択・
 * カメラの操作はエンジンが受け持つ
 */
export function StructureSample({ model, label }: StructureSampleProps) {
  const rootRef = useRef<HTMLElement>(null);
  const titleId = useId();
  const filtersId = useId();
  const propertiesId = useId();
  const [showFilters, setShowFilters] = useState(true);
  const [showProperties, setShowProperties] = useState(true);

  const name = label ?? (model.metadata.find((item) => item.label === "プロジェクト")?.value || "構造モデル");
  const description = `${name}。部材 ${model.members.length.toLocaleString("ja-JP")} 件。ドラッグで回転、右ドラッグで移動、ホイールで拡大・縮小、矢印キーで移動、0 で全体表示、Esc で選択を解除`;
  const { ref, viewer } = useStructureViewer(model, { label: description });
  const current = useStructureState(viewer, (state) => state.model) ?? model;
  const hiddenKinds = useStructureState(viewer, (state) => state.hiddenKinds) ?? NO_KINDS;
  const story = useStructureState(viewer, (state) => state.story) ?? null;
  const axis = useStructureState(viewer, (state) => state.axis) ?? null;
  const visibleCount = useStructureState(viewer, (state) => state.visibleCount) ?? model.members.length;
  const selection = useStructureState(viewer, (state) => state.selection) ?? NO_MEMBERS;
  const projection = useStructureState(viewer, (state) => state.projection) ?? "perspective";
  const colorMode = useStructureState(viewer, (state) => state.colorMode) ?? "kind";
  const edges = useStructureState(viewer, (state) => state.edges) ?? true;
  const translucent = useStructureState(viewer, (state) => state.translucent) ?? true;
  const guides = useStructureState(viewer, (state) => state.guides);
  const style = viewer?.getStyle();
  // 属性の欄は、最後に選んだ部材を示す
  const selected = selection[selection.length - 1] ?? null;
  const toggles: Record<Toggle, boolean> = {
    edges,
    translucent,
    axes: guides?.axes ?? true,
    stories: guides?.stories ?? true,
    labels: guides?.labels ?? true,
    nodes: guides?.nodes ?? false,
  };

  const counts = useMemo(() => {
    const map = new Map<MemberKind, number>();
    for (const member of current.members) map.set(member.kind, (map.get(member.kind) ?? 0) + 1);
    return map;
  }, [current]);
  const axisGroups = useMemo(() => {
    const groups = new Map<string, GridAxis[]>();
    for (const item of current.axes) groups.set(item.group, [...(groups.get(item.group) ?? []), item]);
    return [...groups.entries()];
  }, [current]);

  // 幅の狭い画面では、最初は欄を閉じてモデルを広く見せる
  useEffect(() => {
    if ((rootRef.current?.getBoundingClientRect().width ?? 1024) < 760) {
      setShowFilters(false);
      setShowProperties(false);
    }
  }, []);

  const toggle = (key: Toggle) => {
    if (!viewer) return;
    if (key === "edges") viewer.setEdges(!edges);
    else if (key === "translucent") viewer.setTranslucent(!translucent);
    else viewer.setGuides({ [key]: !toggles[key] });
  };

  const presentKinds = MEMBER_KINDS.filter((kind) => counts.has(kind));
  const storyName = story === null ? null : (current.stories.find((item) => item.id === story)?.name ?? null);

  return (
    <section ref={rootRef} className="structure" aria-labelledby={titleId}>
      <div className="structure-toolbar">
        <h2 id={titleId} className="structure-title">
          {name}
        </h2>
        <fieldset className="structure-group">
          <legend className="structure-visually-hidden">視点</legend>
          <button
            type="button"
            className="structure-button"
            onClick={() => viewer?.fit()}
            title="見えている部材の全体を表示（0）"
          >
            全体
          </button>
          {VIEWPOINTS.map((item) => (
            <button
              key={item.key}
              type="button"
              className="structure-button"
              title={item.title}
              onClick={() => viewer?.viewFrom(item.key)}
            >
              {item.label}
            </button>
          ))}
        </fieldset>
        <fieldset className="structure-group">
          <legend className="structure-visually-hidden">投影</legend>
          <button
            type="button"
            className="structure-button"
            aria-pressed={projection === "perspective"}
            onClick={() => viewer?.setProjection("perspective")}
          >
            透視
          </button>
          <button
            type="button"
            className="structure-button"
            aria-pressed={projection === "orthographic"}
            onClick={() => viewer?.setProjection("orthographic")}
          >
            平行
          </button>
        </fieldset>
        <fieldset className="structure-group">
          <legend className="structure-visually-hidden">色分け</legend>
          <button
            type="button"
            className="structure-button"
            aria-pressed={colorMode === "kind"}
            onClick={() => viewer?.setColorMode("kind")}
          >
            部材の色
          </button>
          <button
            type="button"
            className="structure-button"
            aria-pressed={colorMode === "structure"}
            onClick={() => viewer?.setColorMode("structure")}
          >
            構造種別の色
          </button>
        </fieldset>
        <fieldset className="structure-group">
          <legend className="structure-visually-hidden">見え方</legend>
          {TOGGLES.map((item) => (
            <button
              key={item.key}
              type="button"
              className="structure-button"
              aria-pressed={toggles[item.key]}
              onClick={() => toggle(item.key)}
            >
              {item.label}
            </button>
          ))}
        </fieldset>
        <fieldset className="structure-group">
          <legend className="structure-visually-hidden">欄</legend>
          <button
            type="button"
            className="structure-button"
            aria-expanded={showFilters}
            aria-controls={filtersId}
            onClick={() => setShowFilters((value) => !value)}
          >
            表示
          </button>
          <button
            type="button"
            className="structure-button"
            aria-expanded={showProperties}
            aria-controls={propertiesId}
            onClick={() => setShowProperties((value) => !value)}
          >
            属性
          </button>
        </fieldset>
      </div>
      <div className="structure-body">
        <aside id={filtersId} className="structure-panel structure-panel-start" hidden={!showFilters} aria-label="表示">
          <h3>部材</h3>
          <div className="structure-row">
            <button type="button" className="structure-button" onClick={() => viewer?.setHiddenKinds([])}>
              すべて表示
            </button>
            <button type="button" className="structure-button" onClick={() => viewer?.setHiddenKinds(presentKinds)}>
              すべて隠す
            </button>
          </div>
          {presentKinds.length === 0 ? <p className="structure-hint">部材はありません。</p> : null}
          <ul className="structure-list">
            {presentKinds.map((kind) => {
              const id = `${filtersId}-${kind}`;
              return (
                <li key={kind} className="structure-check">
                  <input
                    id={id}
                    type="checkbox"
                    checked={!hiddenKinds.has(kind)}
                    onChange={(event) => viewer?.setKindsVisible([kind], event.currentTarget.checked)}
                  />
                  {colorMode === "kind" && style ? (
                    <span className="structure-swatch" style={{ background: style.kinds[kind] }} aria-hidden="true" />
                  ) : null}
                  <label htmlFor={id}>{MEMBER_LABELS[kind]}</label>
                  <span className="structure-count">{(counts.get(kind) ?? 0).toLocaleString("ja-JP")}</span>
                </li>
              );
            })}
          </ul>
          {colorMode === "structure" && style ? (
            <ul className="structure-list" aria-label="構造種別の色">
              {STRUCTURE_LEGEND.filter((key) => current.members.some((member) => member.structure === key)).map(
                (key) => (
                  <li key={key} className="structure-check">
                    <span
                      className="structure-swatch"
                      style={{ background: style.structures[key] ?? style.structureFallback }}
                      aria-hidden="true"
                    />
                    <span>{key}</span>
                  </li>
                ),
              )}
            </ul>
          ) : null}

          <h3>階</h3>
          {current.stories.length === 0 ? <p className="structure-hint">階の情報はありません。</p> : null}
          {current.stories.length > 0 ? (
            <div className="structure-chips">
              <button
                type="button"
                className="structure-button"
                aria-pressed={story === null}
                onClick={() => viewer?.focusStory(null)}
              >
                全階
              </button>
              {[...current.stories].reverse().map((item) => (
                <button
                  key={item.id}
                  type="button"
                  className="structure-button"
                  aria-pressed={story === item.id}
                  title={`${item.name}（${item.height.toLocaleString("ja-JP")} mm）の部材を真上から見る`}
                  onClick={() => viewer?.focusStory(story === item.id ? null : item.id)}
                >
                  {item.name}
                </button>
              ))}
            </div>
          ) : null}

          <h3>通り芯</h3>
          {axisGroups.length === 0 ? <p className="structure-hint">通り芯の情報はありません。</p> : null}
          {axisGroups.map(([group, axes]) => (
            <div key={group}>
              <p className="structure-chip-group">{group || "（グループ名なし）"}</p>
              <div className="structure-chips">
                {axes.map((item) => (
                  <button
                    key={item.key}
                    type="button"
                    className="structure-button"
                    aria-pressed={axis?.key === item.key}
                    title={`${item.name} 通りの部材を、通りに直交する向きから見る`}
                    onClick={() => viewer?.focusAxis(axis?.key === item.key ? null : item.key)}
                  >
                    {item.name}
                  </button>
                ))}
              </div>
            </div>
          ))}
        </aside>
        <div ref={ref} className="structure-stage" />
        <aside
          id={propertiesId}
          className="structure-panel structure-panel-end"
          hidden={!showProperties}
          aria-label="属性"
        >
          {selected ? (
            <>
              <h3>
                {selected.name ? `${MEMBER_LABELS[selected.kind]} ${selected.name}` : MEMBER_LABELS[selected.kind]}
              </h3>
              <dl className="structure-properties">
                {selected.properties.map((property) => (
                  <PropertyRow
                    key={`${property.label}:${property.value}`}
                    label={property.label}
                    value={property.value}
                  />
                ))}
              </dl>
              <details className="structure-details">
                <summary>元の属性（XML）</summary>
                <div className="structure-raw">
                  {selected.raw.map((raw) => (
                    <section key={raw.label}>
                      <h4>{raw.label}</h4>
                      <dl className="structure-properties">
                        {Object.entries(raw.attributes).map(([key, value]) => (
                          <PropertyRow key={key} label={key} value={value} />
                        ))}
                      </dl>
                    </section>
                  ))}
                </div>
              </details>
            </>
          ) : (
            <>
              <h3>モデルの情報</h3>
              <dl className="structure-properties">
                {current.metadata.map((property) => (
                  <PropertyRow
                    key={`${property.label}:${property.value}`}
                    label={property.label}
                    value={property.value}
                  />
                ))}
              </dl>
              <p className="structure-hint">部材を選ぶと、符号・断面・材料などの属性を表示します。</p>
            </>
          )}
          {current.notices.length > 0 ? (
            <details className="structure-details">
              <summary>読み取りの注意（{current.notices.length}）</summary>
              <ul>
                {current.notices.map((notice) => (
                  <li key={notice.code} className={notice.level === "warning" ? "structure-notice-warning" : undefined}>
                    {notice.message}（{notice.count.toLocaleString("ja-JP")} 件）
                  </li>
                ))}
              </ul>
            </details>
          ) : null}
        </aside>
      </div>
      <div className="structure-status">
        <span>
          表示中 <strong>{visibleCount.toLocaleString("ja-JP")}</strong> /{" "}
          {current.members.length.toLocaleString("ja-JP")} 部材
        </span>
        {storyName ? (
          <span>
            階 <strong>{storyName}</strong>
          </span>
        ) : null}
        {axis ? (
          <span>
            通り <strong>{axis.name}</strong>
          </span>
        ) : null}
        <output aria-live="polite">
          {selected
            ? `選択中: ${selection.length > 1 ? `${selection.length.toLocaleString("ja-JP")} 件（最後に選んだもの: ` : ""}${MEMBER_LABELS[selected.kind]} ${selected.name || `ID ${selected.id}`}${selected.section ? `（${selected.section.shapes[0] ?? selected.section.name}）` : ""}${selection.length > 1 ? "）" : ""}`
            : "部材を選ぶと属性を表示します（Esc で解除）"}
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
