/// <reference types="vite/client" />

// vite.config.ts의 define으로 빌드 시 주입되는 버전 문자열(git short hash + 날짜).
declare const __BUILD_VERSION__: string;
