import { useEffect, useState } from "react";
import { useParams } from "react-router";
import { Loader2, MessageCircle } from "lucide-react";
import { personaColorSet } from "@/lib/personaAvatar";

interface ShareAxes {
  blood: string;
  mbti: string;
  zodiac: string;
  role: string;
}
interface ShareParticipant {
  id: string;
  name: string;
  model?: string;
  axes?: ShareAxes;
}
interface ShareMessage {
  speaker: string;
  name: string;
  content: string;
  ts: number;
}
interface ShareReport {
  seq: number;
  created_at: number;
  topic: string;
  markdown: string;
  conclusion: string;
}
interface ShareData {
  topics: string[];
  participants: ShareParticipant[];
  messages: ShareMessage[];
  reports: ShareReport[];
}

type LoadState = "loading" | "error" | "notfound" | ShareData;

export function ShareViewPage() {
  const { token } = useParams<{ token: string }>();
  const [state, setState] = useState<LoadState>("loading");

  useEffect(() => {
    if (!token) {
      setState("notfound");
      return;
    }
    let cancelled = false;
    fetch(`/api/share/${encodeURIComponent(token)}`)
      .then((r) => (r.ok ? r.json() : Promise.reject(new Error("http"))))
      .then((d: ShareData | null) => {
        if (cancelled) return;
        setState(d && Array.isArray(d.messages) ? d : "notfound");
      })
      .catch(() => {
        if (!cancelled) setState("error");
      });
    return () => {
      cancelled = true;
    };
  }, [token]);

  return (
    <div className="h-[100dvh] w-screen overflow-hidden" style={{ background: "var(--bg-base)" }}>
      <main className="h-full overflow-y-auto">
        <div className="mx-auto w-full max-w-2xl px-4 py-6" style={{ color: "var(--text-primary)" }}>
          {state === "loading" && (
            <div className="flex items-center justify-center gap-2 py-20 text-[var(--text-secondary)]">
              <Loader2 size={18} className="animate-spin" />
              <span className="text-sm">불러오는 중...</span>
            </div>
          )}

          {(state === "error" || state === "notfound") && (
            <div className="py-20 text-center">
              <p className="text-base font-bold mb-1">토론을 찾을 수 없습니다</p>
              <p className="text-sm text-[var(--text-secondary)]">
                링크가 만료되었거나 잘못된 주소일 수 있습니다.
              </p>
            </div>
          )}

          {typeof state === "object" && <ShareBody data={state} />}
        </div>
      </main>
    </div>
  );
}

function ShareBody({ data }: { data: ShareData }) {
  const title = data.topics[0]?.trim() || "토론";
  const extra = data.topics.filter((t) => t.trim() && t.trim() !== title.trim());

  const colorOf = (speaker: string): string => {
    const p = data.participants.find((x) => x.id === speaker);
    if (p) return personaColorSet(p.id, p.axes?.blood).color;
    return "var(--text-secondary)";
  };
  const isHuman = (speaker: string) => !data.participants.some((p) => p.id === speaker);

  return (
    <>
      {/* 헤더 */}
      <header className="mb-6">
        <div className="flex items-center gap-1.5 text-[var(--text-secondary)] mb-2">
          <MessageCircle size={14} style={{ color: "var(--accent-warm)" }} />
          <span className="text-xs font-semibold tracking-tight">tunaSalon · 토론 공유</span>
        </div>
        <h1 className="text-xl font-extrabold leading-snug">{title}</h1>
        {extra.length > 0 && (
          <div className="mt-2 flex flex-wrap gap-1.5">
            {extra.map((topic) => (
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
        {/* 참가자 */}
        {data.participants.length > 0 && (
          <div className="mt-3 flex flex-wrap gap-x-3 gap-y-1">
            {data.participants.map((p) => (
              <span key={p.id} className="text-[11px] font-medium" style={{ color: colorOf(p.id) }}>
                {p.name}
              </span>
            ))}
          </div>
        )}
      </header>

      {/* 전사 */}
      <div className="flex flex-col gap-3">
        {data.messages.map((m, i) => {
          const human = isHuman(m.speaker);
          return (
            <div key={i} className={`flex flex-col ${human ? "items-end" : "items-start"}`}>
              <span
                className="text-[11px] mb-1 px-1"
                style={{ color: human ? "var(--text-secondary)" : colorOf(m.speaker) }}
              >
                {m.name}
              </span>
              <div
                className={`max-w-[85%] px-4 py-2.5 text-[15px] leading-relaxed ${
                  human ? "rounded-2xl rounded-br-sm" : "rounded-2xl rounded-bl-sm"
                }`}
                style={{
                  background: human ? "var(--bg-elevated)" : "var(--bg-surface)",
                  color: "var(--text-primary)",
                }}
              >
                {m.content}
              </div>
            </div>
          );
        })}
        {data.messages.length === 0 && (
          <p className="text-sm text-[var(--text-secondary)] py-8 text-center">
            아직 발화가 없는 토론입니다.
          </p>
        )}
      </div>

      {/* 결론 리포트 */}
      {data.reports.length > 0 && (
        <section className="mt-8">
          <h2 className="text-sm font-bold text-[var(--text-secondary)] mb-3">결론</h2>
          <div className="flex flex-col gap-3">
            {data.reports.map((r) => (
              <div
                key={r.seq}
                className="rounded-lg p-4"
                style={{ background: "var(--bg-surface)", border: "1px solid var(--border-color)" }}
              >
                {r.topic && <p className="text-xs text-[var(--text-secondary)] mb-1.5">{r.topic}</p>}
                <p className="text-sm leading-relaxed whitespace-pre-wrap">{r.conclusion}</p>
              </div>
            ))}
          </div>
        </section>
      )}

      <footer className="mt-10 pb-4 text-center">
        <span className="text-[11px] text-[var(--text-secondary)] opacity-60">
          읽기 전용 · tunaSalon
        </span>
      </footer>
    </>
  );
}
