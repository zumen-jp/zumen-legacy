import {
  type MarkupDocument,
  type MarkupDrawing,
  type MarkupItem,
  type MarkupStatus,
  markupBox,
  markupFromJson,
  markupToJson,
} from "@zumenjs/core";
import { useMarkupState, useViewerState } from "@zumenjs/react";
import {
  cloudTool,
  type DrawingViewer,
  findingTool,
  type MarkupDraft,
  type MarkupItemInput,
  type MarkupLayer,
  markupEditTool,
  photoTool,
  redlineTool,
  textTool,
} from "@zumenjs/viewer";
import { type FormEvent, useEffect, useId, useMemo, useRef, useState } from "react";
import "./markup-sample.css";

/**
 * マークアップ（赤入れ・雲マーク・メモ・指摘・写真の位置）の画面のサンプル。@zumenjs/react のフックと
 * @zumenjs/viewer の公開された API だけで、道具の操作盤、項目の一覧、指摘の本文と状態の欄、文書の保存と読み込みを作る。
 * 文言と CSS（markup-sample.css）はこちらが持つ
 */

const KIND_NAMES: Record<MarkupItem["kind"], string> = {
  redline: "赤線",
  cloud: "雲マーク",
  text: "メモ",
  finding: "指摘",
  photo: "写真",
};

const NO_IDS: readonly string[] = [];

const STATUS_NAMES: Record<MarkupStatus, string> = { open: "未対応", resolved: "対応済み", closed: "確認済み" };

/** 分類と、その名前。色は markup-sample.css の --zumen-markup-category-<分類>-* */
const CATEGORIES = [
  { id: "correction", name: "修正" },
  { id: "question", name: "質問" },
  { id: "checked", name: "確認" },
] as const;

/** 道具の名前と、操作盤の文言 */
const TOOLS = [
  { name: "markup-edit", label: "編集", title: "クリックで選び、ドラッグで動かす。Delete で消す" },
  { name: "markup-redline", label: "赤線", title: "ドラッグか 2 回のクリックで線を引く" },
  { name: "markup-arrow", label: "矢印", title: "ドラッグか 2 回のクリックで矢印を引く" },
  {
    name: "markup-polyline",
    label: "折線",
    title: "クリックで点を加え、最後の点をもう一度クリックするか Enter で終える",
  },
  { name: "markup-freehand", label: "手書き", title: "ドラッグした跡を残す" },
  { name: "markup-highlighter", label: "蛍光ペン", title: "ドラッグした跡を太い半透明の線で残す" },
  { name: "markup-rectangle", label: "矩形", title: "ドラッグか 2 回のクリックで矩形を描く" },
  { name: "markup-cloud", label: "雲マーク", title: "ドラッグか 2 回のクリックで範囲を囲む" },
  { name: "markup-text", label: "メモ", title: "クリックした位置に文字を書く" },
  { name: "markup-finding", label: "指摘", title: "クリックでピンを置き、ドラッグで範囲を囲む" },
  { name: "markup-photo", label: "写真", title: "撮った位置を押し、写した向きへドラッグする" },
] as const;

/** 下書きに本文などを入れる欄で待っている下書き */
interface Pending {
  draft: MarkupDraft;
  resolve: (item: MarkupItemInput | null) => void;
}

function itemSummary(item: MarkupItem): string {
  const parts = [KIND_NAMES[item.kind]];
  if (item.kind === "finding") {
    if (item.label) parts.push(`#${item.label}`);
    parts.push(STATUS_NAMES[item.status]);
  }
  return parts.join(" ");
}

/**
 * 画面で 14 px ほどになるメモの文字の高さ（用紙の mm）。ISO 3098 の高さ 1.8・2.5・3.5・5・7 mm を 10 倍ずつ広げた
 * 列から、近いものを選ぶ（遠くから書いたメモも読めるように）
 */
function noteHeight(viewer: DrawingViewer): number {
  const target = (14 * 25.4) / 96 / viewer.getView().zoom;
  let best = 3.5;
  for (let power = -3; power <= 6; power++) {
    for (const base of [1.8, 2.5, 3.5, 5, 7]) {
      const height = Number((base * 10 ** power).toPrecision(2));
      if (Math.abs(Math.log(height / target)) < Math.abs(Math.log(best / target))) best = height;
    }
  }
  return best;
}

