import type { ServerFrame, ClientFrame } from "@/types";

/**
 * 실 WebSocket 엔진 커넥터 (자동 재연결).
 * mockEngine.ts와 동일한 시그니처 connect(onFrame, onStatus) -> {send, disconnect}.
 * axum /ws 엔드포인트에 연결, JSON frame을 주고받는다.
 *
 * - 끊기면(onclose/onerror) 지수 백오프로 자동 재연결(1s,2s,4s,8s,최대 10s).
 * - onStatus(connected)로 연결 상태를 알린다(open -> true, close -> false).
 *   서버 재기동 시 새로고침 없이 다시 붙는다.
 */
export function connect(
  onFrame: (frame: ServerFrame) => void,
  onStatus?: (connected: boolean) => void
): { send: (f: ClientFrame) => void; disconnect: () => void } {
  const protocol = location.protocol === "https:" ? "wss" : "ws";
  const url = `${protocol}://${location.host}/ws`;

  let ws: WebSocket | null = null;
  let retry = 0;
  let closedByUser = false;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

  const open = () => {
    ws = new WebSocket(url);

    ws.onopen = () => {
      retry = 0;
      onStatus?.(true);
    };

    ws.onmessage = (e) => {
      try {
        onFrame(JSON.parse(e.data as string) as ServerFrame);
      } catch {
        // JSON 파싱 실패는 조용히 무시
      }
    };

    ws.onclose = () => {
      onStatus?.(false);
      if (closedByUser) return;
      // 지수 백오프(최대 10s)로 재연결
      const delay = Math.min(1000 * 2 ** retry, 10000);
      retry++;
      reconnectTimer = setTimeout(open, delay);
    };

    ws.onerror = () => {
      // 오류 시 소켓을 닫아 onclose 재연결 경로를 탄다.
      ws?.close();
    };
  };

  open();

  return {
    send(frame: ClientFrame) {
      if (ws && ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify(frame));
      }
    },
    disconnect() {
      closedByUser = true;
      if (reconnectTimer) clearTimeout(reconnectTimer);
      ws?.close();
    },
  };
}
