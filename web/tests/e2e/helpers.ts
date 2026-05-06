// 5 player E2E のヘルパ。各 helper は Page を受け取り、目的の状態へ進める。
//
// data-testid の規約:
//   - 接続: input-display-name, input-dev-email, btn-connect
//   - lobby: btn-create-room, input-room-id, btn-join-room, btn-start-match
//   - text-room-id: 部屋 ID 表示
//   - game-stage[data-overlay][data-my-turn][data-completed-tricks]: GameStage container
//   - overlay-bid, overlay-partner[data-mode], overlay-widow, overlay-end, overlay-trick-result
//   - bid-tricks-{n}, bid-trump-{S/H/D/C/NT}, btn-bid, btn-pass
//   - adj-card-{code}, btn-confirm-adjutant
//   - widow-card-{code}[data-discarded], btn-confirm-widow
//   - hand-card-{code}[data-playable][data-selected], btn-play
//   - btn-next-hand

import type { Browser, BrowserContext, Page } from "@playwright/test";
import { expect } from "@playwright/test";

export interface User {
  name: string;
  email: string;
  context: BrowserContext;
  page: Page;
}

const NAMES = ["alice", "bob", "charlie", "dave", "eve"] as const;

/**
 * 5 user を browser から起こして接続まで済ませる。先頭が host(部屋作成側)。
 *
 * `tag` を指定すると dev_email を `${name}-${tag}@local` にしてテスト間で衝突しない。
 * webServer は spec 間で再利用される(reuseExistingServer)ので、Welcome.current_room
 * から auto-rejoin される pollution を避けるために必須。
 */
export async function spawnFiveUsers(browser: Browser, tag = "default"): Promise<User[]> {
  const suffix = tag === "default" ? "" : `-${tag}`;
  const users: User[] = [];
  for (const name of NAMES) {
    const email = `${name}${suffix}@local`;
    const context = await browser.newContext();
    const page = await context.newPage();
    await page.goto("/");
    await page.getByTestId("input-display-name").fill(name);
    await page.getByTestId("input-dev-email").fill(email);
    await page.getByTestId("btn-connect").click();
    await expect(page.getByTestId("panel-lobby")).toBeVisible();
    users.push({ name, email, context, page });
  }
  return users;
}

export async function teardownUsers(users: User[]): Promise<void> {
  for (const u of users) {
    await u.context.close();
  }
}

/** users[0] が部屋を作って ID を返す。残り全員を join + lobby 状態まで進める。 */
export async function createRoomAndJoinAll(users: User[]): Promise<string> {
  const [host, ...rest] = users;
  await host.page.getByTestId("btn-create-room").click();
  await expect(host.page.getByTestId("text-room-id")).toBeVisible();
  const roomId = (await host.page.getByTestId("text-room-id").textContent())?.trim() ?? "";
  if (!roomId) throw new Error("failed to read room_id");

  for (const u of rest) {
    await u.page.getByTestId("input-room-id").fill(roomId);
    await u.page.getByTestId("btn-join-room").click();
    await expect(u.page.getByTestId("panel-room")).toBeVisible();
  }
  // host の player list が 5 人になるまで待つ
  await expect(host.page.getByTestId("list-players").locator("li")).toHaveCount(5);
  return roomId;
}

/** host が match を開始。全員の game-stage が出るまで待つ。 */
export async function startMatch(users: User[]): Promise<void> {
  const [host, ...rest] = users;
  await host.page.getByTestId("btn-start-match").click();
  for (const u of [host, ...rest]) {
    await expect(u.page.getByTestId("game-stage")).toBeVisible({ timeout: 10_000 });
  }
}

/** game-stage の data-overlay 属性を読む。 */
export async function currentOverlay(page: Page): Promise<string> {
  return (await page.getByTestId("game-stage").getAttribute("data-overlay")) ?? "";
}

export async function isMyTurn(page: Page): Promise<boolean> {
  return (await page.getByTestId("game-stage").getAttribute("data-my-turn")) === "true";
}

/**
 * Bidding round を進める。引数で指定した user が "13S" で立つ、他は全員パス。
 * declarer が決まる(全員 stage の overlay !== "bid" になる)まで待つ。
 *
 * 注意: napo-core のルールでは bidding は first_bidder から +1 ずつ巡る。
 * bidder の手番が全員に巡ってきたら declarer が決まる。
 */
export async function autoBid(
  users: User[],
  declarerIndex: number,
  bid: { tricks: number; trump: "S" | "H" | "D" | "C" | "NT" } = { tricks: 13, trump: "S" },
): Promise<void> {
  // 全員が「自分の番が来たら declarerIndex なら宣言、そうでなければパス」を実行する loop
  // 副官指名 phase に入る = bidding 終了
  const deadline = Date.now() + 60_000;
  while (Date.now() < deadline) {
    const overlays = await Promise.all(users.map((u) => currentOverlay(u.page)));
    if (overlays.every((o) => o !== "bid" && o !== "loading")) return;

    for (let i = 0; i < users.length; i++) {
      const u = users[i];
      if (!(await isMyTurn(u.page))) continue;
      if ((await currentOverlay(u.page)) !== "bid") continue;

      if (i === declarerIndex) {
        await u.page.getByTestId(`bid-tricks-${bid.tricks}`).click();
        await u.page.getByTestId(`bid-trump-${bid.trump}`).click();
        await u.page.getByTestId("btn-bid").click();
      } else {
        await u.page.getByTestId("btn-pass").click();
      }
      // 1人だけ act させて、次 loop で再評価
      break;
    }
    await users[0].page.waitForTimeout(150);
  }
  throw new Error("autoBid: timed out waiting for bidding to finish");
}

