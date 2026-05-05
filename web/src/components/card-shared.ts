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

/** 下半分の pip は 180° 回転して表示する(orthodox playing card 慣例)。 */
export function isBottomHalf(y: number): boolean {
  return y > 0.5;
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
