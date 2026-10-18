import { defaultClientConditions, defaultServerConditions } from "vite";
import { defineConfig } from "vitest/config";

export default defineConfig({
  // パッケージ同士は src を読む（package.json の exports の @zumenjs/source）
  resolve: { conditions: ["@zumenjs/source", ...defaultClientConditions] },
  ssr: { resolve: { conditions: ["@zumenjs/source", ...defaultServerConditions] } },
  test: {
    // 図面の読み取りと表示モデルの単体テスト（Node）
    include: ["packages/*/tests/**/*.test.ts"],
    environment: "node",
  },
});
