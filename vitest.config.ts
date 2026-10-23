import { playwright } from "@vitest/browser-playwright";
import { defaultClientConditions, defaultServerConditions } from "vite";
import { defineConfig } from "vitest/config";

// ブラウザのテストは Chromium、Firefox、WebKit で同じものを走らせる。ZUMEN_BROWSERS（空白かカンマ区切り）で絞れる。
// Windows の CI は Chromium だけにする（.github/workflows/ci.yml）
const browsers = ["chromium", "firefox", "webkit"] as const;
const listed = (process.env.ZUMEN_BROWSERS ?? "").split(/[\s,]+/).filter(Boolean);
const selected = listed.length > 0 ? listed : [...browsers];
for (const name of selected) {
  if (!(browsers as readonly string[]).includes(name)) {
    throw new Error(`ZUMEN_BROWSERS: unknown browser "${name}" (use ${browsers.join(", ")})`);
  }
}

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
          // 表示と操作のエンジン・React のフック・Web Worker のテスト（Canvas とポインターのイベントを使うのでブラウザで動かす）
          name: "browser",
          include: ["packages/*/tests/**/*.browser.test.{ts,tsx}"],
          browser: {
            enabled: true,
            provider: playwright(),
            headless: true,
            screenshotFailures: false,
            instances: browsers.filter((name) => selected.includes(name)).map((browser) => ({ browser })),
          },
        },
      },
    ],
  },
});
