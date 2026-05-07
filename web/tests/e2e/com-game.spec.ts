// 1 人 + 4 COM で 1 局完走を確認。
//
// COM はサーバ側 driver(`Room::drive_coms`)が自動的に手を打つ。クライアント
// 側からは alice しか操作しない。既存の autoBid / autoAdjutant / autoWidow /
// playUntilHandEnd は users 配列を回すが、users が 1 要素でも alice の手番だけ
// 拾えば正しく機能する(他は COM が server 側で進める)。

import { expect, test } from "@playwright/test";
import {
  addComs,
  autoAdjutant,
  autoBid,
  autoWidow,
  playUntilHandEnd,
  spawnSingleUser,
  startMatch,
  teardownUsers,
} from "./helpers";

test.describe("COM-only opponents", () => {
  test("alice + 4 COM で 1 局完走", async ({ browser }) => {
    test.setTimeout(300_000);
    const alice = await spawnSingleUser(browser, "alice", "comgame");
    const users = [alice];
    try {
      // 部屋を作る
      await alice.page.getByTestId("btn-create-room").click();
      await expect(alice.page.getByTestId("text-room-id")).toBeVisible();

      // COM × 4
      await addComs(alice.page, 4);

      await startMatch(users);

      // alice = users[0] が declarer。COM は常に pass なので必然的に alice が立つ。
      await autoBid(users, 0, { tricks: 13, trump: "S" });
      await autoAdjutant(alice.page);
      await autoWidow(alice.page, 2);

      // playOneTrick / playUntilHandEnd は alice の手番だけ拾えばよい(他は COM が
      // サーバ側で動く)。
      await playUntilHandEnd(users);

      // EndOverlay
      const end = alice.page.getByTestId("overlay-end");
      await expect(end).toBeVisible();
      const won = await end.getAttribute("data-napo-won");
      expect(["true", "false"]).toContain(won ?? "");
    } finally {
      await teardownUsers(users);
    }
  });
});
