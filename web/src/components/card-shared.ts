// Card 関連の type と const ヘルパ。Card.tsx (component file) と分けることで
// React Fast Refresh の only-export-components ルールを通す。

export const SUITS = {
  S: { sym: "♠", name: "spade", color: "var(--suit-spade)" },
  H: { sym: "♥", name: "heart", color: "var(--suit-heart)" },
  D: { sym: "♦", name: "diamond", color: "var(--suit-diamond)" },
  C: { sym: "♣", name: "club", color: "var(--suit-club)" },
} as const;

export type SuitKey = keyof typeof SUITS;
export type SuitMeta = (typeof SUITS)[SuitKey];

export type CardCode =
  | "JK"
  | `${SuitKey}${"A" | "K" | "Q" | "J" | "10" | "9" | "8" | "7" | "6" | "5" | "4" | "3" | "2"}`;

export type Highlight = "mighty" | "jack" | "partner" | null;

export interface ParsedCard {
  suit: SuitKey | "JK";
  rank: string;
}

export function parseCard(code: string): ParsedCard {
  if (!code || code === "JK") return { suit: "JK", rank: "JK" };
  return { suit: code[0] as SuitKey, rank: code.slice(1) };
}

const SUIT_ORDER: Record<SuitKey | "JK", number> = { C: 0, D: 1, S: 2, H: 3, JK: 4 };
const RANK_ORDER: Record<string, number> = {
  A: 0,
  K: 1,
  Q: 2,
  J: 3,
  "10": 4,
  "9": 5,
  "8": 6,
  "7": 7,
  "6": 8,
  "5": 9,
  "4": 10,
  "3": 11,
  "2": 12,
};

/**
 * 手札表示用ソート。suit ♣♦♠♥ → JK の順、各 suit 内は A,K,Q,J,10..2 の強い順。
 * ゲームロジックには影響させず、見た目だけ並べる。
 */
export function sortHand(cards: readonly string[]): string[] {
  return [...cards].sort((a, b) => {
    const pa = parseCard(a);
    const pb = parseCard(b);
    const sa = SUIT_ORDER[pa.suit] ?? 99;
    const sb = SUIT_ORDER[pb.suit] ?? 99;
    if (sa !== sb) return sa - sb;
    return (RANK_ORDER[pa.rank] ?? 99) - (RANK_ORDER[pb.rank] ?? 99);
  });
}

/** 下半分の pip は 180° 回転して表示する(orthodox playing card 慣例)。 */
export function isBottomHalf(y: number): boolean {
  return y > 0.5;
}

/**
 * lead suit を返す。napo-core v0.2.0 以降は単純に先頭カードの表面スート。
 * (旧仕様では 裏J/正J が lead した場合に切り札スート扱いだったが廃止)
 */
export function effectiveLeadSuit(
  leadCard: string | undefined,
  _trumpSuit: SuitKey | null,
  _isNoTrump: boolean,
): SuitKey | null {
  if (!leadCard) return null;
  const surface = leadCard[0] as SuitKey;
  if (!SUITS[surface]) return null;
  return surface;
}

/**
 * 「この card は lead suit に属するか」(マスト・フォロー判定)。
 * napo-core v0.2.0 以降は **表面スート** のみで判定する(裏J/正J も例外なし)。
 * trumpSuit / isNoTrump 引数は将来の拡張用に残してあるが現在は未使用。
 */
export function belongsToLead(
  card: string,
  leadSuit: SuitKey,
  _trumpSuit: SuitKey | null,
  _isNoTrump: boolean,
): boolean {
  if (card === "JK") return false;
  const surface = card[0] as SuitKey;
  return surface === leadSuit;
}

/** pip 配置 — 0..1 の (column, row) 座標。 */
export type PipLayout = ReadonlyArray<readonly [number, number]>;

export const PIP_LAYOUTS: Record<string, PipLayout> = {
  "2": [
    [0.5, 0.22],
    [0.5, 0.78],
  ],
  "3": [
    [0.5, 0.22],
    [0.5, 0.5],
    [0.5, 0.78],
  ],
  "4": [
    [0.28, 0.22],
    [0.72, 0.22],
    [0.28, 0.78],
    [0.72, 0.78],
  ],
  "5": [
    [0.28, 0.22],
    [0.72, 0.22],
    [0.5, 0.5],
    [0.28, 0.78],
    [0.72, 0.78],
  ],
  "6": [
    [0.28, 0.22],
    [0.72, 0.22],
    [0.28, 0.5],
    [0.72, 0.5],
    [0.28, 0.78],
    [0.72, 0.78],
  ],
  "7": [
    [0.28, 0.22],
    [0.72, 0.22],
    [0.5, 0.36],
    [0.28, 0.5],
    [0.72, 0.5],
    [0.28, 0.78],
    [0.72, 0.78],
  ],
  "8": [
    [0.28, 0.22],
    [0.72, 0.22],
    [0.5, 0.36],
    [0.28, 0.5],
    [0.72, 0.5],
    [0.5, 0.64],
    [0.28, 0.78],
    [0.72, 0.78],
  ],
  "9": [
    [0.28, 0.22],
    [0.72, 0.22],
    [0.28, 0.4],
    [0.72, 0.4],
    [0.5, 0.5],
    [0.28, 0.6],
    [0.72, 0.6],
    [0.28, 0.78],
    [0.72, 0.78],
  ],
  "10": [
    [0.28, 0.18],
    [0.72, 0.18],
    [0.5, 0.3],
    [0.28, 0.4],
    [0.72, 0.4],
    [0.28, 0.6],
    [0.72, 0.6],
    [0.5, 0.7],
    [0.28, 0.82],
    [0.72, 0.82],
  ],
};
