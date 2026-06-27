import { type ReactNode } from "react";

// 토론 리포트 전용 경량 마크다운 렌더. 의존성 없이 헤딩(##)·불릿(-)·**굵게**만 처리.
// 리포트는 제한된 마크다운만 쓰므로 풀 파서가 불필요하다.

function inlineBold(text: string, key: string): ReactNode[] {
  const out: ReactNode[] = [];
  const re = /\*\*([^*]+)\*\*/g;
  let last = 0;
  let k = 0;
  let m = re.exec(text);
  while (m !== null) {
    if (m.index > last) out.push(text.slice(last, m.index));
    out.push(<strong key={`${key}-b${k++}`}>{m[1]}</strong>);
    last = m.index + m[0].length;
    m = re.exec(text);
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}

export function ReportMarkdown({ content }: { content: string }) {
  const lines = content.split("\n");
  const blocks: ReactNode[] = [];
  let bullets: ReactNode[][] = [];

  const flushBullets = (key: string) => {
    if (bullets.length === 0) return;
    blocks.push(
      <ul key={`ul-${key}`} className="list-disc pl-5 my-1 space-y-0.5">
        {bullets.map((b, i) => (
          <li key={i}>{b}</li>
        ))}
      </ul>,
    );
    bullets = [];
  };

  lines.forEach((raw, i) => {
    const line = raw.trimEnd();
    if (/^#{1,6}\s/.test(line)) {
      flushBullets(`h${i}`);
      const text = line.replace(/^#{1,6}\s+/, "");
      blocks.push(
        <h4
          key={i}
          className="text-[13px] font-bold mt-3 mb-1 first:mt-0"
          style={{ color: "var(--accent-warm)" }}
        >
          {inlineBold(text, `h${i}`)}
        </h4>,
      );
    } else if (/^[-*]\s/.test(line)) {
      bullets.push(inlineBold(line.replace(/^[-*]\s+/, ""), `b${i}`));
    } else if (line.trim() === "") {
      flushBullets(`e${i}`);
    } else {
      flushBullets(`p${i}`);
      blocks.push(
        <p key={i} className="my-1 leading-relaxed">
          {inlineBold(line, `p${i}`)}
        </p>,
      );
    }
  });
  flushBullets("end");

  return <>{blocks}</>;
}
