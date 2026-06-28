import { useEffect, useState } from "react";
import { ArrowLeft, MessageSquareText, Loader2, Archive } from "lucide-react";

/** GET /api/rooms 응답 항목(서버 영속 방 1개). */
export interface RoomListItem {
  room_id: string;
  topics: string[];
  updated_at: number; // unix 초
  created_at?: number; // 방 최초 생성(구 데이터는 없음)
  concluded_at?: number; // 마지막 결론 시각(결론 없으면 없음)
  concluded: boolean;
  report_count: number;
}

interface ArchivePageProps {
  /** 카드 클릭 시 해당 방으로 입장. */
  onEnter: (item: RoomListItem) => void;
  /** 로비로 돌아가기. */
  onBack: () => void;
}

/** unix 초 -> "YYYY.MM.DD". 0/없으면 빈 문자열. */
function fmtDate(unixSec?: number): string {
  if (!unixSec) return "";
  const d = new Date(unixSec * 1000);
  const mm = String(d.getMonth() + 1).padStart(2, "0");
  const dd = String(d.getDate()).padStart(2, "0");
  return `${d.getFullYear()}.${mm}.${dd}`;
}

/** unix 초 -> "N분/시간/일 전" 또는 날짜. updated_at=0(구 데이터)이면 빈 문자열. */
function timeAgo(unixSec: number): string {
  if (!unixSec) return "";
  const diff = Date.now() / 1000 - unixSec;
  if (diff < 60) return "방금";
  if (diff < 3600) return `${Math.floor(diff / 60)}분 전`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}시간 전`;
  if (diff < 86400 * 30) return `${Math.floor(diff / 86400)}일 전`;
  return new Date(unixSec * 1000).toISOString().slice(0, 10);
}

export function ArchivePage({ onEnter, onBack }: ArchivePageProps) {
  const [rooms, setRooms] = useState<RoomListItem[] | null>(null);
  const [error, setError] = useState(false);

  useEffect(() => {
    let cancelled = false;
    fetch("/api/rooms")
      .then((r) => (r.ok ? r.json() : Promise.reject(new Error("fetch failed"))))
      .then((data: RoomListItem[]) => {
        if (!cancelled) setRooms(Array.isArray(data) ? data : []);
      })
      .catch(() => {
        if (!cancelled) setError(true);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div className="h-[100dvh] w-screen overflow-hidden" style={{ background: "var(--bg-base)" }}>
      <main className="h-full overflow-y-auto">
        <div className="min-h-full px-4 py-8">
          <section className="w-full max-w-4xl mx-auto" style={{ color: "var(--text-primary)" }}>
            {/* 헤더: 뒤로 + 제목 */}
            <div className="flex items-center gap-3 mb-8">
              <button
                onClick={onBack}
                className="w-10 h-10 rounded-lg flex items-center justify-center transition-colors hover:bg-white/5"
                style={{ background: "var(--bg-elevated)", color: "var(--text-secondary)" }}
                aria-label="로비로 돌아가기"
                title="로비로 돌아가기"
              >
                <ArrowLeft size={18} />
              </button>
              <div className="flex items-center gap-2 flex-1 min-w-0">
                <Archive size={20} className="text-[var(--accent-warm)] shrink-0" />
                <h1 className="text-xl font-extrabold tracking-tight">이전 토론들</h1>
              </div>
            </div>

            {/* 본문 */}
            {rooms === null && !error && (
              <div className="flex items-center justify-center gap-2 py-16 text-[var(--text-secondary)]">
                <Loader2 size={18} className="animate-spin" />
                <span className="text-sm">불러오는 중...</span>
              </div>
            )}

            {error && (
              <div
                className="rounded-lg p-5"
                style={{ background: "var(--bg-surface)", border: "1px solid var(--border-color)" }}
              >
                <p className="text-sm text-[var(--text-secondary)]">
                  방 목록을 불러오지 못했습니다. 잠시 후 다시 시도해 주세요.
                </p>
              </div>
            )}

            {rooms !== null && !error && rooms.length === 0 && (
              <div
                className="rounded-lg p-5"
                style={{ background: "var(--bg-surface)", border: "1px solid var(--border-color)" }}
              >
                <h2 className="text-base font-bold mb-1">저장된 토론방이 없습니다</h2>
                <p className="text-sm text-[var(--text-secondary)]">
                  로비에서 토론방을 만들면 여기에 모두 보관됩니다.
                </p>
              </div>
            )}

            {rooms !== null && rooms.length > 0 && (
              <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
                {rooms.map((item) => {
                  const title = item.topics[0]?.trim() || item.room_id;
                  const extra = item.topics.filter((t) => t.trim() && t.trim() !== title.trim());
                  const ago = timeAgo(item.updated_at);
                  return (
                    <div
                      key={item.room_id}
                      className="rounded-lg p-4"
                      style={{ background: "var(--bg-surface)", border: "1px solid var(--border-color)" }}
                    >
                      <div className="flex items-center justify-between gap-3 mb-3">
                        <span className="flex items-center gap-2 min-w-0">
                          <MessageSquareText size={18} className="text-[var(--accent-warm)] shrink-0" />
                          {item.concluded && (
                            <span
                              className="text-[10px] font-bold px-1.5 py-0.5 rounded-md shrink-0"
                              style={{
                                background: "rgba(74, 222, 128, 0.12)",
                                color: "#4ade80",
                                border: "1px solid rgba(74, 222, 128, 0.25)",
                              }}
                            >
                              결론 남
                            </span>
                          )}
                        </span>
                        {ago && (
                          <span className="text-[10px] text-[var(--text-secondary)] shrink-0">{ago}</span>
                        )}
                      </div>
                      <h2 className="text-base font-bold leading-snug mb-2">{title}</h2>
                      {/* 시작 / 결론 날짜 */}
                      {(fmtDate(item.created_at) || item.concluded_at) && (
                        <div className="flex flex-wrap gap-x-3 gap-y-0.5 text-[11px] text-[var(--text-secondary)] mb-1">
                          {fmtDate(item.created_at) && <span>시작 {fmtDate(item.created_at)}</span>}
                          {item.concluded_at && (
                            <span style={{ color: "#4ade80" }}>결론 {fmtDate(item.concluded_at)}</span>
                          )}
                        </div>
                      )}
                      {item.report_count > 0 && (
                        <p className="text-xs text-[var(--text-secondary)]">
                          리포트 {item.report_count}개
                        </p>
                      )}
                      {extra.length > 0 && (
                        <div className="mt-3 flex flex-wrap gap-1.5">
                          {extra.slice(0, 3).map((topic) => (
                            <span
                              key={topic}
                              className="px-2 py-0.5 rounded-md text-[11px] font-medium"
                              style={{ background: "var(--bg-elevated)", color: "var(--text-secondary)" }}
                            >
                              {topic}
                            </span>
                          ))}
                        </div>
                      )}
                      <button
                        onClick={() => onEnter(item)}
                        className="mt-4 w-full h-9 rounded-lg text-sm font-semibold"
                        style={{ background: "var(--bg-elevated)", color: "var(--accent-warm)" }}
                      >
                        토론방 입장
                      </button>
                    </div>
                  );
                })}
              </div>
            )}
          </section>
        </div>
      </main>
    </div>
  );
}
