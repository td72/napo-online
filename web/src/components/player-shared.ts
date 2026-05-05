// Player.tsx と切り離した shared な型 + 関数(only-export-components 対応)。

const PLAYER_COLORS = ["#C84545", "#4A6FA5", "#7A8B5C", "#D67843", "#8B6FA5", "#5C7A8B"];

/** 日本語/ASCII の名前を簡易ハッシュして PLAYER_COLORS から色を選ぶ。 */
export function colorForName(name: string): string {
  if (!name) return "#888";
  let h = 0;
  for (let i = 0; i < name.length; i++) {
    h = (h * 31 + name.charCodeAt(i)) | 0;
  }
  return PLAYER_COLORS[Math.abs(h) % PLAYER_COLORS.length];
}

export type Team = "napo" | "army" | null;

export type PlayerStatus = "thinking" | "played" | "pass" | "waiting" | "napoleon" | "";

export const STATUS_TEXT: Record<PlayerStatus, string> = {
  thinking: "考え中…",
  played: "出した",
  pass: "パス",
  waiting: "待機",
  napoleon: "ナポレオン",
  "": "",
};
