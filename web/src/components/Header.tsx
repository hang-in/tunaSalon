import { Users, Tag, PanelRightOpen } from "lucide-react";

interface HeaderProps {
  topics: string[];
  connected: boolean;
  participantCount: number;
  onToggleSidebar: () => void;
}

export function Header({ topics, connected, participantCount, onToggleSidebar }: HeaderProps) {
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
