import { test, expect } from "@playwright/test";
import {
  autoAdjutant,
  autoBid,
  autoWidow,
  createRoomAndJoinAll,
  playUntilHandEnd,
  spawnFiveUsers,
  startMatch,
  teardownUsers,
} from "./helpers";

test.describe("napo-online full game flow", () => {
  test("5 player が 1 局を完走できる(bid → adjutant → widow → 10 trick → end)", async ({
    browser,
  }) => {
    test.setTimeout(180_000);
    const users = await spawnFiveUsers(browser, "fullgame");
    try {
      await createRoomAndJoinAll(users);
      await startMatch(users);

      // bidding: alice (index 0) が 13S で立つ、他はパス。
      // ただし first_bidder はランダム seat なので、bidding で first から +1 ずつ巡って
      // 全員に手番が来る。alice の手番で 13S 宣言、他はパスを autoBid が処理。
      await autoBid(users, 0, { tricks: 13, trump: "S" });

      // alice が declarer。副官指名 overlay に入る(declarer は picker、他は wait)。
      // alice の手札を確認したうえで含まれないカードを副官に指名。
      await autoAdjutant(users[0].page);

      // declarer = alice が widow 交換(2枚捨てる)。他はこの間 wait 状態。
      await autoWidow(users[0].page, 2);

      // 10 trick まで全員愚直プレイ。EndOverlay が出るまで進める。
      await playUntilHandEnd(users);

      // 各 client の end overlay 属性を assert
      const endStage = users[0].page.getByTestId("overlay-end");
      const napoWon = await endStage.getAttribute("data-napo-won");
      expect(napoWon === "true" || napoWon === "false").toBe(true);
      expect(await endStage.getAttribute("data-current-game")).toBe("1");
      // playwright.config.ts で NAPO_HANDS_PER_MATCH=3 に設定されている
      expect(await endStage.getAttribute("data-total-games")).toBe("3");
      // 全員に同じ overlay が出ているか
      for (const u of users) {
        await expect(u.page.getByTestId("overlay-end")).toBeVisible();
      }
    } finally {
      await teardownUsers(users);
    }
  });
});
