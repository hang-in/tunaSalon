import path from "path"
import { execSync } from "child_process"
import react from "@vitejs/plugin-react"
import { defineConfig } from "vite"

// 빌드 시점 버전: git short hash + 빌드 날짜. 채팅방 헤더에 표시해 배포 반영 여부를
// 눈으로 확인한다(빌드만 하고 restart 안 하면 라이브가 안 바뀌는 함정 대비).
function buildVersion(): string {
  try {
    const hash = execSync("git rev-parse --short HEAD").toString().trim();
    const date = new Date().toISOString().slice(0, 10);
    return `${hash} (${date})`;
  } catch {
    return "dev";
  }
}

// https://vite.dev/config/
export default defineConfig({
  base: '/',
  define: {
    __BUILD_VERSION__: JSON.stringify(buildVersion()),
  },
  // kimi-plugin-inspect-react(inspectAttr)는 모든 JSX에 code-path 속성을 주입해
  // React.Fragment에 "Invalid prop code-path" 콘솔 경고를 유발하므로 제거했다(Kimi 스캐폴드 dev 도구).
  plugins: [react()],
  server: {
    host: "0.0.0.0",
    port: 6173,
    strictPort: true,
    proxy: {
      "/ws": {
        target: "ws://localhost:8080",
        ws: true,
      },
      "/api": {
        target: "http://localhost:8080",
      },
    },
  },
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
});
