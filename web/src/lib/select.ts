// Zustand store state(WS から流れてくる observation / room / seats)を
// design 由来の GamePcViewModel に変換する pure 関数群。
//
// Phase E-3 で App.tsx から呼んで GamePC / GameMobile に渡す。

import type { CornerPos, FieldPlayVM, GamePcViewModel, OpponentVM } from "../components/GamePC";
import type { SuitKey } from "../components/card-shared";
import type { RoomView, SeatAssignment, WireObservation, WirePlayedCard } from "../types";

const FACE_RANKS = new Set(["A", "K", "Q", "J", "10"]);

/** "S","H","D","C" を返す。"NT" の場合は null。 */
export function trumpSuitFromCode(s: string): SuitKey | null {
  return s === "S" || s === "H" || s === "D" || s === "C" ? s : null;
}

/** 自分(mySeat)を中心に、(mySeat+offset)%5 を tl/tr/bl/br に割り当てる。 */
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

/** 副官 seat。observation.adjutant_player が確定していたらそれ、なければ null。 */
function adjutantSeat(obs: WireObservation): number | null {
  return obs.adjutant_player ?? null;
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
  const adjSeat = adjutantSeat(observation);

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

/** observation から「現在 phase が Bidding か」を推定(declarer 未定なら Bidding)。 */
export function isBidding(obs: WireObservation | null): boolean {
  if (!obs) return false;
  return obs.declarer === null;
}
