"use client";

import { type CSSProperties, useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import { isMemberVisible, type MemberFilter, membersOnAxis } from "../filter.js";
import { hex, KIND_COLORS, STRUCTURE_COLORS } from "../three/colors.js";
import { type ColorMode, type Projection, StructureView, type ViewOptions, type Viewpoint } from "../three/view.js";
import {
  type GridAxis,
  MEMBER_KINDS,
  MEMBER_LABELS,
  type MemberKind,
  type StructureMember,
  type StructureModel,
} from "../types.js";
import { STB_VIEWER_CSS } from "./styles.js";

export interface StbViewerProps {
  model: StructureModel;
  /** 部材を選んだとき（選択を外したときは null） */
  onSelect?: (member: StructureMember | null) => void;
  /** 配色。省略時は OS の設定に合わせる */
  theme?: "light" | "dark";
  className?: string;
  style?: CSSProperties;
  /** モデルの名前。見出しと読み上げに使う */
  label?: string;
}

type Toggle = Exclude<keyof ViewOptions, "theme" | "colorMode">;

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

function usePrefersDark(): boolean {
  const [dark, setDark] = useState(false);
  useEffect(() => {
    const query = globalThis.matchMedia?.("(prefers-color-scheme: dark)");
    if (!query) return;
    setDark(query.matches);
    const listener = (event: MediaQueryListEvent) => setDark(event.matches);
    query.addEventListener("change", listener);
    return () => query.removeEventListener("change", listener);
  }, []);
  return dark;
}

/** ST-Bridge の構造モデルを 3D で表示し、部材の選択・属性の確認・表示の切り替え（種類・階・通り芯）を行う */
export function StbViewer({ model, onSelect, theme, className, style, label }: StbViewerProps) {
  const stageRef = useRef<HTMLDivElement>(null);
  const rootRef = useRef<HTMLElement>(null);
  const viewRef = useRef<StructureView | null>(null);
  const pointerRef = useRef<{ x: number; y: number; moved: boolean } | null>(null);
  const titleId = useId();
  const filtersId = useId();
  const propertiesId = useId();
  const prefersDark = usePrefersDark();
  const resolvedTheme = theme ?? (prefersDark ? "dark" : "light");

  const [hiddenKinds, setHiddenKinds] = useState<ReadonlySet<MemberKind>>(new Set());
  const [story, setStory] = useState<number | null>(null);
  const [axisKey, setAxisKey] = useState<string | null>(null);
  const [selected, setSelected] = useState<StructureMember | null>(null);
  const [colorMode, setColorMode] = useState<ColorMode>("kind");
  const [toggles, setToggles] = useState<Record<Toggle, boolean>>({
    edges: true,
    translucent: true,
    axes: true,
    stories: true,
    labels: true,
    nodes: false,
  });
  const [projection, setProjection] = useState<Projection>("perspective");
  const [showFilters, setShowFilters] = useState(true);
  const [showProperties, setShowProperties] = useState(true);
  const [dragging, setDragging] = useState(false);

  const counts = useMemo(() => {
    const map = new Map<MemberKind, number>();
    for (const member of model.members) map.set(member.kind, (map.get(member.kind) ?? 0) + 1);
    return map;
  }, [model]);
  const axisGroups = useMemo(() => {
    const groups = new Map<string, GridAxis[]>();
    for (const axis of model.axes) groups.set(axis.group, [...(groups.get(axis.group) ?? []), axis]);
    return [...groups.entries()];
  }, [model]);
  const axis = useMemo(() => model.axes.find((item) => item.key === axisKey) ?? null, [model, axisKey]);
  const onAxis = useMemo(() => (axis ? membersOnAxis(model, axis) : null), [model, axis]);
  const filter = useMemo<MemberFilter>(
    () => ({ hiddenKinds, stories: story === null ? null : new Set([story]), only: onAxis }),
    [hiddenKinds, story, onAxis],
  );
  const visibleCount = useMemo(
    () => model.members.filter((member) => isMemberVisible(member, filter)).length,
    [model, filter],
  );

  const select = useCallback(
    (member: StructureMember | null) => {
      setSelected(member);
      viewRef.current?.setSelection(member?.key ?? null);
      onSelect?.(member);
    },
    [onSelect],
  );

  // 最新の選択の処理（描画の canvas に直接付けたイベントから呼ぶ）
  const selectRef = useRef(select);
  selectRef.current = select;

  // three.js の描画を作る。React の StrictMode の二重実行でも、前のものを破棄してから作る。
  // canvas のイベントは描画の部品が持つ canvas に直接付ける
  useEffect(() => {
    const stage = stageRef.current;
    if (!stage) return;
    const view = new StructureView(stage);
    const canvas = view.canvas;
    canvas.tabIndex = 0;
    canvas.setAttribute("role", "img");
    viewRef.current = view;
    const onPointerDown = (event: PointerEvent) => {
      pointerRef.current = { x: event.clientX, y: event.clientY, moved: false };
    };
    const onPointerMove = (event: PointerEvent) => {
      const pointer = pointerRef.current;
      if (!pointer || pointer.moved) return;
      if (Math.hypot(event.clientX - pointer.x, event.clientY - pointer.y) > 4) {
        pointer.moved = true;
        setDragging(true);
      }
    };
    const onPointerUp = (event: PointerEvent) => {
      const pointer = pointerRef.current;
      pointerRef.current = null;
      setDragging(false);
      if (!pointer || pointer.moved || event.button !== 0) return;
      selectRef.current(view.pick(event.clientX, event.clientY));
    };
    const onPointerCancel = () => {
      pointerRef.current = null;
      setDragging(false);
    };
    const onContextMenu = (event: Event) => event.preventDefault();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        selectRef.current(null);
        event.preventDefault();
      } else if (event.key === "0" || event.key === "f") {
        view.fit();
        event.preventDefault();
      }
    };
    canvas.addEventListener("pointerdown", onPointerDown);
    canvas.addEventListener("pointermove", onPointerMove);
    canvas.addEventListener("pointerup", onPointerUp);
    canvas.addEventListener("pointercancel", onPointerCancel);
    canvas.addEventListener("contextmenu", onContextMenu);
    canvas.addEventListener("keydown", onKeyDown);
    return () => {
      canvas.removeEventListener("pointerdown", onPointerDown);
      canvas.removeEventListener("pointermove", onPointerMove);
      canvas.removeEventListener("pointerup", onPointerUp);
      canvas.removeEventListener("pointercancel", onPointerCancel);
      canvas.removeEventListener("contextmenu", onContextMenu);
      canvas.removeEventListener("keydown", onKeyDown);
      view.dispose();
      viewRef.current = null;
    };
  }, []);

  useEffect(() => {
    viewRef.current?.setOptions({ theme: resolvedTheme });
  }, [resolvedTheme]);

  // モデルが変わったら表示の状態を初期化する
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    setHiddenKinds(new Set());
    setStory(null);
    setAxisKey(null);
    setSelected(null);
    setProjection("perspective");
    view.setProjection("perspective");
    view.setFilter({});
    view.setModel(model);
  }, [model]);

  useEffect(() => {
    viewRef.current?.setFilter(filter, axis);
  }, [filter, axis]);

  // 絞り込みで隠れた部材の選択は外す（属性の欄に見えない部材を残さない）
  useEffect(() => {
    if (selected && !isMemberVisible(selected, filter)) select(null);
  }, [filter, selected, select]);

  useEffect(() => {
    viewRef.current?.setOptions({ colorMode, ...toggles });
  }, [colorMode, toggles]);

  // 幅の狭い画面では、最初は欄を閉じてモデルを広く見せる
  useEffect(() => {
    if ((rootRef.current?.getBoundingClientRect().width ?? 1024) < 760) {
      setShowFilters(false);
      setShowProperties(false);
    }
  }, []);

  const changeProjection = (next: Projection) => {
    setProjection(next);
    viewRef.current?.setProjection(next);
  };

  const viewFrom = (viewpoint: Viewpoint) => viewRef.current?.viewFrom(viewpoint);

  const chooseStory = (id: number | null) => {
    setStory(id);
    setAxisKey(null);
    requestAnimationFrame(() => {
      const view = viewRef.current;
      if (!view) return;
      if (id === null) view.viewFrom("iso");
      else {
        view.setProjection("orthographic");
        setProjection("orthographic");
        view.viewFrom("top");
      }
    });
  };

  const chooseAxis = (next: GridAxis | null) => {
    setAxisKey(next?.key ?? null);
    setStory(null);
    requestAnimationFrame(() => {
      const view = viewRef.current;
      if (!view) return;
      if (!next) view.viewFrom("iso");
      else {
        view.setProjection("orthographic");
        setProjection("orthographic");
        view.viewAxis(next);
      }
    });
  };

  const setKind = (kind: MemberKind, show: boolean) => {
    setHiddenKinds((current) => {
      const next = new Set(current);
      if (show) next.delete(kind);
      else next.add(kind);
      return next;
    });
  };

  const name = label ?? (model.metadata.find((item) => item.label === "プロジェクト")?.value || "構造モデル");
  const description = `${name}。部材 ${model.members.length.toLocaleString("ja-JP")} 件。ドラッグで回転、右ドラッグで移動、ホイールで拡大・縮小、矢印キーで移動、0 で全体表示、Esc で選択を解除`;

  useEffect(() => {
    viewRef.current?.canvas.setAttribute("aria-label", description);
  }, [description]);
  const presentKinds = MEMBER_KINDS.filter((kind) => counts.has(kind));
  const storyName = story === null ? null : (model.stories.find((item) => item.id === story)?.name ?? null);

  return (
    <section
      ref={rootRef}
      className={["stbv", className].filter(Boolean).join(" ")}
      style={style}
      aria-labelledby={titleId}
      data-theme={resolvedTheme}
    >
      <style href="zumenjs-stb" precedence="default">
        {STB_VIEWER_CSS}
      </style>
      <div className="stbv-toolbar">
        <h2 id={titleId} className="stbv-title">
          {name}
        </h2>
        <fieldset className="stbv-group">
          <legend className="stbv-visually-hidden">視点</legend>
          <button
            type="button"
            className="stbv-button"
            onClick={() => viewRef.current?.fit()}
            title="見えている部材の全体を表示（0）"
          >
            全体
          </button>
          {VIEWPOINTS.map((item) => (
            <button
              key={item.key}
              type="button"
              className="stbv-button"
              title={item.title}
              onClick={() => viewFrom(item.key)}
            >
              {item.label}
            </button>
          ))}
        </fieldset>
        <fieldset className="stbv-group">
          <legend className="stbv-visually-hidden">投影</legend>
          <button
            type="button"
            className="stbv-button"
            aria-pressed={projection === "perspective"}
            onClick={() => changeProjection("perspective")}
          >
            透視
          </button>
          <button
            type="button"
            className="stbv-button"
            aria-pressed={projection === "orthographic"}
            onClick={() => changeProjection("orthographic")}
          >
            平行
          </button>
        </fieldset>
        <fieldset className="stbv-group">
          <legend className="stbv-visually-hidden">色分け</legend>
          <button
            type="button"
            className="stbv-button"
            aria-pressed={colorMode === "kind"}
            onClick={() => setColorMode("kind")}
          >
            部材の色
          </button>
          <button
            type="button"
            className="stbv-button"
            aria-pressed={colorMode === "structure"}
            onClick={() => setColorMode("structure")}
          >
            構造種別の色
          </button>
        </fieldset>
        <fieldset className="stbv-group">
          <legend className="stbv-visually-hidden">見え方</legend>
          {TOGGLES.map((item) => (
            <button
              key={item.key}
              type="button"
              className="stbv-button"
              aria-pressed={toggles[item.key]}
              onClick={() => setToggles((current) => ({ ...current, [item.key]: !current[item.key] }))}
            >
              {item.label}
            </button>
          ))}
        </fieldset>
        <fieldset className="stbv-group">
          <legend className="stbv-visually-hidden">欄</legend>
          <button
            type="button"
            className="stbv-button"
            aria-expanded={showFilters}
            aria-controls={filtersId}
            onClick={() => setShowFilters((value) => !value)}
          >
            表示
          </button>
          <button
            type="button"
            className="stbv-button"
            aria-expanded={showProperties}
            aria-controls={propertiesId}
            onClick={() => setShowProperties((value) => !value)}
          >
            属性
          </button>
        </fieldset>
      </div>
      <div className="stbv-body">
        <aside id={filtersId} className="stbv-panel stbv-panel-start" hidden={!showFilters} aria-label="表示">
          <h3>部材</h3>
          <div className="stbv-row">
            <button type="button" className="stbv-button" onClick={() => setHiddenKinds(new Set())}>
              すべて表示
            </button>
            <button type="button" className="stbv-button" onClick={() => setHiddenKinds(new Set(presentKinds))}>
              すべて隠す
            </button>
          </div>
          {presentKinds.length === 0 ? <p className="stbv-hint">部材はありません。</p> : null}
          <ul className="stbv-list">
            {presentKinds.map((kind) => {
              const id = `${filtersId}-${kind}`;
              return (
                <li key={kind} className="stbv-check">
                  <input
                    id={id}
                    type="checkbox"
                    checked={!hiddenKinds.has(kind)}
                    onChange={(event) => setKind(kind, event.currentTarget.checked)}
                  />
                  {colorMode === "kind" ? (
                    <span className="stbv-swatch" style={{ background: hex(KIND_COLORS[kind]) }} aria-hidden="true" />
                  ) : null}
                  <label htmlFor={id}>{MEMBER_LABELS[kind]}</label>
                  <span className="stbv-count">{(counts.get(kind) ?? 0).toLocaleString("ja-JP")}</span>
                </li>
              );
            })}
          </ul>
          {colorMode === "structure" ? (
            <ul className="stbv-list" aria-label="構造種別の色">
              {STRUCTURE_LEGEND.filter((key) => model.members.some((member) => member.structure === key)).map((key) => (
                <li key={key} className="stbv-check">
                  <span
                    className="stbv-swatch"
                    style={{ background: hex(STRUCTURE_COLORS[key] ?? 0) }}
                    aria-hidden="true"
                  />
                  <span>{key}</span>
                </li>
              ))}
            </ul>
          ) : null}

          <h3>階</h3>
          {model.stories.length === 0 ? <p className="stbv-hint">階の情報はありません。</p> : null}
          {model.stories.length > 0 ? (
            <div className="stbv-chips">
              <button
                type="button"
                className="stbv-button"
                aria-pressed={story === null}
                onClick={() => chooseStory(null)}
              >
                全階
              </button>
              {[...model.stories].reverse().map((item) => (
                <button
                  key={item.id}
                  type="button"
                  className="stbv-button"
                  aria-pressed={story === item.id}
                  title={`${item.name}（${item.height.toLocaleString("ja-JP")} mm）の部材を真上から見る`}
                  onClick={() => chooseStory(story === item.id ? null : item.id)}
                >
                  {item.name}
                </button>
              ))}
            </div>
          ) : null}

          <h3>通り芯</h3>
          {axisGroups.length === 0 ? <p className="stbv-hint">通り芯の情報はありません。</p> : null}
          {axisGroups.map(([group, axes]) => (
            <div key={group}>
              <p className="stbv-chip-group">{group || "（グループ名なし）"}</p>
              <div className="stbv-chips">
                {axes.map((item) => (
                  <button
                    key={item.key}
                    type="button"
                    className="stbv-button"
                    aria-pressed={axisKey === item.key}
                    title={`${item.name} 通りの部材を、通りに直交する向きから見る`}
                    onClick={() => chooseAxis(axisKey === item.key ? null : item)}
                  >
                    {item.name}
                  </button>
                ))}
              </div>
            </div>
          ))}
        </aside>
        <div ref={stageRef} className="stbv-stage" data-dragging={dragging} />
        <aside id={propertiesId} className="stbv-panel stbv-panel-end" hidden={!showProperties} aria-label="属性">
          {selected ? (
            <>
              <h3>
                {selected.name ? `${MEMBER_LABELS[selected.kind]} ${selected.name}` : MEMBER_LABELS[selected.kind]}
              </h3>
              <dl className="stbv-properties">
                {selected.properties.map((property) => (
                  <PropertyRow
                    key={`${property.label}:${property.value}`}
                    label={property.label}
                    value={property.value}
                  />
                ))}
              </dl>
              <details className="stbv-details">
                <summary>元の属性（XML）</summary>
                <div className="stbv-raw">
                  {selected.raw.map((raw) => (
                    <section key={raw.label}>
                      <h4>{raw.label}</h4>
                      <dl className="stbv-properties">
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
              <dl className="stbv-properties">
                {model.metadata.map((property) => (
                  <PropertyRow
                    key={`${property.label}:${property.value}`}
                    label={property.label}
                    value={property.value}
                  />
                ))}
              </dl>
              <p className="stbv-hint">部材を選ぶと、符号・断面・材料などの属性を表示します。</p>
            </>
          )}
          {model.notices.length > 0 ? (
            <details className="stbv-details">
              <summary>読み取りの注意（{model.notices.length}）</summary>
              <ul>
                {model.notices.map((notice) => (
                  <li key={notice.code} className={notice.level === "warning" ? "stbv-notice-warning" : undefined}>
                    {notice.message}（{notice.count.toLocaleString("ja-JP")} 件）
                  </li>
                ))}
              </ul>
            </details>
          ) : null}
        </aside>
      </div>
      <div className="stbv-status">
        <span>
          表示中 <strong>{visibleCount.toLocaleString("ja-JP")}</strong> /{" "}
          {model.members.length.toLocaleString("ja-JP")} 部材
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
            ? `選択中: ${MEMBER_LABELS[selected.kind]} ${selected.name || `ID ${selected.id}`}${selected.section ? `（${selected.section.shapes[0] ?? selected.section.name}）` : ""}`
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
