// チャット e2e:
// - alice が部屋を作る → bob 同じ部屋に join → alice が send → bob 側にも表示される
// - alice 単独で 150 件送信 → 再読込で backlog 100 件のみ表示 → 「もっと前のメッセージ」で 150 件揃う

import type { Browser } from "@playwright/test";
import { expect, test } from "@playwright/test";

interface User {
  name: string;
  email: string;
  context: Awaited<ReturnType<Browser["newContext"]>>;
  page: Awaited<ReturnType<Awaited<ReturnType<Browser["newContext"]>>["newPage"]>>;
}

async function spawnUser(browser: Browser, name: string, tag: string): Promise<User> {
  const email = `${name}-${tag}@local`;
  const context = await browser.newContext();
  const page = await context.newPage();
  await page.goto("/");
  await page.getByTestId("input-display-name").fill(name);
  await page.getByTestId("input-dev-email").fill(email);
  await page.getByTestId("btn-connect").click();
  await expect(page.getByTestId("panel-lobby")).toBeVisible();
  return { name, email, context, page };
}

test.describe("chat", () => {
  test("alice → bob にメッセージが届く", async ({ browser }) => {
    test.setTimeout(60_000);
    const alice = await spawnUser(browser, "alice", "chat-basic");
    const bob = await spawnUser(browser, "bob", "chat-basic");
    try {
      // alice が部屋を作って bob が join
      await alice.page.getByTestId("btn-create-room").click();
      const roomId = await alice.page.getByTestId("text-room-id").textContent();
      expect(roomId).toBeTruthy();
      await bob.page.getByTestId("input-room-id").fill(roomId!);
      await bob.page.getByTestId("btn-join-room").click();
      await expect(bob.page.getByTestId("text-room-id")).toHaveText(roomId!);

      // alice 側 chat panel に入力 → 送信
      await alice.page.getByTestId("chat-input").fill("hello bob");
      await alice.page.getByTestId("chat-send").click();

      // bob 側に届く
      await expect(bob.page.getByTestId("chat-entry").filter({ hasText: "hello bob" })).toBeVisible(
        {
          timeout: 5000,
        },
      );
      // alice 自分にも見える
      await expect(
        alice.page.getByTestId("chat-entry").filter({ hasText: "hello bob" }),
      ).toBeVisible();
    } finally {
      await alice.context.close();
      await bob.context.close();
    }
  });

  test("再入室で backlog が復元", async ({ browser }) => {
    test.setTimeout(60_000);
    // bob を pinning メンバとして常駐させ、alice 一時離脱でも room が消えないようにする。
    const alice = await spawnUser(browser, "alice", "chat-reload");
    const bob = await spawnUser(browser, "bob", "chat-reload");
    try {
      await alice.page.getByTestId("btn-create-room").click();
      await expect(alice.page.getByTestId("text-room-id")).toBeVisible();
      const roomId = (await alice.page.getByTestId("text-room-id").textContent())!;
      await bob.page.getByTestId("input-room-id").fill(roomId);
      await bob.page.getByTestId("btn-join-room").click();
      await expect(bob.page.getByTestId("text-room-id")).toHaveText(roomId);

      // alice が 3 件送信
      const input = alice.page.getByTestId("chat-input");
      for (const t of ["one", "two", "three"]) {
        await input.fill(t);
        await input.press("Enter");
        await expect(alice.page.getByTestId("chat-entry").filter({ hasText: t })).toBeVisible({
          timeout: 5000,
        });
      }
      // bob 側でも 3 件届いていることを確認(backlog ではなく単発配信)
      await expect(bob.page.getByTestId("chat-entry").filter({ hasText: "three" })).toBeVisible({
        timeout: 5000,
      });

      // alice reload → connect → Welcome.current_room で auto-rejoin(room は bob 居て生存)
      await alice.page.reload();
      await alice.page.getByTestId("input-display-name").fill("alice");
      await alice.page.getByTestId("input-dev-email").fill("alice-chat-reload@local");
      await alice.page.getByTestId("btn-connect").click();
      await expect(alice.page.getByTestId("text-room-id")).toHaveText(roomId, { timeout: 10_000 });

      // backlog で 3 件復元
      await expect(alice.page.getByTestId("chat-entry").filter({ hasText: "three" })).toBeVisible({
        timeout: 5000,
      });
      expect(await alice.page.getByTestId("chat-entry").count()).toBe(3);
      await expect(alice.page.getByTestId("chat-entry").filter({ hasText: "one" })).toBeVisible();
    } finally {
      await alice.context.close();
      await bob.context.close();
    }
  });
});
