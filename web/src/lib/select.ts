// Zustand store state(WS から流れてくる observation / room / seats)を
// design 由来の GamePcViewModel / Overlay VMs に変換する pure 関数群。

import type {
  CornerPos,
  FieldPlayVM,
  FieldSeatPos,
  GamePcViewModel,
  OpponentVM,
  TrickPhase,
} from "../components/GamePC";
import type { BidSeatVM, OverlayPhase, StandingVM, TeamPlayerVM } from "../components/Overlays";
import { effectiveLeadSuit, sortHand, type SuitKey } from "../components/card-shared";
import type {
  PublicEvent,
  RoomView,
  SeatAssignment,
  WireBid,
  WireObservation,
  WirePlayedCard,
  WireTrump,
} from "../types";

const FACE_RANKS = new Set(["A", "K", "Q", "J", "10"]);

/** "S","H","D","C" を返す。"NT" の場合は null。 */
export function trumpSuitFromCode(s: string): SuitKey | null {
  return s === "S" || s === "H" || s === "D" || s === "C" ? s : null;
}

/** 自分(mySeat)を中心に、(mySeat+offset)%num を tl/tr/bl/br に割り当てる。 */
function cornerForOffset(offset: number): CornerPos {
  // 自分は bottom 中央。テーブル俯瞰視点で時計回り(自分→左隣→対面左→対面右→右隣):
  //   +1 → 左隣(手前左)= bl
  //   +2 → 向かい左(奥左)= tl
  //   +3 → 向かい右(奥右)= tr
  //   +4 → 右隣(手前右)= br
  switch (offset) {
    case 1:
      return "bl";
    case 2:
      return "tl";
    case 3:
      return "tr";
    case 4:
      return "br";
    default:
      throw new Error(`invalid seat offset ${offset}`);
  }
}

/** seat → 自分視点の corner。自分自身を渡したら null。 */
export function seatToCorner(mySeat: number, seat: number, numPlayers: number): CornerPos | null {
  const offset = (((seat - mySeat) % numPlayers) + numPlayers) % numPlayers;
  if (offset === 0) return null;
  return cornerForOffset(offset);
}

/** 場のカード配置: 自席は "self"(下中央)、他席は corner。 */
export function seatToFieldPos(mySeat: number, seat: number, numPlayers: number): FieldSeatPos {
  const offset = (((seat - mySeat) % numPlayers) + numPlayers) % numPlayers;
  if (offset === 0) return "self";
  return cornerForOffset(offset);
}

/** 各 seat ごとに自分が取った face card を集約。 */
function capturedBySeat(obs: WireObservation, numPlayers: number): string[][] {
  const out: string[][] = Array.from({ length: numPlayers }, () => []);
  for (const t of obs.completed) {
    for (const p of t.plays) {
      const rank = p.card.slice(1);
      if (FACE_RANKS.has(rank)) {
        out[t.winner].push(p.card);
      }
    }
  }
  return out;
}

export interface SelectInput {
  observation: WireObservation | null;
  room: RoomView | null;
  seats: SeatAssignment[] | null;
  mySeat: number | null;
  myEmail: string | null;
  hands_per_match: number | null;
  /** 局番号(0-based の hand_index)。0..hands_per_match-1。 */
  handIndex: number | null;
  /** seat → 切断時刻 (ms epoch)。store.disconnectedSeats を渡す。 */
  disconnectedSeats?: Map<number, number>;
  /**
   * trick 演出 buffer。直前 trick の 5 枚を「勝者吸い寄せ」アニメで表示中の値。
   * App.tsx の state machine が completed 増分検知時に注入する。
   */
  trickAnim?: { plays: WirePlayedCard[]; winnerSeat: number } | null;
}

/**
 * 必要な state が全部揃ってる時だけ vm を返す。
 * Bidding 中で declarer/bid 等が無い場合も partial で OK(BidOverlay 側で対応)。
 */
