import path from "path"
import react from "@vitejs/plugin-react"
import { defineConfig } from "vite"

// https://vite.dev/config/
export default defineConfig({
  base: '/',
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
