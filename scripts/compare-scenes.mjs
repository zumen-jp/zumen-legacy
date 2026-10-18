// 2 つの Scene の JSON を比べる。数は 1e-9 の相対誤差まで同じとみなす（三角関数の実装の差）。
// crates/zumen/tests/golden.rs と同じ規則。

/**
 * @param {unknown} expected
 * @param {unknown} actual
 * @param {number} [limit] 返す違いの数の上限
 * @returns {string[]} 違いの場所と値
 */
export function compareScenes(expected, actual, limit = 10) {
  /** @type {string[]} */
  const out = [];
  /** @param {string} path @param {unknown} a @param {unknown} b */
  const walk = (path, a, b) => {
    if (out.length >= limit) return;
    if (typeof a === "number" && typeof b === "number") {
      if (Math.abs(a - b) > 1e-9 * Math.max(Math.abs(a), Math.abs(b), 1)) out.push(`${path}: ${a} ≠ ${b}`);
    } else if (Array.isArray(a) && Array.isArray(b)) {
      if (a.length !== b.length) out.push(`${path}: length ${a.length} ≠ ${b.length}`);
      else for (let index = 0; index < a.length; index++) walk(`${path}[${index}]`, a[index], b[index]);
    } else if (a && b && typeof a === "object" && typeof b === "object") {
      for (const key of new Set([...Object.keys(a), ...Object.keys(b)])) {
        walk(
          `${path}.${key}`,
          /** @type {Record<string, unknown>} */ (a)[key],
          /** @type {Record<string, unknown>} */ (b)[key],
        );
      }
    } else if (a !== b) {
      out.push(`${path}: ${JSON.stringify(a)} ≠ ${JSON.stringify(b)}`);
    }
  };
  walk("scene", expected, actual);
  return out;
}