export function selectGameVm(input: SelectInput): GamePcViewModel | null {
  const { observation, seats, mySeat, hands_per_match, handIndex, disconnectedSeats, trickAnim } =
    input;
  if (!observation || !seats || mySeat === null) return null;

  const seatToName = new Map<number, string>();
  for (const s of seats) seatToName.set(s.seat, s.display_name);
  const nameOf = (seat: number) => seatToName.get(seat) ?? `seat${seat}`;

  const captured = capturedBySeat(observation, observation.num_players);
  const adjSeat = observation.adjutant_player ?? null;

  // opponents: 自分以外の 4 seat を offset で position 割当
  const opponents: OpponentVM[] = [];
  for (let off = 1; off <= 4; off++) {
    const seat = (mySeat + off) % observation.num_players;
    const pos = cornerForOffset(off);
    const isBack = pos === "tl" || pos === "tr";
    const since = disconnectedSeats?.get(seat);
    opponents.push({
      name: nameOf(seat),
      pos,
      depth: isBack ? "back" : "front",
      status: observation.to_act === seat ? "thinking" : "waiting",
      // 残り手札数: 配り 10枚 - 完了 trick 数(各 trick で 1 枚減る)
      cards: Math.max(0, 10 - observation.completed.length),
      captured: captured[seat] ?? [],
      isNapo: observation.declarer === seat,
      isPartner: adjSeat === seat,
      disconnected: since !== undefined,
      disconnectedSinceMs: since,
    });
  }

  // trick 表示: gather 中は trickAnim の 5 枚、通常は current_trick。
  // gather 中の lead はソースが完了 trick(plays[0] = lead)、winner は winnerSeat と一致する seat。
  const trickSource: WirePlayedCard[] = trickAnim?.plays ?? observation.current_trick;
  const trickPhase: TrickPhase = trickAnim ? "gather" : "playing";
  const trickWinnerSeat = trickAnim?.winnerSeat ?? null;
  const trickWinnerPos: FieldSeatPos | null =
    trickWinnerSeat !== null
      ? seatToFieldPos(mySeat, trickWinnerSeat, observation.num_players)
      : null;
  const trick: FieldPlayVM[] = trickSource.map((p: WirePlayedCard, i) => ({
    player: nameOf(p.seat),
    card: p.card,
    rot: i % 2 === 0 ? -4 : 4,
    seat: p.seat,
    pos: seatToFieldPos(mySeat, p.seat, observation.num_players),
    lead: i === 0,
    winner: trickWinnerSeat !== null && p.seat === trickWinnerSeat,
  }));

  const napoSeat = observation.declarer;
  const napoName = napoSeat !== null && napoSeat !== undefined ? nameOf(napoSeat) : "—";

  const trumpFromBid = observation.current_bid?.bid.trump ?? "NT";
  const trumpSuit = trumpSuitFromCode(trumpFromBid) ?? "S";
  const isNoTrump = trumpFromBid === "NT";
  const bidValue = observation.current_bid?.bid.tricks ?? 0;

  // 裏 J ルール: lead suit は単純 surface ではなく、sei/ura-jack が lead なら trump 扱い。
  const leadCard = observation.current_trick[0]?.card;
  const leadSuit = effectiveLeadSuit(leadCard, isNoTrump ? null : trumpSuit, isNoTrump);

  return {
    phase: observation.to_act === mySeat ? "turn" : "wait",
    myHand: sortHand(observation.hand),
    myCaptured: captured[mySeat] ?? [],
    opponents,
    trick,
    trickPhase,
    trickWinnerPos,
    revealedFaceDiscards: sortHand(observation.revealed_face_discards),
    leadSuit,
    napoName,
    trumpSuit,
    isNoTrump,
    bidValue,
    roomId: input.room?.room_id ?? "—",
    gameNo: (handIndex ?? 0) + 1,
    totalGames: hands_per_match ?? 10,
    trickNo: observation.trick_index + 1,
    totalTricks: 10,
  };
}

