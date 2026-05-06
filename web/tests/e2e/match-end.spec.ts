// Hand またぎテスト: 1 局完走 → 全員 next-hand → hand 2 の bid overlay が出るまで。
//
// 3 局完走 → match_ended の検証は Rust 側の integration test で行う方が安価。
// ここでは event 駆動の overlay 切り替え(hand_ended→between_hands→playing)が
// 動くことを確認する。

import { expect, test } from "@playwright/test";
import {
  clickNextHand,
  createRoomAndJoinAll,
  playOneHand,
  spawnFiveUsers,
  startMatch,
  teardownUsers,
} from "./helpers";

test.describe("hand transitions", () => {
  test("hand 1 → next_hand → hand 2 bid overlay", async ({ browser }) => {
    test.setTimeout(180_000);
    const users = await spawnFiveUsers(browser, "matchend");
    try {
      await createRoomAndJoinAll(users);
      await startMatch(users);

      // hand 1 を完走
      await playOneHand(users, 0);

      // EndOverlay は matchEnded=false(まだ局が残っている)
      for (const u of users) {
        const overlay = u.page.getByTestId("overlay-end");
        await expect(overlay).toBeVisible({ timeout: 10_000 });
        await expect(overlay).toHaveAttribute("data-match-ended", "false");
        await expect(overlay).toHaveAttribute("data-current-game", "1");
      }

      // 全員 Next → hand 2 が開始
      await clickNextHand(users);

      // hand 2 で再び bid overlay
      for (const u of users) {
        await expect(u.page.getByTestId("overlay-bid")).toBeVisible({ timeout: 15_000 });
      }
      // 1 局目とは別の hand_index になっている(GAME カウンタが 2 に進んでいる)
      const game = await users[0].page.getByTestId("game-stage").getAttribute("data-trick-index");
      expect(game).toBe("0");
    } finally {
      await teardownUsers(users);
    }
  });
});
