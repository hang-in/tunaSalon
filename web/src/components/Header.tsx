import { useState } from "react";
import { Users, Tag, PanelRightOpen, Box, HelpCircle, X, Pause, Play } from "lucide-react";

interface HeaderProps {
  topics: string[];
  connected: boolean;
  participantCount: number;
  onToggleSidebar: () => void;
  bg3d: boolean;
  onToggle3d: () => void;
  paused: boolean;
  onTogglePause: () => void;
}

export function Header({ topics, connected, participantCount, onToggleSidebar, bg3d, onToggle3d, paused, onTogglePause }: HeaderProps) {
  const [helpOpen, setHelpOpen] = useState(false);
  return (
    <header
      className="fixed top-0 left-0 right-0 h-16 z-40 flex items-center px-4 lg:px-6"
      style={{
        background: "rgba(30, 30, 30, 0.85)",
        backdropFilter: "blur(16px)",
        WebkitBackdropFilter: "blur(16px)",
        borderBottom: "1px solid var(--border-color)",
      }}
    >
      {/* Left: Logo + name */}
      <div className="flex items-center gap-2.5 shrink-0">
        <button
          className="lg:hidden p-1.5 rounded-lg hover:bg-white/5 transition-colors"
          onClick={onToggleSidebar}
          aria-label="패널 열기"
        >
          <PanelRightOpen size={18} className="text-[var(--text-secondary)]" />
        </button>
        <div
          className="w-8 h-8 rounded-lg flex items-center justify-center"
          style={{ background: "rgba(229, 164, 74, 0.15)" }}
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
            <path
              d="M12 2C8.5 2 6 4.5 6 7C6 9 7 10.5 8.5 11.5C6 12.5 3 15 3 19C3 20.5 4 22 6 22H18C20 22 21 20.5 21 19C21 15 18 12.5 15.5 11.5C17 10.5 18 9 18 7C18 4.5 15.5 2 12 2Z"
              stroke="#E5A44A"
              strokeWidth="1.5"
              strokeLinecap="round"
              strokeLinejoin="round"
              fill="rgba(229, 164, 74, 0.2)"
            />
            <circle cx="9.5" cy="7.5" r="0.8" fill="#E5A44A" />
            <circle cx="14.5" cy="7.5" r="0.8" fill="#E5A44A" />
            <path d="M10.5 9.5C11 10 13 10 13.5 9.5" stroke="#E5A44A" strokeWidth="0.8" strokeLinecap="round" />
          </svg>
        </div>
        <h1 className="text-lg font-extrabold tracking-tight text-[var(--text-primary)]">
          tunaSalon
        </h1>
      </div>

      {/* Center: Topic chips */}
      <div className="flex-1 mx-4 min-w-0 hidden sm:block">
        <div className="flex items-center gap-1.5 overflow-x-auto no-scrollbar">
          <Tag size={13} className="text-[var(--text-secondary)] shrink-0 mr-1" />
          {topics.map((topic) => (
            <span
              key={topic}
              className="shrink-0 inline-flex items-center px-2.5 py-1 rounded-lg text-xs font-medium"
              style={{
                background: "var(--bg-elevated)",
                color: "var(--text-secondary)",
              }}
            >
              {topic}
            </span>
          ))}
          {topics.length === 0 && (
            <span className="text-xs text-[var(--text-secondary)] italic">주제 없음</span>
          )}
        </div>
      </div>

      {/* Right: Status */}
      <div className="flex items-center gap-3 shrink-0">
        <button
          onClick={onToggle3d}
          className="p-1.5 rounded-lg hover:bg-white/5 transition-colors"
          aria-label="3D 배경 켜기/끄기"
          title={bg3d ? "3D 배경 끄기 (GPU 절약)" : "3D 배경 켜기"}
        >
          <Box
            size={16}
            style={{ color: bg3d ? "var(--accent-warm)" : "var(--text-secondary)" }}
          />
        </button>

        {/* 도움말 버튼 */}
        <div className="relative">
          <button
            onClick={() => setHelpOpen((v) => !v)}
            className="p-1.5 rounded-lg hover:bg-white/5 transition-colors"
            aria-label="도움말"
            title="도움말"
          >
            <HelpCircle
              size={16}
              style={{ color: helpOpen ? "var(--accent-warm)" : "var(--text-secondary)" }}
            />
          </button>

          {helpOpen && (
            <>
              {/* backdrop for closing */}
              <div className="fixed inset-0 z-40" onClick={() => setHelpOpen(false)} />

              <div
                className="absolute right-0 top-9 z-50 w-72 rounded-xl p-4 shadow-2xl"
                style={{
                  background: "var(--bg-surface)",
                  border: "1px solid var(--border-color)",
                }}
              >
                <div className="flex items-center justify-between mb-3">
                  <span className="text-[13px] font-bold text-[var(--text-primary)]">tunaSalon 사용법</span>
                  <button
                    onClick={() => setHelpOpen(false)}
                    className="p-1 rounded-md hover:bg-white/5 transition-colors"
                  >
                    <X size={13} className="text-[var(--text-secondary)]" />
                  </button>
                </div>

                <ul className="space-y-2.5 text-[12px] text-[var(--text-secondary)] leading-relaxed">
                  <li>
                    <span className="font-semibold text-[var(--text-primary)]">메시지 전송</span>
                    <br />
                    Enter로 전송, Shift+Enter로 줄바꿈
                  </li>
                  <li>
                    <span className="font-semibold text-[var(--text-primary)]"># 주제 설정</span>
                    <br />
                    입력창의 # 버튼으로 1-5개 주제 태그 설정 - 페르소나가 그 주제로 대화합니다
                  </li>
                  <li>
                    <span className="font-semibold text-[var(--text-primary)]">발화 욕구(λ) 링</span>
                    <br />
                    사이드바 아바타 외곽 링이 차오를수록 말하고 싶다는 뜻, θ를 넘으면 발화합니다
                  </li>
                  <li>
                    <span className="font-semibold text-[var(--text-primary)]">방 상태</span>
                    <br />
                    흐름: 대화 다양성 / 냉각도: 엔진 활성도
                  </li>
                  <li>
                    <span className="font-semibold text-[var(--text-primary)]">일시정지</span>
                    <br />
                    헤더 일시정지 버튼으로 페르소나 발화를 멈출 수 있습니다. 멈춘 상태에서도 메시지와 주제 전송은 가능합니다.
                  </li>
                  <li>
                    <span className="font-semibold text-[var(--text-primary)]">3D 배경</span>
                    <br />
                    헤더 박스 아이콘으로 켜고 끌 수 있습니다
                  </li>
                </ul>
              </div>
            </>
          )}
        </div>

        {/* 일시정지/재개 버튼 */}
        <button
          onClick={onTogglePause}
          className="flex items-center gap-1.5 p-1.5 rounded-lg hover:bg-white/5 transition-colors"
          aria-label={paused ? "재개" : "일시정지"}
          title={paused ? "재개 - 대화를 다시 시작합니다" : "일시정지 - 페르소나 발화를 멈춥니다"}
          style={{ color: paused ? "var(--accent-warm)" : "var(--text-secondary)" }}
        >
          {paused ? <Play size={16} /> : <Pause size={16} />}
          {paused && (
            <span className="text-xs font-medium hidden sm:inline" style={{ color: "var(--accent-warm)" }}>
              일시정지됨
            </span>
          )}
        </button>

        <div className="flex items-center gap-1.5">
          <div
            className={`w-2 h-2 rounded-full ${connected ? "pulse-dot" : ""}`}
            style={{ background: connected ? "#4ade80" : "#ef4444" }}
          />
          <span className="text-xs font-medium text-[var(--text-secondary)] hidden sm:inline">
            {connected ? "연결됨" : "연결 중..."}
          </span>
        </div>
        <div className="flex items-center gap-1 text-[var(--text-secondary)]">
          <Users size={13} />
          <span className="text-xs font-medium">{participantCount}</span>
        </div>
      </div>
    </header>
  );
}
