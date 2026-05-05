// WebSocket connection helper. ts-rs で生成された ServerMessage / ClientMessage 型を扱う。
import type { ClientMessage, ServerMessage } from "../types";

export interface WsHandlers {
  onOpen: () => void;
  onMessage: (msg: ServerMessage) => void;
  onClose: () => void;
  onError?: (err: Event) => void;
}

/** dev_email クエリ付きの ws(s):// URL を組み立てる。 */
export function buildWsUrl(devEmail?: string): string {
  const url = new URL("/ws", window.location.origin);
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  if (devEmail) url.searchParams.set("dev_email", devEmail);
  return url.toString();
}

/**
 * WebSocket を開いて handlers を登録する。返り値の `send` で ClientMessage を送れる。
 * 切断・再接続は呼び出し側で管理(close → 再 openWebSocket)。
 */
export function openWebSocket(url: string, handlers: WsHandlers) {
  const ws = new WebSocket(url);
  ws.onopen = handlers.onOpen;
  ws.onclose = handlers.onClose;
  if (handlers.onError) ws.onerror = handlers.onError;
  ws.onmessage = (ev) => {
    let msg: ServerMessage;
    try {
      msg = JSON.parse(ev.data) as ServerMessage;
    } catch (e) {
      console.error("failed to parse ServerMessage", ev.data, e);
      return;
    }
    handlers.onMessage(msg);
  };
  return {
    send(msg: ClientMessage) {
      if (ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify(msg));
      } else {
        console.warn("ws not open, drop:", msg);
      }
    },
    close() {
      ws.close();
    },
    raw: ws,
  };
}