/**
 * declarer が副官カードを指名する。手札に含まれない最初の S/H/D/C × A を選ぶ。
 * 既に副官選択 overlay は declarer に "picker" モードで出ている前提。
 */
export async function autoAdjutant(declarer: Page): Promise<void> {
  await expect(declarer.getByTestId("overlay-partner")).toHaveAttribute("data-mode", "picker", {
    timeout: 10_000,
  });

  // 自分の手札に含まれない card_code を 1 つ選ぶ。SA → HA → DA → CA → SK → ...
  const candidates: string[] = [];
  for (const r of ["A", "K", "Q", "J", "10"]) {
    for (const s of ["S", "H", "D", "C"]) candidates.push(s + r);
  }
  const myHandLocator = declarer.locator('[data-testid^="hand-card-"]');
  const handCards = await myHandLocator.evaluateAll((els) =>
    els.map((e) => e.getAttribute("data-testid")?.replace("hand-card-", "") ?? ""),
  );
  const pick = candidates.find((c) => !handCards.includes(c));
  if (!pick) throw new Error("autoAdjutant: no candidate card found");

  await declarer.getByTestId(`adj-card-${pick}`).click();
  await declarer.getByTestId("btn-confirm-adjutant").click();
}

/**
 * declarer が widow を交換。先頭 N 枚を捨てる(最も愚直)。
 */
export async function autoWidow(declarer: Page, discardCount = 2): Promise<void> {
  await expect(declarer.getByTestId("overlay-widow")).toBeVisible({ timeout: 10_000 });

  // 場札 (widow) のカードを優先的に捨てる(=取り込まない選択)。N 枚 click。
  const allCards = declarer.locator('[data-testid^="widow-card-"]');
  await expect(allCards).not.toHaveCount(0);
  for (let i = 0; i < discardCount; i++) {
    await allCards.nth(i).click();
  }
  await declarer.getByTestId("btn-confirm-widow").click();
}

/**
 * 1 trick を全員プレイさせる。各 user が my-turn になったら playable な手札の先頭を
 * 出す。1 trick 終了(completed-tricks が +1)するまでループ。
 *
 * TrickResultOverlay(2s 自動 dismiss)が出ている間 click は弾かれるため、
 * 各 user に対して overlay-trick-result が disappear するまで待ってから click。
 */
export async function playOneTrick(users: User[]): Promise<void> {
  const stage = users[0].page.getByTestId("game-stage");
  const startCount = Number((await stage.getAttribute("data-completed-tricks")) ?? "0");
  console.log(`[playOneTrick] start, startCount=${startCount}`);
  const deadline = Date.now() + 60_000;
  let iters = 0;
  while (Date.now() < deadline) {
    iters++;
    const cur = Number((await stage.getAttribute("data-completed-tricks")) ?? "0");
    if (cur > startCount) {
      console.log(`[playOneTrick] complete, count=${cur}`);
      return;
    }
    // 局終了で room phase が between_hands / ended になったら退出
    const phase = await stage.getAttribute("data-room-phase");
    if (phase === "between_hands" || phase === "ended") {
      console.log(`[playOneTrick] hand ended, room phase=${phase}`);
      return;
    }

    let acted = false;
    for (const u of users) {
      const overlay = await currentOverlay(u.page);
      const myTurn = await isMyTurn(u.page);
      if (overlay !== "none" || !myTurn) continue;

      // TrickResultOverlay が出ている間は click が弾かれるので消えるまで待つ
      const trickResult = u.page.getByTestId("overlay-trick-result");
      if (await trickResult.isVisible().catch(() => false)) {
        await trickResult.waitFor({ state: "hidden", timeout: 5_000 }).catch(() => {});
      }

      const playable = u.page.locator('[data-testid^="hand-card-"][data-playable="true"]');
      const n = await playable.count();
      if (n === 0) {
        console.log(`[playOneTrick] ${u.name}: my-turn but 0 playable cards`);
        continue;
      }
      const cardId = await playable.first().getAttribute("data-testid");
      console.log(`[playOneTrick iter=${iters}] ${u.name} clicks ${cardId}`);
      await playable.first().click();
      await u.page.getByTestId("btn-play").click();
      acted = true;
      break;
    }
    if (!acted && iters % 20 === 0) {
      // 20 iter (= 6s) 何もできなかったら state を吐く
      const states = await Promise.all(
        users.map(async (u) => {
          const stg = u.page.getByTestId("game-stage");
          const o = await stg.getAttribute("data-overlay");
          const m = await stg.getAttribute("data-my-turn");
          const rp = await stg.getAttribute("data-room-phase");
          const ct = await stg.getAttribute("data-completed-tricks");
          return `${u.name}=ovl:${o}/turn:${m}/phase:${rp}/done:${ct}`;
        }),
      );
      console.log(`[playOneTrick iter=${iters}] stuck: ${states.join(", ")}`);
    }
    await users[0].page.waitForTimeout(300);
  }
  throw new Error("playOneTrick: timed out");
}

/** 10 trick 全部消化して end overlay が出るまで進める。 */
export async function playUntilHandEnd(users: User[]): Promise<void> {
  for (let i = 0; i < 10; i++) {
    await playOneTrick(users);
  }
  // 全員に EndOverlay が出るまで待つ
  for (const u of users) {
    await expect(u.page.getByTestId("overlay-end")).toBeVisible({ timeout: 10_000 });
  }
}
