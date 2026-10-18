"use client";

import type { SceneLayer } from "@zumenjs/core";
import { useMemo } from "react";

interface LayerListProps {
  layers: readonly SceneLayer[];
  hidden: ReadonlySet<number>;
  counts: ReadonlyMap<number, number>;
  idPrefix: string;
  onChange: (ids: readonly number[], visible: boolean) => void;
}

/**
 * レイヤの表示切り替え。JWW のようにレイヤグループがある図面は、グループごとにまとめ、
 * グループの切り替え（一部だけ表示中は中間の状態）を付ける。
 */
export function LayerList({ layers, hidden, counts, idPrefix, onChange }: LayerListProps) {
  const groups = useMemo(() => {
    const result: { key: string; name: string | null; layers: SceneLayer[] }[] = [];
    for (const layer of layers) {
      const name = layer.group ?? null;
      const last = result[result.length - 1];
      if (last && last.name === name) last.layers.push(layer);
      // 同じ名前のグループが離れて現れても区別できるよう、最初のレイヤの番号を鍵にする
      else result.push({ key: `${name ?? ""}:${layer.id}`, name, layers: [layer] });
    }
    return result;
  }, [layers]);

  const items = (members: SceneLayer[]) => (
    <ul className="cadv-layers">
      {members.map((layer) => {
        const id = `${idPrefix}-${layer.id}`;
        return (
          <li key={layer.id} className="cadv-layer">
            <input
              id={id}
              type="checkbox"
              checked={!hidden.has(layer.id)}
              onChange={(event) => onChange([layer.id], event.currentTarget.checked)}
            />
            <label htmlFor={id}>{layer.name}</label>
            <span className="cadv-count">
              {counts.get(layer.id) ?? 0}
              <span className="cadv-visually-hidden"> 要素</span>
            </span>
          </li>
        );
      })}
    </ul>
  );

  return (
    <>
      {groups.map((group) => {
        if (group.name === null) return <div key={group.key}>{items(group.layers)}</div>;
        const ids = group.layers.map((layer) => layer.id);
        const visibleCount = ids.filter((id) => !hidden.has(id)).length;
        const id = `${idPrefix}-group-${group.layers[0]?.id ?? 0}`;
        return (
          <fieldset key={group.key} className="cadv-layer-group">
            <legend className="cadv-layer cadv-layer-group-title">
              <input
                id={id}
                type="checkbox"
                checked={visibleCount === ids.length}
                ref={(element) => {
                  if (element) element.indeterminate = visibleCount > 0 && visibleCount < ids.length;
                }}
                onChange={(event) => onChange(ids, event.currentTarget.checked)}
              />
              <label htmlFor={id}>{group.name || "（名前なし）"}</label>
              <span className="cadv-count">
                {ids.length}
                <span className="cadv-visually-hidden"> レイヤ</span>
              </span>
            </legend>
            {items(group.layers)}
          </fieldset>
        );
      })}
    </>
  );
}
