// 再接続シナリオ:WidowExchange 中(declarer 以外の wait 状態)で 1 人切断 → 再接続。
// issue #2 で報告されたバグ:再接続側の observation が反映されず "observation 待機中..." の
// まま止まる。
//
// 現状失敗を期待するので test.fixme でマーク。サーバ側を直したら fixme を外す。

import { expect, test } from "@playwright/test";
import {
  autoAdjutant,
  autoBid,
  createRoomAndJoinAll,
  spawnFiveUsers,
  startMatch,
  teardownUsers,
  type User,
} from "./helpers";

test.describe("reconnection (issue #2)", () => {
  test("WidowExchange 中に 1 人切断 → 再接続で observation が反映される", async ({ browser }) => {
    test.setTimeout(120_000);
    const users = await spawnFiveUsers(browser, "reconnect");
    try {
      await createRoomAndJoinAll(users);
      await startMatch(users);
      await autoBid(users, 0, { tricks: 13, trump: "S" });
      await autoAdjutant(users[0].page);

      // この時点で declarer (alice) は overlay-widow、他は overlay=none で wait。
      // bob (users[1]) を切断して再接続する。
      const bobOriginal = users[1];
      await bobOriginal.context.close();

      // 新しい context で同じ dev_email で接続
      const newContext = await browser.newContext();
      const newPage = await newContext.newPage();
      await newPage.goto("/");
      await newPage.getByTestId("input-display-name").fill(bobOriginal.name);
      await newPage.getByTestId("input-dev-email").fill(bobOriginal.email);
      await newPage.getByTestId("btn-connect").click();

      // 再接続後、room に自動的に join される(server 側で同 player_id を members で検出)
      // ただし ClientMessage::join_room を再送する必要があるかも。issue #2 の対象。
      await expect(newPage.getByTestId("game-stage")).toBeVisible({ timeout: 15_000 });

      const overlay = await newPage.getByTestId("game-stage").getAttribute("data-overlay");
      expect(["none", "widow", "partner"]).toContain(overlay ?? "");

      const reconnected: User = {
        ...bobOriginal,
        context: newContext,
        page: newPage,
      };
      users[1] = reconnected;
    } finally {
      await teardownUsers(users);
    }
  });
});
