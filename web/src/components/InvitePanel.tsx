import { useState, useMemo } from "react";
import { UserPlus } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { indianName } from "@/lib/indianName";
import { AxisSelect } from "@/components/AxisSelect";
import {
  BLOOD_OPTIONS,
  MBTI_OPTIONS,
  ZODIAC_OPTIONS,
  ROLE_OPTIONS,
  MAX_PERSONAS,
} from "@/lib/personaAxes";

interface InvitePanelProps {
  /** human 제외 현재 persona 수 */
  personaCount: number;
  onInvite: (blood: string, mbti: string, zodiac: string, role?: string) => void;
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
