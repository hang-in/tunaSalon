import { useState, useMemo } from "react";
import { UserPlus } from "lucide-react";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { indianName } from "@/lib/indianName";

interface InvitePanelProps {
  /** human 제외 현재 persona 수 */
  personaCount: number;
  onInvite: (blood: string, mbti: string, zodiac: string, role?: string) => void;
}

const BLOOD_OPTIONS = [
  { value: "A", label: "A형" },
  { value: "B", label: "B형" },
  { value: "O", label: "O형" },
  { value: "AB", label: "AB형" },
];

const MBTI_OPTIONS = [
  "ENTP","ENTJ","ENFP","ENFJ",
  "ESTP","ESTJ","ESFP","ESFJ",
  "INTP","INTJ","INFP","INFJ",
  "ISTP","ISTJ","ISFP","ISFJ",
];

const ZODIAC_OPTIONS = [
  { value: "ari", label: "양자리" },
  { value: "tau", label: "황소자리" },
  { value: "gem", label: "쌍둥이자리" },
  { value: "can", label: "게자리" },
  { value: "leo", label: "사자자리" },
  { value: "vir", label: "처녀자리" },
  { value: "lib", label: "천칭자리" },
  { value: "sco", label: "전갈자리" },
  { value: "sag", label: "사수자리" },
  { value: "cap", label: "염소자리" },
  { value: "aqu", label: "물병자리" },
  { value: "pis", label: "물고기자리" },
];

const ROLE_OPTIONS = [
  { value: "friend",     label: "friend - 친구" },
  { value: "chaos",      label: "chaos - 와일드카드" },
  { value: "critic",     label: "critic - 비평가" },
  { value: "realist",    label: "realist - 현실주의자" },
  { value: "teacher",    label: "teacher - 교사" },
  { value: "poet",       label: "poet - 시인" },
  { value: "strategist", label: "strategist - 전략가" },
  { value: "summarizer", label: "summarizer - 정리자" },
];

const MAX_PERSONAS = 3;

export function InvitePanel({ personaCount, onInvite }: InvitePanelProps) {
  const [blood, setBlood] = useState("");
  const [mbti, setMbti] = useState("");
  const [zodiac, setZodiac] = useState("");
  const [role, setRole] = useState("");

  const isFull = personaCount >= MAX_PERSONAS;
  const canInvite = !isFull && blood !== "" && mbti !== "" && zodiac !== "";

  const preview = useMemo(
    () => indianName(mbti, blood, zodiac),
    [mbti, blood, zodiac]
  );

  const handleInvite = () => {
    if (!canInvite) return;
    onInvite(blood, mbti, zodiac, role || undefined);
    // 선택 초기화
    setBlood("");
    setMbti("");
    setZodiac("");
    setRole("");
  };

  return (
    <div
      className="rounded-xl p-3"
      style={{ background: "var(--bg-base)" }}
    >
      {/* 이름 미리보기 */}
      <div className="mb-3 min-h-[22px]">
        {preview ? (
          <p
            className="text-[12px] font-semibold text-center truncate"
            style={{ color: "var(--accent-warm)" }}
            title={preview}
          >
            {preview}
          </p>
        ) : (
          <p className="text-[11px] text-center text-[var(--text-secondary)]">
            혈액형 · MBTI · 별자리를 선택하면 이름이 생성됩니다
          </p>
        )}
      </div>

      {/* 드롭다운 그리드: 혈액형 / MBTI */}
      <div className="grid grid-cols-2 gap-1.5 mb-1.5">
        <Select value={blood} onValueChange={setBlood} disabled={isFull}>
          <SelectTrigger
            className="h-8 text-[11px] rounded-lg"
            style={{
              background: "var(--bg-surface)",
              border: "1px solid var(--border-color)",
              color: blood ? "var(--text-primary)" : "var(--text-secondary)",
            }}
          >
            <SelectValue placeholder="혈액형" />
          </SelectTrigger>
          <SelectContent>
            {BLOOD_OPTIONS.map((o) => (
              <SelectItem key={o.value} value={o.value} className="text-[12px]">
                {o.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        <Select value={mbti} onValueChange={setMbti} disabled={isFull}>
          <SelectTrigger
            className="h-8 text-[11px] rounded-lg"
            style={{
              background: "var(--bg-surface)",
              border: "1px solid var(--border-color)",
              color: mbti ? "var(--text-primary)" : "var(--text-secondary)",
            }}
          >
            <SelectValue placeholder="MBTI" />
          </SelectTrigger>
          <SelectContent>
            {MBTI_OPTIONS.map((m) => (
              <SelectItem key={m} value={m} className="text-[12px]">
                {m}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      {/* 별자리 */}
      <div className="mb-1.5">
        <Select value={zodiac} onValueChange={setZodiac} disabled={isFull}>
          <SelectTrigger
            className="h-8 text-[11px] w-full rounded-lg"
            style={{
              background: "var(--bg-surface)",
              border: "1px solid var(--border-color)",
              color: zodiac ? "var(--text-primary)" : "var(--text-secondary)",
            }}
          >
            <SelectValue placeholder="별자리" />
          </SelectTrigger>
          <SelectContent>
            {ZODIAC_OPTIONS.map((o) => (
              <SelectItem key={o.value} value={o.value} className="text-[12px]">
                {o.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      {/* role(선택) */}
      <div className="mb-3">
        <Select value={role} onValueChange={setRole} disabled={isFull}>
          <SelectTrigger
            className="h-8 text-[11px] w-full rounded-lg"
            style={{
              background: "var(--bg-surface)",
              border: "1px solid var(--border-color)",
              color: role ? "var(--text-primary)" : "var(--text-secondary)",
            }}
          >
            <SelectValue placeholder="역할 (선택)" />
          </SelectTrigger>
          <SelectContent>
            {ROLE_OPTIONS.map((o) => (
              <SelectItem key={o.value} value={o.value} className="text-[12px]">
                {o.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      {/* 추가 버튼 또는 full 안내 */}
      {isFull ? (
        <p
          className="text-[11px] text-center py-1.5"
          style={{ color: "var(--text-secondary)" }}
        >
          방이 가득 찼습니다 (최대 {MAX_PERSONAS}명)
        </p>
      ) : (
        <button
          onClick={handleInvite}
          disabled={!canInvite}
          className="w-full h-8 rounded-lg text-[12px] font-semibold flex items-center justify-center gap-1.5 transition-all disabled:opacity-30"
          style={{
            background: canInvite ? "var(--accent-warm)" : "var(--bg-elevated)",
            color: canInvite ? "#fff" : "var(--text-secondary)",
          }}
        >
          <UserPlus size={13} />
          초대
        </button>
      )}
    </div>
  );
}
