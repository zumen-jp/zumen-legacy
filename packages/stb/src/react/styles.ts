/** 部品の見た目。class 名は stbv- で始め、利用側の CSS と衝突させない。cad-viewer と同じ色の決まりにする */
export const STB_VIEWER_CSS = `
.stbv {
  --stbv-surface: #ffffff;
  --stbv-surface-muted: #f3f4f6;
  --stbv-text: #1a1a1c;
  --stbv-text-muted: #55585e;
  --stbv-border: #c9ccd2;
  --stbv-accent: #0031d8;
  --stbv-selected: #e8f1fe;
  --stbv-focus: #ffd43d;
  --stbv-warning: #9a5b00;
  container-type: inline-size;
  display: grid;
  grid-template-rows: auto minmax(0, 1fr) auto;
  min-height: 0;
  height: 100%;
  color: var(--stbv-text);
  background: var(--stbv-surface);
  border: 1px solid var(--stbv-border);
  font: 14px/1.5 system-ui, -apple-system, "Hiragino Sans", "Noto Sans JP", sans-serif;
  box-sizing: border-box;
}
.stbv[data-theme="dark"] {
  --stbv-surface: #1f2023;
  --stbv-surface-muted: #2a2c30;
  --stbv-text: #f1f2f4;
  --stbv-text-muted: #b6b9bf;
  --stbv-border: #4a4d53;
  --stbv-accent: #9db7f9;
  --stbv-selected: #26385f;
  --stbv-warning: #f2b45a;
}
.stbv *, .stbv *::before, .stbv *::after { box-sizing: border-box; }
.stbv-toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 4px 12px;
  padding: 6px 8px;
  border-bottom: 1px solid var(--stbv-border);
  background: var(--stbv-surface-muted);
}
.stbv-title { margin: 0; font-size: 14px; font-weight: 700; overflow-wrap: anywhere; }
.stbv-group { display: flex; flex-wrap: wrap; gap: 4px; margin: 0; padding: 0; border: 0; min-width: 0; }
.stbv-visually-hidden {
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
.stbv-button {
  min-height: 40px;
  min-width: 40px;
  padding: 0 12px;
  border: 1px solid var(--stbv-border);
  border-radius: 6px;
  background: var(--stbv-surface);
  color: var(--stbv-text);
  font: inherit;
  cursor: pointer;
}
.stbv-button:hover { border-color: var(--stbv-text-muted); }
.stbv-button[aria-pressed="true"] {
  border-color: var(--stbv-accent);
  background: var(--stbv-selected);
  color: var(--stbv-accent);
  font-weight: 600;
}
@media (pointer: coarse) { .stbv-button { min-height: 44px; min-width: 44px; } }
.stbv :focus-visible {
  outline: 4px solid var(--stbv-focus);
  outline-offset: 0;
  box-shadow: 0 0 0 2px #000;
}
.stbv-body {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  min-height: 0;
}
.stbv-panel {
  width: 248px;
  min-height: 0;
  overflow: auto;
  padding: 8px 12px 12px;
  background: var(--stbv-surface);
}
.stbv-panel[hidden] { display: none; }
/* 欄を隠しても描画の領域が中央の列に残るよう、列を明示する */
.stbv-panel-start { grid-column: 1; border-right: 1px solid var(--stbv-border); }
.stbv-panel-end { grid-column: 3; width: 300px; border-left: 1px solid var(--stbv-border); }
.stbv-panel h3 { margin: 12px 0 6px; font-size: 14px; font-weight: 700; }
.stbv-panel h3:first-child { margin-top: 4px; }
.stbv-stage { grid-column: 2; grid-row: 1; position: relative; min-width: 0; min-height: 240px; overflow: hidden; touch-action: none; }
.stbv-canvas { position: absolute; inset: 0; width: 100%; height: 100%; display: block; cursor: grab; }
.stbv-stage[data-dragging="true"] .stbv-canvas { cursor: grabbing; }
.stbv-labels { position: absolute; inset: 0; pointer-events: none; overflow: hidden; }
.stbv-label {
  font: 600 11px/1 system-ui, -apple-system, "Hiragino Sans", "Noto Sans JP", sans-serif;
  color: var(--stbv-text);
  white-space: nowrap;
  user-select: none;
}
.stbv-label-axis {
  min-width: 22px;
  padding: 4px 5px;
  border: 1.5px solid #7a2e2e;
  border-radius: 999px;
  background: var(--stbv-surface);
  text-align: center;
}
.stbv[data-theme="dark"] .stbv-label-axis { border-color: #e08a8a; }
.stbv-label-story {
  padding: 2px 6px;
  border-radius: 4px;
  background: color-mix(in srgb, var(--stbv-surface) 85%, transparent);
  color: var(--stbv-text-muted);
  transform: translate(-50%, -100%);
}
.stbv-list { display: grid; gap: 2px; margin: 0; padding: 0; list-style: none; }
.stbv-check {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 32px;
  padding: 2px 4px;
  border-radius: 4px;
}
.stbv-check:hover { background: var(--stbv-surface-muted); }
.stbv-check input { width: 18px; height: 18px; margin: 0; flex: none; accent-color: var(--stbv-accent); }
.stbv-check label { flex: 1; min-width: 0; overflow-wrap: anywhere; cursor: pointer; }
.stbv-swatch { width: 12px; height: 12px; flex: none; border-radius: 2px; border: 1px solid rgb(0 0 0 / 0.25); }
.stbv-count { color: var(--stbv-text-muted); font-variant-numeric: tabular-nums; font-size: 12px; }
.stbv-chips { display: flex; flex-wrap: wrap; gap: 4px; }
.stbv-chips .stbv-button { min-height: 32px; min-width: 36px; padding: 0 8px; font-size: 13px; }
.stbv-chip-group { margin: 6px 0 2px; color: var(--stbv-text-muted); font-size: 12px; }
.stbv-row { display: flex; flex-wrap: wrap; gap: 4px; margin-bottom: 6px; }
.stbv-row .stbv-button { min-height: 32px; padding: 0 8px; font-size: 13px; }
.stbv-properties { display: grid; grid-template-columns: minmax(4.5em, max-content) minmax(0, 1fr); gap: 4px 12px; margin: 0; }
.stbv-properties dt { max-width: 9em; color: var(--stbv-text-muted); overflow-wrap: anywhere; }
.stbv-properties dd { margin: 0; overflow-wrap: anywhere; }
.stbv-details { margin-top: 12px; }
.stbv-details summary { cursor: pointer; font-weight: 600; }
.stbv-details ul { margin: 8px 0 0; padding-left: 20px; }
.stbv-details li { margin-bottom: 4px; }
.stbv-raw { margin-top: 8px; }
.stbv-raw h4 { margin: 8px 0 4px; font-size: 13px; overflow-wrap: anywhere; }
.stbv-raw .stbv-properties { font: 12px/1.45 ui-monospace, SFMono-Regular, Menlo, monospace; }
.stbv-notice-warning { color: var(--stbv-warning); }
.stbv-status {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 16px;
  min-height: 32px;
  align-items: center;
  padding: 4px 12px;
  border-top: 1px solid var(--stbv-border);
  background: var(--stbv-surface-muted);
  color: var(--stbv-text-muted);
  font-variant-numeric: tabular-nums;
}
.stbv-status strong { color: var(--stbv-text); font-weight: 600; }
.stbv-hint { margin: 0; color: var(--stbv-text-muted); }
@container (max-width: 760px) {
  .stbv-body { grid-template-columns: minmax(0, 1fr); grid-template-rows: minmax(300px, 1fr) auto auto; }
  .stbv-stage { grid-column: 1; grid-row: 1; }
  .stbv-panel, .stbv-panel-end { grid-column: 1; width: auto; max-height: 40vh; border: 0; border-top: 1px solid var(--stbv-border); }
  .stbv-panel-start { grid-row: 2; }
  .stbv-panel-end { grid-row: 3; }
}
@media (forced-colors: active) {
  .stbv-button[aria-pressed="true"] { outline: 2px solid Highlight; }
}
`;
