import type { ServerFrame, ClientFrame } from "@/types";

/**
 * 실 WebSocket 엔진 커넥터.
 * mockEngine.ts와 동일한 시그니처 `connect(onFrame) -> {send, disconnect}`.
 * axum /ws 엔드포인트에 연결, JSON frame을 주고받는다.
 */
export function connect(
  onFrame: (frame: ServerFrame) => void
): { send: (f: ClientFrame) => void; disconnect: () => void } {
  const protocol = location.protocol === "https:" ? "wss" : "ws";
  const url = `${protocol}://${location.host}/ws`;
  const ws = new WebSocket(url);

  ws.onmessage = (e) => {
    try {
      const frame = JSON.parse(e.data as string) as ServerFrame;
      onFrame(frame);
    } catch {
      // JSON 파싱 실패는 조용히 무시
    }
  };

  ws.onerror = () => {
    // 연결 오류는 조용히 무시 (브라우저 콘솔에 자동 출력됨)
  };

  return {
    send(frame: ClientFrame) {
      if (ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify(frame));
      }
    },
    disconnect() {
      ws.close();
    },
  };
}