/** 自分の seat に対応する display_name(無ければ email or null)。 */
export function mySeatName(input: Pick<SelectInput, "seats" | "mySeat" | "myEmail">): string {
  const { seats, mySeat, myEmail } = input;
  if (seats && mySeat !== null) {
    const s = seats.find((x) => x.seat === mySeat);
    if (s) return s.display_name;
  }
  return myEmail ?? "you";
}

/**
 * observation から overlay phase を推定。
 * - declarer 未確定 → "bid"
 * - declarer 決定、adjutant_declared event(現局)未着 → "partner"
 * - adjutant_declared 後、visible_widow が見えている(declarer 視点)→ "widow"
 * - それ以外 → null(通常 play 中、または非 declarer の widow 待ち)
 *
 * 注: 非 declarer の observation.adjutant_card は declarer がカードを公開する
 * (= adjutant_player が確定する)まで常に null。adjutant_card だけ見ると
 * WidowExchange phase でも "partner" と誤判定されるので、events で補完する。
 *
 * events array は match 全体で蓄積されるので、hand またぎで誤判定しないよう
 * 最新 hand_started 以降の event だけ見る。
 */
export function selectOverlayPhase(
  obs: WireObservation | null,
  events: PublicEvent[] = [],
): OverlayPhase {
  if (!obs) return null;
  if (obs.declarer === null) return "bid";

  const eventsThisHand = eventsAfterLastHandStart(events);
  const adjutantDeclared = eventsThisHand.some((e) => e.type === "adjutant_declared");
  if (!adjutantDeclared) return "partner";

  if (obs.visible_widow !== null) return "widow";
  return null;
}

/** events から最後の hand_started 以降の event だけを切り出す。 */
function eventsAfterLastHandStart(events: PublicEvent[]): PublicEvent[] {
  let lastIdx = -1;
  for (let i = events.length - 1; i >= 0; i--) {
    if (events[i].type === "hand_started") {
      lastIdx = i;
      break;
    }
  }
  if (lastIdx < 0) return events;
  return events.slice(lastIdx);
}

/** events から最新の hand_started.hand_index を返す(配信されてなければ null)。 */
export function currentHandIndex(events: PublicEvent[]): number | null {
  for (let i = events.length - 1; i >= 0; i--) {
    const e = events[i];
    if (e.type === "hand_started") return e.payload.hand_index;
  }
  return null;
}

/** WireBid → 表示用テキスト("13 ♠"等)。 */
function bidLabel(bid: WireBid): string {
  const suit = bid.trump === "NT" ? "NT" : suitSymbol(bid.trump);
  return `${bid.tricks} ${suit}`;
}

function suitSymbol(s: WireTrump): string {
  switch (s) {
    case "S":
      return "♠";
    case "H":
      return "♥";
    case "D":
      return "♦";
    case "C":
      return "♣";
    case "NT":
      return "NT";
  }
}

export interface BidSelection {
  opponents: BidSeatVM[];
  currentTopBid: { tricks: number; trump: SuitKey | "NT"; by: string } | null;
  /** 入札可能な最低 trick 数(未入札なら 13、既入札なら +1)。 */
  minTricks: number;
}

/**
 * Bidding 用 overlay のための seat 別 VM を組み立てる。
 * 自分は中央下に固定なので opponents[] には含めない。
 */
export function selectBidVMs(
  input: Pick<SelectInput, "observation" | "seats" | "mySeat">,
): BidSelection | null {
  const { observation, seats, mySeat } = input;
  if (!observation || !seats || mySeat === null) return null;

  const seatToName = new Map<number, string>();
  for (const s of seats) seatToName.set(s.seat, s.display_name);
  const nameOf = (seat: number) => seatToName.get(seat) ?? `seat${seat}`;

  // bid_history は時系列。各 seat の最新エントリだけを引きたい。
  const lastEntry = new Map<number, WireBid | null | undefined>();
  // 何度パスしたかも記録(undefined = 未行動)
  const everPassed = new Set<number>();
  for (const e of observation.bid_history) {
    lastEntry.set(e.seat, e.bid);
    if (e.bid === null) everPassed.add(e.seat);
  }

  const top = observation.current_bid;
  const opponents: BidSeatVM[] = [];
  for (let off = 1; off <= 4; off++) {
    const seat = (mySeat + off) % observation.num_players;
    const pos = cornerForOffset(off);
    const last = lastEntry.get(seat);
    let label: string;
    if (last === undefined) {
      label = "—";
    } else if (last === null) {
      label = "パス";
    } else {
      label = bidLabel(last);
    }
    opponents.push({
      name: nameOf(seat),
      pos,
      bid: label,
      passed: everPassed.has(seat) && top?.seat !== seat,
      leading: top?.seat === seat,
      waiting: observation.to_act === seat,
    });
  }

  const currentTopBid = top
    ? { tricks: top.bid.tricks, trump: top.bid.trump, by: nameOf(top.seat) }
    : null;
  const minTricks = top ? top.bid.tricks + 1 : 13;

  return { opponents, currentTopBid, minTricks };
}

