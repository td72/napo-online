// Zustand store state(WS から流れてくる observation / room / seats)を
// design 由来の GamePcViewModel / Overlay VMs に変換する pure 関数群。

import type { CornerPos, FieldPlayVM, GamePcViewModel, OpponentVM } from "../components/GamePC";
import type { BidSeatVM, OverlayPhase } from "../components/Overlays";
import type { SuitKey } from "../components/card-shared";
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
  // 自分は bottom 中央。時計回りで:
  //   +1 → 右隣(手前右)= br
  //   +2 → 向かい右(奥右)= tr
  //   +3 → 向かい左(奥左)= tl
  //   +4 → 左隣(手前左)= bl
  switch (offset) {
    case 1:
      return "br";
    case 2:
      return "tr";
    case 3:
      return "tl";
    case 4:
      return "bl";
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
}

/**
 * 必要な state が全部揃ってる時だけ vm を返す。
 * Bidding 中で declarer/bid 等が無い場合も partial で OK(BidOverlay 側で対応)。
 */
export function selectGameVm(input: SelectInput): GamePcViewModel | null {
  const { observation, seats, mySeat, hands_per_match, handIndex } = input;
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
    });
  }

  // current trick → FieldPlayVM[]
  const trick: FieldPlayVM[] = observation.current_trick.map((p: WirePlayedCard, i) => ({
    player: nameOf(p.seat),
    card: p.card,
    rot: i % 2 === 0 ? -4 : 4,
    lead: i === 0,
  }));

  const leadCard = observation.current_trick[0]?.card;
  const leadSuit = leadCard ? trumpSuitFromCode(leadCard[0]) : null;

  const napoSeat = observation.declarer;
  const napoName = napoSeat !== null && napoSeat !== undefined ? nameOf(napoSeat) : "—";

  const trumpFromBid = observation.current_bid?.bid.trump ?? "NT";
  const trumpSuit = trumpSuitFromCode(trumpFromBid) ?? "S";
  const isNoTrump = trumpFromBid === "NT";
  const bidValue = observation.current_bid?.bid.tricks ?? 0;

  return {
    phase: observation.to_act === mySeat ? "turn" : "wait",
    myHand: observation.hand,
    myCaptured: captured[mySeat] ?? [],
    opponents,
    trick,
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
 * observation から「現在 phase が Bidding か」を推定。
 * Rust 側 Phase::Bidding は declarer 未確定 + (bid_history が空 or current_bid が null と not all-pass)。
 * adjutant_card 未確定で declarer あり → 副官指名待ち。
 */
export function selectOverlayPhase(obs: WireObservation | null): OverlayPhase {
  if (!obs) return null;
  if (obs.declarer === null) return "bid";
  if (obs.adjutant_card === null) return "partner";
  return null;
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
