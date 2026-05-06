// Mobile レイアウト検証用。各 phase で screenshot を撮って artifact として残す。
// Playwright trace に記録され、test-results/ に PNG が出る。

import { expect, test } from "@playwright/test";
import {
  autoAdjutant,
  autoBid,
  autoWidow,
  createRoomAndJoinAll,
  spawnFiveUsers,
  startMatch,
  teardownUsers,
} from "./helpers";

test.use({ viewport: { width: 414, height: 896 } });

test.describe("mobile layout screenshots", () => {
  test("各 phase の mobile レイアウトを撮影", async ({ browser }) => {
    test.setTimeout(180_000);
    const users = await spawnFiveUsers(browser, "mobile");
    try {
      await createRoomAndJoinAll(users);
      await startMatch(users);

      // alice 視点で mobile レイアウトに切替
      const alice = users[0].page;
      await alice.getByRole("button", { name: "mobile" }).click();

      // === Phase 1: Bidding ===
      await expect(alice.getByTestId("overlay-bid")).toBeVisible({ timeout: 15_000 });
      await alice.screenshot({ path: "test-results/mobile-bid.png", fullPage: false });
      await autoBid(users, 0, { tricks: 13, trump: "S" });

      // === Phase 2: Partner picker ===
      await expect(alice.getByTestId("overlay-partner")).toBeVisible({ timeout: 10_000 });
      await alice.screenshot({ path: "test-results/mobile-partner.png", fullPage: false });

      await autoAdjutant(alice);

      // === Phase 3: Widow exchange ===
      await expect(alice.getByTestId("overlay-widow")).toBeVisible({ timeout: 10_000 });
      await alice.screenshot({ path: "test-results/mobile-widow.png", fullPage: false });

      await autoWidow(alice, 2);

      // === Phase 4: Playing(自分の番)===
      await expect(alice.getByTestId("overlay-widow")).toBeHidden({ timeout: 10_000 });
      await alice.waitForTimeout(500);
      await alice.screenshot({ path: "test-results/mobile-playing-my-turn.png", fullPage: false });
    } finally {
      await teardownUsers(users);
    }
  });
});
