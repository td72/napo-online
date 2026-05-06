import { defineConfig, devices } from "@playwright/test";

const PORT = 8080;
const BASE_URL = `http://127.0.0.1:${PORT}`;

export default defineConfig({
  testDir: "./tests/e2e",
  // 5 player の bot 進行は依存関係が多いので、並列化せずに 1 spec ずつ走らせる。
  fullyParallel: false,
  workers: 1,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["github"], ["list"]] : [["list"]],
  use: {
    baseURL: BASE_URL,
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    video: "retain-on-failure",
    // GamePC layout が 1280x800 + header + padding なので viewport を大きめに。
    viewport: { width: 1400, height: 1000 },
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"], viewport: { width: 1400, height: 1000 } },
    },
  ],
  webServer: {
    // mise run dev:integrated は web build → cargo run の 2 段階。
    // playwright が一度だけ build & 起動するように、reuseExistingServer で
    // ローカル試遊中(ポート占有)のサーバを使い回せるようにする。
    command: "mise run dev:integrated",
    cwd: "..",
    url: `${BASE_URL}/healthz`,
    timeout: 180_000,
    reuseExistingServer: !process.env.CI,
    env: {
      // dev_email クエリで identity を分離するので、本番 CF Access verification は不要。
      NAPO_DEV_MODE: "true",
      // テスト専用 DB(消えても OK)
      NAPO_DB: "/tmp/napo-e2e.db",
      // e2e は 3 局/match に短縮(本番は 10 局)。Match 完走 spec のターンアラウンドを抑える。
      NAPO_HANDS_PER_MATCH: "3",
    },
  },
});
