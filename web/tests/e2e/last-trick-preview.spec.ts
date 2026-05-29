// 直前 trick プレビュー(issue #14)の表示/非表示を確認。
//
// hand 1 trick 目(completed.length === 0)は非表示、2 trick 目以降は表示。
// hover/click で拡大パネルが出ることを確認する。

import { expect, test } from "@playwright/test";
import {
  addComs,
  autoAdjutant,
  autoBid,
  autoWidow,
  playOneTrick,
  playUntilHandEnd,
  spawnSingleUser,
  startMatch,
  teardownUsers,
} from "./helpers";

test.describe("直前 trick プレビュー", () => {
  test("1 trick 目は非表示、2 trick 目以降は表示・拡大", async ({ browser, page: _page }) => {
    test.setTimeout(300_000);
    const alice = await spawnSingleUser(browser, "alice", "lasttrick");
    const users = [alice];
    try {
      await alice.page.getByTestId("btn-create-room").click();
      await expect(alice.page.getByTestId("text-room-id")).toBeVisible();
      await addComs(alice.page, 4);
      await startMatch(users);

      await autoBid(users, 0, { tricks: 13, trump: "S" });
      await autoAdjutant(alice.page);
      await autoWidow(alice.page, 2);

      // trick 1 開始直後 — completed.length === 0 → preview は DOM に存在しない
      const stage = alice.page.getByTestId("game-stage");
      await expect(stage).toHaveAttribute("data-completed-tricks", "0", { timeout: 10_000 });
      await expect(alice.page.getByTestId("last-trick-preview")).not.toBeVisible();

      // 1 trick 完走 → completed.length === 1
      await playOneTrick(users);
      await expect(stage).not.toHaveAttribute("data-completed-tricks", "0", { timeout: 15_000 });

      // preview バッジが表示される
      const badge = alice.page.getByTestId("last-trick-preview");
      await expect(badge).toBeVisible({ timeout: 5_000 });

      // hover で拡大パネルが出る(hover イベントを simulate)
      await badge.hover();
      await expect(alice.page.getByTestId("last-trick-expanded")).toBeVisible({ timeout: 3_000 });

      // hover を外すと拡大パネルが消える
      await alice.page.mouse.move(0, 0);
      await expect(alice.page.getByTestId("last-trick-expanded")).not.toBeVisible();

      // click でもトグルできる
      await badge.click();
      await expect(alice.page.getByTestId("last-trick-expanded")).toBeVisible({ timeout: 3_000 });
      await badge.click();
      await expect(alice.page.getByTestId("last-trick-expanded")).not.toBeVisible();

      // 残りを完走
      await playUntilHandEnd(users);
    } finally {
      await teardownUsers(users);
    }
  });
});
