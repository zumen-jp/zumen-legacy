import { rmSync } from "node:fs";

// 各パッケージのビルド結果を消す（古いファイルが dist に残らないように）
for (const name of ["core", "viewer", "sxf", "jww", "dxf", "reader", "react"]) {
  rmSync(new URL(`../packages/${name}/dist`, import.meta.url), { recursive: true, force: true });
}
