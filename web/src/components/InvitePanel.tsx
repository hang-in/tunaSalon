import { useState, useMemo } from "react";
import { UserPlus } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
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

interface Option {
  value: string;
  label: string;
}

const BLOOD_OPTIONS: Option[] = [
  { value: "A", label: "A형" },
  { value: "B", label: "B형" },
  { value: "O", label: "O형" },
  { value: "AB", label: "AB형" },
];

const MBTI_OPTIONS: Option[] = [
  "ENTP", "ENTJ", "ENFP", "ENFJ",
  "ESTP", "ESTJ", "ESFP", "ESFJ",
  "INTP", "INTJ", "INFP", "INFJ",
  "ISTP", "ISTJ", "ISFP", "ISFJ",
].map((m) => ({ value: m, label: m }));

const ZODIAC_OPTIONS: Option[] = [
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

const ROLE_OPTIONS: Option[] = [
  { value: "friend", label: "친구" },
  { value: "chaos", label: "와일드카드" },
  { value: "critic", label: "비평가" },
  { value: "realist", label: "현실주의자" },
  { value: "teacher", label: "교사" },
  { value: "poet", label: "시인" },
  { value: "strategist", label: "전략가" },
  { value: "summarizer", label: "정리자" },
];

const MAX_PERSONAS = 3;

function AxisSelect({
  label,
  value,
  onChange,
  options,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  options: Option[];
}) {
  return (
    <div className="flex flex-col gap-1">
      <span className="text-[11px] font-medium" style={{ color: "var(--text-secondary)" }}>
        {label}
      </span>
      <Select value={value} onValueChange={onChange}>
        <SelectTrigger
          className="h-9 text-[12px] rounded-lg w-full"
          style={{
            background: "var(--bg-base)",
            border: "1px solid var(--border-color)",
            color: value ? "var(--text-primary)" : "var(--text-secondary)",
          }}
        >
          <SelectValue placeholder={label} />
        </SelectTrigger>
        <SelectContent
          style={{
            background: "var(--bg-elevated)",
            borderColor: "var(--border-color)",
            color: "var(--text-primary)",
          }}
        >
          {options.map((o) => (
            <SelectItem key={o.value} value={o.value} className="text-[12px]">
              {o.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  );
}

export function InvitePanel({ personaCount, onInvite }: InvitePanelProps) {
  const [open, setOpen] = useState(false);
  const [blood, setBlood] = useState("");
  const [mbti, setMbti] = useState("");
  const [zodiac, setZodiac] = useState("");
  const [role, setRole] = useState("");

  const isFull = personaCount >= MAX_PERSONAS;
  // 모든 축(혈액형·MBTI·별자리·역할)을 다 골라야 초대할 수 있다.
  const allChosen = blood !== "" && mbti !== "" && zodiac !== "" && role !== "";
  const preview = useMemo(() => indianName(mbti, blood, zodiac), [mbti, blood, zodiac]);

  const reset = () => {
    setBlood("");
    setMbti("");
    setZodiac("");
    setRole("");
  };

  const handleInvite = () => {
    if (!allChosen) return;
    onInvite(blood, mbti, zodiac, role);
    reset();
    setOpen(false);
  };

  const handleCancel = () => {
    reset();
    setOpen(false);
  };

  // 방이 가득 찬 경우: 버튼 대신 안내 메시지만.
  if (isFull) {
    return (
      <p
        className="text-[12px] text-center py-2.5 rounded-xl"
        style={{ color: "var(--text-secondary)", background: "var(--bg-base)" }}
      >
        방이 가득 찼습니다 (최대 {MAX_PERSONAS}명)
      </p>
    );
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        setOpen(o);
        if (!o) reset();
      }}
    >
      <DialogTrigger asChild>
        <button
          className="w-full h-10 rounded-xl text-[13px] font-semibold flex items-center justify-center gap-1.5 transition-opacity hover:opacity-90"
          style={{ background: "var(--accent-warm)", color: "#fff" }}
        >
          <UserPlus size={15} />
          참가자 초대
        </button>
      </DialogTrigger>

      <DialogContent
        className="sm:max-w-2xl"
        style={{ background: "var(--bg-surface)", borderColor: "var(--border-color)" }}
      >
        <DialogHeader>
          <DialogTitle style={{ color: "var(--text-primary)" }}>참가자 초대</DialogTitle>
        </DialogHeader>

        {/* 1열: 축 가로 배열 (모바일은 2x2). 모두 골라야 초대 가능. */}
        <div className="grid grid-cols-2 sm:grid-cols-4 gap-2">
          <AxisSelect label="혈액형" value={blood} onChange={setBlood} options={BLOOD_OPTIONS} />
          <AxisSelect label="MBTI" value={mbti} onChange={setMbti} options={MBTI_OPTIONS} />
          <AxisSelect label="별자리" value={zodiac} onChange={setZodiac} options={ZODIAC_OPTIONS} />
          <AxisSelect label="역할" value={role} onChange={setRole} options={ROLE_OPTIONS} />
        </div>

        {/* 2열: 닉네임 미리보기 */}
        <div
          className="min-h-[48px] flex flex-col items-center justify-center rounded-xl px-3 py-2"
          style={{ background: "var(--bg-base)", border: "1px solid var(--border-color)" }}
        >
          {preview ? (
            <>
              <p className="text-[16px] font-bold" style={{ color: "var(--accent-warm)" }}>
                {preview}
              </p>
              {!role && (
                <p className="text-[11px] mt-0.5" style={{ color: "var(--text-secondary)" }}>
                  역할까지 고르면 초대할 수 있어요
                </p>
              )}
            </>
          ) : (
            <p className="text-[12px]" style={{ color: "var(--text-secondary)" }}>
              혈액형 · MBTI · 별자리를 고르면 닉네임이 만들어집니다
            </p>
          )}
        </div>

        {/* 3열: 취소 / 초대 */}
        <div className="grid grid-cols-2 gap-2">
          <button
            onClick={handleCancel}
            className="h-10 rounded-xl text-[13px] font-semibold transition-colors"
            style={{ background: "var(--bg-elevated)", color: "var(--text-secondary)" }}
          >
            취소
          </button>
          <button
            onClick={handleInvite}
            disabled={!allChosen}
            className="h-10 rounded-xl text-[13px] font-semibold flex items-center justify-center gap-1.5 transition-all disabled:opacity-30"
            style={{
              background: allChosen ? "var(--accent-warm)" : "var(--bg-elevated)",
              color: allChosen ? "#fff" : "var(--text-secondary)",
            }}
          >
            <UserPlus size={14} />
            초대
          </button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