/** declarer の表示名(Bidding 終了後に使用)。 */
export function declarerName(input: Pick<SelectInput, "observation" | "seats">): string | null {
  const { observation, seats } = input;
  if (!observation || !seats) return null;
  const seat = observation.declarer;
  if (seat === null || seat === undefined) return null;
  return seats.find((s) => s.seat === seat)?.display_name ?? `seat${seat}`;
}

export interface EndSelection {
  napoTeam: TeamPlayerVM[];
  armyTeam: TeamPlayerVM[];
  napoWon: boolean;
  bidValue: number;
  taken: number;
  standings: StandingVM[];
}

/**
 * 局終了 overlay 用 vm。lastScores と完了 observation から napo/army 別チーム成績を組み立てる。
 * 必要な情報がそろっていなければ null。
 */
export function selectEndVm(input: {
  observation: WireObservation | null;
  seats: SeatAssignment[] | null;
  lastScores: { scores: number[]; cumulative: number[] } | null;
}): EndSelection | null {
  const { observation, seats, lastScores } = input;
  if (!observation || !seats || !lastScores) return null;
  if (observation.declarer === null || observation.current_bid === null) return null;

  const declarer = observation.declarer;
  const partner = observation.adjutant_player ?? null;

  const tricksByPlayer: number[] = Array.from({ length: observation.num_players }, () => 0);
  for (const t of observation.completed) tricksByPlayer[t.winner]++;

  const captured = capturedBySeat(observation, observation.num_players);

  const seatToName = new Map<number, string>();
  for (const s of seats) seatToName.set(s.seat, s.display_name);
  const nameOf = (seat: number) => seatToName.get(seat) ?? `seat${seat}`;

  const napoTeam: TeamPlayerVM[] = [];
  const armyTeam: TeamPlayerVM[] = [];
  const standings: StandingVM[] = [];
  // 勝敗判定用にナポ側スコア合計だけ持つ(settle は napo 勝利で declarer に正、敗北で負を返す)。
  let napoScore = 0;
  // ナポ側の獲得「絵札枚数」(宣言比較用)。トリック数ではない。
  let taken = 0;

  for (let seat = 0; seat < observation.num_players; seat++) {
    const name = nameOf(seat);
    const score = lastScores.scores[seat] ?? 0;
    const total = lastScores.cumulative[seat] ?? 0;
    const tricks = tricksByPlayer[seat] ?? 0;
    const picture = captured[seat]?.length ?? 0;
    const role: "napo" | "partner" | "army" =
      seat === declarer ? "napo" : seat === partner ? "partner" : "army";

    standings.push({ name, total, delta: score, role });

    if (role === "napo") {
      napoTeam.push({ name, tricks, picture });
      napoScore += score;
      taken += picture;
    } else if (role === "partner") {
      napoTeam.push({ name, tricks, picture, hidden: "partner" });
      napoScore += score;
      taken += picture;
    } else {
      armyTeam.push({ name, tricks, picture });
    }
  }

  return {
    napoTeam,
    armyTeam,
    napoWon: napoScore > 0,
    bidValue: observation.current_bid.bid.tricks,
    taken,
    standings,
  };
}
