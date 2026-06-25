import { Fragment } from "react";
import type { ReactNode } from "react";

// 메시지 본문을 가독성 있게 렌더한다:
//  - 참가자 닉네임 → 그 persona 색으로 굵게 강조(상대를 부르는 게 눈에 띄게).
//  - **굵게** 마크다운 → <strong>.
//  - 빈 줄(\n\n) → 문단 분리, 단일 줄바꿈(\n) → <br>.
// 닉네임 색상은 markdown으로 표현 불가하므로 커스텀 렌더가 필요하다.

export interface Mention {
  name: string;
  color: string;
}

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function renderInline(text: string, mentions: Mention[], keyBase: string): ReactNode[] {
  // "나" 같은 1글자/흔한 토큰은 오탐이 심하므로 2글자 이상만 강조 대상.
  const valid = mentions.filter((m) => m.name && m.name.length >= 2);
  const colorOf = new Map(valid.map((m) => [m.name, m.color]));
  // 긴 이름 먼저 매칭(부분 일치 방지).
  const names = valid
    .map((m) => m.name)
    .sort((a, b) => b.length - a.length)
    .map(escapeRegExp);
  const nameAlt = names.length ? `|(${names.join("|")})` : "";
  const re = new RegExp(`(\\*\\*[^*]+\\*\\*)${nameAlt}`, "g");

  const out: ReactNode[] = [];
  let last = 0;
  let k = 0;
  let m = re.exec(text);
  while (m !== null) {
    if (m.index > last) out.push(text.slice(last, m.index));
    if (m[1]) {
      out.push(<strong key={`${keyBase}-b${k++}`}>{m[1].slice(2, -2)}</strong>);
    } else if (m[2]) {
      const color = colorOf.get(m[2]) ?? "var(--accent-warm)";
      out.push(
        <strong key={`${keyBase}-n${k++}`} style={{ color, fontWeight: 700 }}>
          {m[2]}
        </strong>
      );
    }
    last = m.index + m[0].length;
    m = re.exec(text);
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}

export function RichText({ content, mentions }: { content: string; mentions: Mention[] }) {
  const paras = content.split(/\n{2,}/);
  return (
    <>
      {paras.map((para, pi) => (
        <p key={pi} className={pi > 0 ? "mt-2" : ""}>
          {para.split("\n").map((line, li) => (
            <Fragment key={li}>
              {li > 0 && <br />}
              {renderInline(line, mentions, `${pi}-${li}`)}
            </Fragment>
          ))}
        </p>
      ))}
    </>
  );
}
