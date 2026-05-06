// 1 局完走の前段階(bid/adj/widow/最初の play)だけデバッグ用に切り出した spec。
// 各 phase 後の data-* attribute と hand を console.log する。

import { test } from "@playwright/test";
import {
  autoAdjutant,
  autoBid,
  autoWidow,
  createRoomAndJoinAll,
  spawnFiveUsers,
  startMatch,
  teardownUsers,
} from "./helpers";

test.describe("debug: phase transitions", () => {
  test("dump state after each phase", async ({ browser }) => {
    test.setTimeout(180_000);
    const users = await spawnFiveUsers(browser);
    try {
      await createRoomAndJoinAll(users);
      await startMatch(users);
      console.log("=== after startMatch ===");
      await dumpAll(users);

      await autoBid(users, 0, { tricks: 13, trump: "S" });
      console.log("=== after autoBid ===");
      await dumpAll(users);

      await autoAdjutant(users[0].page);
      console.log("=== after autoAdjutant ===");
      await dumpAll(users);

      await autoWidow(users[0].page, 2);
      console.log("=== after autoWidow ===");
      await dumpAll(users);

      // ここで最初のプレイヤーが to_act のはず。1 click だけ試して、その後の state も dump
      const stage0 = users[0].page.getByTestId("game-stage");
      const myTurn0 = await stage0.getAttribute("data-my-turn");
      console.log(`alice my-turn after widow: ${myTurn0}`);

      // 1 trick まるごとプレイ。to_act を回しながら 5 回 click
      for (let step = 0; step < 5; step++) {
        let acted = false;
        for (const u of users) {
          const stage = u.page.getByTestId("game-stage");
          const overlay = await stage.getAttribute("data-overlay");
          const myTurn = await stage.getAttribute("data-my-turn");
          if (overlay !== "none" || myTurn !== "true") continue;

          const playable = u.page.locator('[data-testid^="hand-card-"][data-playable="true"]');
          const count = await playable.count();
          if (count === 0) continue;
          const firstId = await playable.first().getAttribute("data-testid");
          console.log(`step ${step}: ${u.name} clicks ${firstId}`);
          await playable.first().click();
          await u.page.getByTestId("btn-play").click();
          await u.page.waitForTimeout(400);
          acted = true;
          break;
        }
        if (!acted) {
          console.log(`step ${step}: nobody acted`);
          break;
        }
        console.log(`=== after step ${step} ===`);
        await dumpAll(users);
      }
    } finally {
      await teardownUsers(users);
    }
  });
});

async function dumpAll(users: Awaited<ReturnType<typeof spawnFiveUsers>>): Promise<void> {
  for (const u of users) {
    const stage = u.page.getByTestId("game-stage");
    const overlay = await stage.getAttribute("data-overlay");
    const myTurn = await stage.getAttribute("data-my-turn");
    const completed = await stage.getAttribute("data-completed-tricks");
    const trick = await stage.getAttribute("data-trick-index");
    const hand = await u.page
      .locator('[data-testid^="hand-card-"]')
      .evaluateAll((els) =>
        els.map((e) => e.getAttribute("data-testid")?.replace("hand-card-", "")),
      );
    console.log(
      `  ${u.name}: overlay=${overlay} myTurn=${myTurn} completed=${completed} trick=${trick} hand=[${hand.join(",")}]`,
    );
  }
}