function download(name: string, text: string): void {
  const url = URL.createObjectURL(new Blob([text], { type: "application/json" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = name;
  link.click();
  URL.revokeObjectURL(url);
}

export interface MarkupToolbarProps {
  viewer: DrawingViewer | null;
  layer: MarkupLayer | null;
  /** マークアップを付ける図面（バイトの SHA-256 と形式） */
  drawing: MarkupDrawing;
  /** 保存するファイルの名前の元 */
  name: string;
  /** 保存した文書を開いたとき */
  onOpen: (document: MarkupDocument) => void;
}

/** マークアップの道具を viewer に加え、道具・分類・取り消し・保存と読み込みの操作盤を出す */
export function MarkupToolbar({ viewer, layer, drawing, name, onOpen }: MarkupToolbarProps) {
  const [message, setMessage] = useState<string | null>(null);
  const [category, setCategory] = useState<string>("correction");
  const [pending, setPending] = useState<Pending | null>(null);
  const [onlyOpen, setOnlyOpen] = useState(false);
  const fileId = useId();
  const tool = useViewerState(viewer, (state) => state.tool) ?? "select";
  const canUndo = useMarkupState(layer, (state) => state.canUndo) ?? false;
  const canRedo = useMarkupState(layer, (state) => state.canRedo) ?? false;

  // 道具の下書きに、選んでいる分類を入れる。本文の要る種類は欄で入れてから加える
  const categoryRef = useRef(category);
  categoryRef.current = category;
  useEffect(() => {
    if (!viewer || !layer) return;
    const withCategory = (draft: MarkupDraft) => ({ ...draft, category: categoryRef.current }) as MarkupItemInput;
    const ask = (draft: MarkupDraft) => {
      const sized =
        draft.kind === "text" && draft.height === undefined ? { ...draft, height: noteHeight(viewer) } : draft;
      return new Promise<MarkupItemInput | null>((resolve) =>
        setPending({ draft: withCategory(sized) as MarkupDraft, resolve }),
      );
    };
    const removers = [
      markupEditTool(layer),
      redlineTool(layer, { create: withCategory }),
      redlineTool(layer, { name: "markup-arrow", arrows: "end", create: withCategory }),
      redlineTool(layer, { name: "markup-polyline", mode: "polyline", create: withCategory }),
      redlineTool(layer, { name: "markup-freehand", mode: "freehand", create: withCategory }),
      redlineTool(layer, {
        name: "markup-highlighter",
        mode: "freehand",
        lineWidth: 4,
        color: "#ffd40080",
        create: withCategory,
      }),
      redlineTool(layer, { name: "markup-rectangle", mode: "rectangle", create: withCategory }),
      cloudTool(layer, { create: withCategory }),
      textTool(layer, { create: ask }),
      findingTool(layer, { create: ask }),
      photoTool(layer, { create: ask }),
    ].map((created) => viewer.addTool(created));
    return () => {
      for (const remove of removers) remove();
    };
  }, [viewer, layer]);

  useEffect(() => {
    layer?.setFilter(onlyOpen ? (item) => item.kind === "finding" && item.status === "open" : null);
  }, [layer, onlyOpen]);

  const open = (file: File | null) => {
    if (!file) return;
    file
      .text()
      .then((text) => {
        const next = markupFromJson(text);
        onOpen(next);
        setMessage(
          next.drawing.sha256 === drawing.sha256
            ? `${file.name} を開きました`
            : `${file.name} は別の版の図面のマークアップです。位置は用紙の mm のまま描き、図形との対応は確かめていません`,
        );
      })
      .catch((error: unknown) =>
        setMessage(`${file.name} を開けません: ${error instanceof Error ? error.message : error}`),
      );
  };

  if (!layer || !viewer) return null;
  return (
    <>
      <fieldset className="sample-group">
        <legend className="sample-visually-hidden">マークアップ</legend>
        {TOOLS.map((entry) => (
          <button
            key={entry.name}
            type="button"
            className="sample-button"
            aria-pressed={tool === entry.name}
            title={entry.title}
            onClick={() => viewer.setTool(entry.name)}
          >
            {entry.label}
          </button>
        ))}
      </fieldset>
      <fieldset className="sample-group">
        <legend className="sample-visually-hidden">マークアップの文書</legend>
        <label className="sample-label" htmlFor={`${fileId}-category`}>
          分類
        </label>
        <select
          id={`${fileId}-category`}
          className="sample-select"
          value={category}
          onChange={(event) => setCategory(event.currentTarget.value)}
        >
          {CATEGORIES.map((entry) => (
            <option key={entry.id} value={entry.id}>
              {entry.name}
            </option>
          ))}
        </select>
        <button type="button" className="sample-button" disabled={!canUndo} onClick={() => layer.undo()} title="Ctrl+Z">
          元に戻す
        </button>
        <button
          type="button"
          className="sample-button"
          disabled={!canRedo}
          onClick={() => layer.redo()}
          title="Shift+Ctrl+Z"
        >
          やり直す
        </button>
        <button
          type="button"
          className="sample-button"
          onClick={() => download(`${name}.markup.json`, `${markupToJson(layer.document, 2)}\n`)}
        >
          保存
        </button>
        <label className="sample-button markup-open" htmlFor={fileId}>
          開く
          <input
            id={fileId}
            type="file"
            accept=".json,application/json"
            className="sample-visually-hidden"
            onChange={(event) => {
              open(event.currentTarget.files?.[0] ?? null);
              event.currentTarget.value = "";
            }}
          />
        </label>
        <label className="markup-check">
          <input type="checkbox" checked={onlyOpen} onChange={(event) => setOnlyOpen(event.currentTarget.checked)} />
          未対応の指摘だけ
        </label>
      </fieldset>
      {message ? (
        <p className="markup-message" role="status">
          {message}
        </p>
      ) : null}
      {pending ? (
        <DraftDialog
          pending={pending}
          onDone={(item) => {
            pending.resolve(item);
            setPending(null);
          }}
        />
      ) : null}
    </>
  );
}

/** 下書きの本文・状態・写真の参照を入れる欄 */
function DraftDialog({ pending, onDone }: { pending: Pending; onDone: (item: MarkupItemInput | null) => void }) {
  const ref = useRef<HTMLDialogElement>(null);
  const textId = useId();
  const { draft } = pending;
  useEffect(() => {
    const dialog = ref.current;
    if (dialog && !dialog.open) dialog.showModal();
  }, []);
  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const text = String(form.get("text") ?? "");
    if (draft.kind === "photo") onDone({ ...draft, photo: String(form.get("photo") ?? ""), ...(text ? { text } : {}) });
    else if (draft.kind === "finding") onDone({ ...draft, text, status: String(form.get("status")) as MarkupStatus });
    else onDone({ ...draft, text } as MarkupItemInput);
  };
  return (
    <dialog ref={ref} className="markup-dialog" onCancel={() => onDone(null)} aria-labelledby={`${textId}-title`}>
      <form method="dialog" onSubmit={submit}>
        <h3 id={`${textId}-title`}>{KIND_NAMES[draft.kind]}</h3>
        {draft.kind === "photo" ? (
          <label>
            写真の参照（ファイル名・URL）
            <input name="photo" required autoComplete="off" />
          </label>
        ) : null}
        <label>
          {draft.kind === "finding" ? "指摘" : draft.kind === "photo" ? "説明（任意）" : "本文"}
          <textarea id={textId} name="text" rows={3} required={draft.kind === "text"} />
        </label>
        {draft.kind === "finding" ? (
          <label>
            状態
            <select name="status" defaultValue={draft.status}>
              {Object.entries(STATUS_NAMES).map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </select>
          </label>
        ) : null}
        <div className="sample-row">
          <button type="submit" className="sample-button">
            加える
          </button>
          <button type="button" className="sample-button" onClick={() => onDone(null)}>
            やめる
          </button>
        </div>
      </form>
    </dialog>
  );
}

export interface MarkupListProps {
  viewer: DrawingViewer | null;
  layer: MarkupLayer | null;
  /** 文書の drawing.sha256 が、表示している図面のバイトの SHA-256 と同じか */
  sameDrawing: boolean;
}

/** 図面のビューの項目の一覧と、選んだ項目の本文・状態の欄 */
export function MarkupList({ viewer, layer, sameDrawing }: MarkupListProps) {
  const document = useMarkupState(layer, (state) => state.document);
  const filter = useMarkupState(layer, (state) => state.filter) ?? null;
  const selection = useMarkupState(layer, (state) => state.selection) ?? NO_IDS;
  const scene = useViewerState(viewer, (state) => state.scene);
  // 層が描いている項目と同じ（図面のビューのもので、filter を通るもの）
  const items = useMemo(
    () => document?.items.filter((item) => item.view === scene?.view && (!filter || filter(item))) ?? [],
    [document, filter, scene],
  );
  const selected = items.find((item) => item.id === selection[selection.length - 1]) ?? null;

  // 選んだ項目が指す図形を強調する（同じバイトの図面のときだけ、id が図形を指す）
  useEffect(() => {
    if (!viewer || !scene) return;
    const ids = new Set(sameDrawing ? (selected?.entities ?? []) : []);
    viewer.setHighlight(
      "markup",
      scene.entities.filter((entity) => ids.has(entity.id)),
    );
  }, [viewer, scene, selected, sameDrawing]);

  if (!layer || !viewer) return null;
  const show = (item: MarkupItem) => {
    layer.select([item.id]);
    const box = markupBox(item);
    const margin = Math.max(box.maxX - box.minX, box.maxY - box.minY, 20) * 0.5;
    viewer.fitBounds({
      minX: box.minX - margin,
      minY: box.minY - margin,
      maxX: box.maxX + margin,
      maxY: box.maxY + margin,
    });
  };
  return (
    <section className="markup-list" aria-label="マークアップ">
      <h3>マークアップ（{items.length}）</h3>
      {items.length === 0 ? <p className="sample-hint">操作盤の道具で図面に書き込みます。</p> : null}
      <ul>
        {items.map((item) => (
          <li key={item.id}>
            <button
              type="button"
              className="markup-item"
              aria-current={selection.includes(item.id)}
              onClick={() => show(item)}
            >
              <span className="markup-kind">{itemSummary(item)}</span>
              {item.carried && item.carried.state !== "unchanged" ? (
                <span className="markup-carried">{item.carried.state === "changed" ? "変更あり" : "図形なし"}</span>
              ) : null}
              <span className="markup-text">{item.text || (item.kind === "photo" ? item.photo : "")}</span>
            </button>
          </li>
        ))}
      </ul>
      {selected ? <ItemEditor key={selected.id} layer={layer} item={selected} /> : null}
    </section>
  );
}

/** 選んだ項目の本文・状態・分類を変える欄 */
function ItemEditor({ layer, item }: { layer: MarkupLayer; item: MarkupItem }) {
  const id = useId();
  const [text, setText] = useState(item.text ?? "");
  const commit = () => {
    if (text !== (item.text ?? "") && (item.kind !== "text" || text !== "")) {
      layer.update(item.id, { text: text === "" ? undefined : text });
    }
  };
  return (
    <div className="markup-editor">
      <h3>{itemSummary(item)}</h3>
      <label htmlFor={`${id}-text`}>{item.kind === "finding" ? "指摘" : "本文"}</label>
      <textarea
        id={`${id}-text`}
        rows={3}
        value={text}
        onChange={(event) => setText(event.currentTarget.value)}
        onBlur={commit}
      />
      {item.kind === "finding" ? (
        <>
          <label htmlFor={`${id}-status`}>状態</label>
          <select
            id={`${id}-status`}
            className="sample-select"
            value={item.status}
            onChange={(event) => layer.update(item.id, { status: event.currentTarget.value as MarkupStatus })}
          >
            {Object.entries(STATUS_NAMES).map(([value, label]) => (
              <option key={value} value={value}>
                {label}
              </option>
            ))}
          </select>
        </>
      ) : null}
      <label htmlFor={`${id}-category`}>分類</label>
      <select
        id={`${id}-category`}
        className="sample-select"
        value={item.category ?? ""}
        onChange={(event) => layer.update(item.id, { category: event.currentTarget.value || undefined })}
      >
        <option value="">なし</option>
        {CATEGORIES.map((entry) => (
          <option key={entry.id} value={entry.id}>
            {entry.name}
          </option>
        ))}
      </select>
      <dl className="sample-properties">
        {item.author ? (
          <>
            <dt>作成者</dt>
            <dd>{item.author}</dd>
          </>
        ) : null}
        {item.updatedAt ? (
          <>
            <dt>更新</dt>
            <dd>{new Date(item.updatedAt).toLocaleString("ja-JP")}</dd>
          </>
        ) : null}
        {item.entities?.length ? (
          <>
            <dt>図形</dt>
            <dd>{item.entities.join("、")}</dd>
          </>
        ) : null}
      </dl>
      <div className="sample-row">
        <button type="button" className="sample-button" onClick={() => layer.remove(item.id)}>
          消す
        </button>
      </div>
    </div>
  );
}
