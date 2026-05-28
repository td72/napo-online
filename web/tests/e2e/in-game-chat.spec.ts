// 試合中(GameStage 表示中)にも ChatPanel が出て送受信できることを確認する。
// 単独 user + 4 COM で match を開始し、GameStage と並んで chat-panel が出ていることを見る。
//
// レイアウト分岐(PC=sidebar, mobile=bottom)は data-variant 属性で識別する。

import { expect, test } from "@playwright/test";
import { addComs, spawnSingleUser, startMatch, teardownUsers } from "./helpers";

test.describe("in-game chat", () => {
  test("試合中も chat-panel が表示され、送信した内容が反映される", async ({ browser }) => {
    test.setTimeout(60_000);
    // 再実行時に Welcome.current_room による auto-rejoin で lobby に戻れない事故を
    // 避けるため、tag に timestamp を入れて identity を毎回フレッシュにする。
    const alice = await spawnSingleUser(browser, "alice", `ingame-chat-${Date.now()}`);
    const users = [alice];
    try {
      await alice.page.getByTestId("btn-create-room").click();
      await expect(alice.page.getByTestId("text-room-id")).toBeVisible();

      await addComs(alice.page, 4);
      await startMatch(users);

      // GameStage と並んで chat-panel が見える(default は pc layout → sidebar)
      await expect(alice.page.getByTestId("game-stage")).toBeVisible();
      const panel = alice.page.getByTestId("chat-panel");
      await expect(panel).toBeVisible();
      await expect(panel).toHaveAttribute("data-variant", "sidebar");

      // 送信できる
      await alice.page.getByTestId("chat-input").fill("during match");
      await alice.page.getByTestId("chat-send").click();
      await expect(
        alice.page.getByTestId("chat-entry").filter({ hasText: "during match" }),
      ).toBeVisible({ timeout: 5000 });
    } finally {
      await teardownUsers(users);
    }
  });
});
