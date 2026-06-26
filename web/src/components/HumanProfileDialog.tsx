import { useState, useEffect, useMemo } from "react";
import { UserCog } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { indianName } from "@/lib/indianName";
import { AxisSelect } from "@/components/AxisSelect";
import { PersonaAvatar } from "@/lib/personaAvatar";
import {
  BLOOD_OPTIONS,
  MBTI_OPTIONS,
  ZODIAC_OPTIONS,
  ROLE_OPTIONS,
} from "@/lib/personaAxes";

interface HumanProfileDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** 현재 저장된 내 4축(있으면 폼 초기값). */
  initial?: { blood: string; mbti: string; zodiac: string; role: string };
  onSave: (blood: string, mbti: string, zodiac: string, role: string) => void;
}

// "나" 캐릭터(4축)를 직접 구성하는 모달. 클릭 트리거는 외부(나 카드)에서 제어.
export function HumanProfileDialog({ open, onOpenChange, initial, onSave }: HumanProfileDialogProps) {
  const [blood, setBlood] = useState("");
  const [mbti, setMbti] = useState("");
  const [zodiac, setZodiac] = useState("");
  const [role, setRole] = useState("");

  // 열릴 때 현재 저장값으로 폼을 채운다.
  useEffect(() => {
    if (open) {
      setBlood(initial?.blood ?? "");
      setMbti(initial?.mbti ?? "");
      setZodiac(initial?.zodiac ?? "");
      setRole(initial?.role ?? "");
    }
  }, [open, initial]);

  const allChosen = blood !== "" && mbti !== "" && zodiac !== "" && role !== "";
  const preview = useMemo(() => indianName(mbti, blood, zodiac), [mbti, blood, zodiac]);

  const handleSave = () => {
    if (!allChosen) return;
    onSave(blood, mbti, zodiac, role);
    onOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="sm:max-w-2xl"
        style={{ background: "var(--bg-surface)", borderColor: "var(--border-color)" }}
      >
        <DialogHeader>
          <DialogTitle style={{ color: "var(--text-primary)" }}>내 캐릭터 만들기</DialogTitle>
        </DialogHeader>

        {/* 축 선택 (모바일 2x2) */}
        <div className="grid grid-cols-2 sm:grid-cols-4 gap-2">
          <AxisSelect label="혈액형" value={blood} onChange={setBlood} options={BLOOD_OPTIONS} />
          <AxisSelect label="MBTI" value={mbti} onChange={setMbti} options={MBTI_OPTIONS} />
          <AxisSelect label="별자리" value={zodiac} onChange={setZodiac} options={ZODIAC_OPTIONS} />
          <AxisSelect label="역할" value={role} onChange={setRole} options={ROLE_OPTIONS} />
        </div>

        {/* 미리보기: 캐릭터 + 닉네임 */}
        <div
          className="min-h-[72px] flex items-center justify-center gap-3 rounded-xl px-3 py-2"
          style={{ background: "var(--bg-base)", border: "1px solid var(--border-color)" }}
        >
          {allChosen ? (
            <>
              <div
                className="w-12 h-12 rounded-full flex items-center justify-center overflow-hidden shrink-0"
                style={{ background: "var(--bg-elevated)" }}
              >
                <PersonaAvatar axes={{ blood, mbti, zodiac, role }} color="#E5A44A" pose="calm" size={44} />
              </div>
              <p className="text-[16px] font-bold" style={{ color: "var(--accent-warm)" }}>
                {preview}
              </p>
            </>
          ) : (
            <p className="text-[12px]" style={{ color: "var(--text-secondary)" }}>
              네 축(혈액형 · MBTI · 별자리 · 역할)을 모두 고르면 캐릭터가 만들어집니다
            </p>
          )}
        </div>

        <div className="grid grid-cols-2 gap-2">
          <button
            onClick={() => onOpenChange(false)}
            className="h-10 rounded-xl text-[13px] font-semibold transition-colors"
            style={{ background: "var(--bg-elevated)", color: "var(--text-secondary)" }}
          >
            취소
          </button>
          <button
            onClick={handleSave}
            disabled={!allChosen}
            className="h-10 rounded-xl text-[13px] font-semibold flex items-center justify-center gap-1.5 transition-all disabled:opacity-30"
            style={{
              background: allChosen ? "var(--accent-warm)" : "var(--bg-elevated)",
              color: allChosen ? "#fff" : "var(--text-secondary)",
            }}
          >
            <UserCog size={14} />
            저장
          </button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
