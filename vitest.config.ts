import { playwright } from "@vitest/browser-playwright";
import { defaultClientConditions, defaultServerConditions } from "vite";
import { defineConfig } from "vitest/config";

export default defineConfig({
  // パッケージ同士は src を読む（package.json の exports の @zumenjs/source）
  resolve: { conditions: ["@zumenjs/source", ...defaultClientConditions] },
  ssr: { resolve: { conditions: ["@zumenjs/source", ...defaultServerConditions] } },
  test: {
    projects: [
      {
        extends: true,
        test: {
          // 図面の読み取りと表示モデルの単体テスト（Node）
          name: "node",
          include: ["packages/*/tests/**/*.test.ts"],
          exclude: ["**/*.browser.test.*"],
          environment: "node",
        },
      },
      {
        extends: true,
        // テストの途中で依存を最適化し直すと、Vite がページを読み直してテストが不安定になるので、先に最適化する
        optimizeDeps: { include: ["react", "react/jsx-dev-runtime", "react-dom/client"] },
        test: {
          // 表示と操作のエンジン・React のフックのテスト（Canvas とポインターのイベントを使うので Chromium で動かす）
          name: "browser",
          include: ["packages/*/tests/**/*.browser.test.{ts,tsx}"],
          browser: {
            enabled: true,
            provider: playwright(),
            headless: true,
            screenshotFailures: false,
            instances: [{ browser: "chromium" }],
          },
        },
      },
    ],
  },
});
