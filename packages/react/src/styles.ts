/** 部品の見た目。class 名は cadv- で始め、利用側の CSS と衝突させない */
export const CAD_VIEWER_CSS = `
.cadv {
  --cadv-surface: #ffffff;
  --cadv-surface-muted: #f3f4f6;
  --cadv-text: #1a1a1c;
  --cadv-text-muted: #55585e;
  --cadv-border: #c9ccd2;
  --cadv-accent: #0031d8;
  --cadv-selected: #e8f1fe;
  --cadv-focus: #ffd43d;
  container-type: inline-size;
  display: grid;
  grid-template-rows: auto minmax(0, 1fr) auto;
  min-height: 0;
  height: 100%;
  color: var(--cadv-text);
  background: var(--cadv-surface);
  border: 1px solid var(--cadv-border);
  font: 14px/1.5 system-ui, -apple-system, "Hiragino Sans", "Noto Sans JP", sans-serif;
  box-sizing: border-box;
}
@media (prefers-color-scheme: dark) {
  .cadv {
    --cadv-surface: #1f2023;
    --cadv-surface-muted: #2a2c30;
    --cadv-text: #f1f2f4;
    --cadv-text-muted: #b6b9bf;
    --cadv-border: #4a4d53;
    --cadv-accent: #9db7f9;
    --cadv-selected: #26385f;
  }
}
.cadv *, .cadv *::before, .cadv *::after { box-sizing: border-box; }
.cadv-toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 4px 12px;
  padding: 6px 8px;
  border-bottom: 1px solid var(--cadv-border);
  background: var(--cadv-surface-muted);
}
.cadv-group { display: flex; flex-wrap: wrap; gap: 4px; margin: 0; padding: 0; border: 0; min-width: 0; }
.cadv-visually-hidden {
  position: absolute;
  width: 1px;
  height: 1px;
  margin: -1px;
  padding: 0;
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
  border: 0;
}
.cadv-button {
  min-height: 40px;
  min-width: 40px;
  padding: 0 12px;
  border: 1px solid var(--cadv-border);
  border-radius: 6px;
  background: var(--cadv-surface);
  color: var(--cadv-text);
  font: inherit;
  cursor: pointer;
}
.cadv-button:hover { border-color: var(--cadv-text-muted); }
.cadv-label { align-self: center; color: var(--cadv-text-muted); }
.cadv-select {
  min-height: 40px;
  max-width: 16em;
  padding: 0 8px;
  border: 1px solid var(--cadv-border);
  border-radius: 6px;
  background: var(--cadv-surface);
  color: var(--cadv-text);
  font: inherit;
}
.cadv-button[aria-pressed="true"] {
  border-color: var(--cadv-accent);
  background: var(--cadv-selected);
  color: var(--cadv-accent);
  font-weight: 600;
}
@media (pointer: coarse) { .cadv-button, .cadv-select { min-height: 44px; } .cadv-button { min-width: 44px; } }
.cadv :focus-visible {
  outline: 4px solid var(--cadv-focus);
  outline-offset: 0;
  box-shadow: 0 0 0 2px #000;
}
.cadv-body {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  min-height: 0;
}
.cadv-panel {
  width: 240px;
  min-height: 0;
  overflow: auto;
  padding: 8px 12px 12px;
  background: var(--cadv-surface);
}
.cadv-panel[hidden] { display: none; }
/* 欄を閉じても、図面の枠が幅 0 の 1 列目に入らないよう列を決めておく */
.cadv-panel-start { grid-column: 1; border-right: 1px solid var(--cadv-border); }
.cadv-panel-end { grid-column: 3; width: 280px; border-left: 1px solid var(--cadv-border); }
.cadv-panel h3 {
  margin: 4px 0 8px;
  font-size: 14px;
  font-weight: 700;
}
.cadv-stage { grid-column: 2; grid-row: 1; position: relative; min-width: 0; min-height: 240px; overflow: hidden; }
.cadv-layers { display: grid; gap: 2px; margin: 0; padding: 0; list-style: none; }
.cadv-layer {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 32px;
  padding: 2px 4px;
  border-radius: 4px;
}
.cadv-layer:hover { background: var(--cadv-surface-muted); }
.cadv-layer input { width: 18px; height: 18px; margin: 0; flex: none; accent-color: var(--cadv-accent); }
.cadv-layer label { flex: 1; min-width: 0; overflow-wrap: anywhere; cursor: pointer; }
.cadv-count { color: var(--cadv-text-muted); font-variant-numeric: tabular-nums; font-size: 12px; }
.cadv-layer-group { margin: 0 0 8px; padding: 0; border: 0; min-width: 0; }
.cadv-layer-group-title { width: 100%; font-weight: 600; background: var(--cadv-surface-muted); }
.cadv-layer-group .cadv-layers { padding-left: 16px; }
.cadv-row { display: flex; flex-wrap: wrap; gap: 4px; margin-bottom: 8px; }
.cadv-row .cadv-button { min-height: 32px; padding: 0 8px; font-size: 13px; }
.cadv-properties { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: 4px 12px; margin: 0; }
.cadv-properties dt { color: var(--cadv-text-muted); }
.cadv-properties dd { margin: 0; overflow-wrap: anywhere; }
.cadv-notices { margin-top: 16px; }
.cadv-notices summary { cursor: pointer; font-weight: 600; }
.cadv-notices ul { margin: 8px 0 0; padding-left: 20px; }
.cadv-notices li { margin-bottom: 4px; }
.cadv-status {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 16px;
  min-height: 32px;
  align-items: center;
  padding: 4px 12px;
  border-top: 1px solid var(--cadv-border);
  background: var(--cadv-surface-muted);
  color: var(--cadv-text-muted);
  font-variant-numeric: tabular-nums;
}
.cadv-status strong { color: var(--cadv-text); font-weight: 600; }
.cadv-hint { margin: 0; color: var(--cadv-text-muted); }
@container (max-width: 760px) {
  .cadv-body { grid-template-columns: minmax(0, 1fr); grid-template-rows: minmax(280px, 1fr) auto auto; }
  .cadv-stage { grid-column: 1; grid-row: 1; }
  .cadv-panel, .cadv-panel-end { grid-column: 1; width: auto; max-height: 40vh; border: 0; border-top: 1px solid var(--cadv-border); }
  .cadv-panel-start { grid-row: 2; }
  .cadv-panel-end { grid-row: 3; }
}
@media (forced-colors: active) {
  .cadv-button[aria-pressed="true"] { outline: 2px solid Highlight; }
}
`;
