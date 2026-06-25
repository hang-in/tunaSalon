import { useState, useMemo } from "react";
import { UserPlus, X, Play } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
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

interface BuiltPersona {
  blood: string;
  mbti: string;
  zodiac: string;
  role: string;
  name: string;
}

interface CreateRoomDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** 표시용 주제(입력창 값). 빈 문자열이면 placeholder가 쓰인다. */
  topic: string;
  /** "토론 시작" 시 호출. personas = ["blood:mbti:zodiac:role", ...] (2~3명). */
  onStart: (personas: string[]) => void;
}

const MIN_PERSONAS = 2;

export function CreateRoomDialog({ open, onOpenChange, topic, onStart }: CreateRoomDialogProps) {
  const [blood, setBlood] = useState("");
  const [mbti, setMbti] = useState("");
  const [zodiac, setZodiac] = useState("");
  const [role, setRole] = useState("");
  const [list, setList] = useState<BuiltPersona[]>([]);

  const preview = useMemo(() => indianName(mbti, blood, zodiac), [mbti, blood, zodiac]);
  const axesChosen = blood !== "" && mbti !== "" && zodiac !== "" && role !== "";
  const isFull = list.length >= MAX_PERSONAS;
  const canAdd = axesChosen && !isFull && !!preview && !list.some((p) => p.name === preview);
  const canStart = list.length >= MIN_PERSONAS && list.length <= MAX_PERSONAS;

  const resetAxes = () => {
    setBlood("");
    setMbti("");
    setZodiac("");
    setRole("");
  };

  const resetAll = () => {
    resetAxes();
    setList([]);
  };

  const handleAdd = () => {
    if (!canAdd) return;
    setList((prev) => [...prev, { blood, mbti, zodiac, role, name: preview }]);
    resetAxes();
  };

  const handleRemove = (name: string) => {
    setList((prev) => prev.filter((p) => p.name !== name));
  };

  const handleStart = () => {
    if (!canStart) return;
    onStart(list.map((p) => `${p.blood}:${p.mbti}:${p.zodiac}:${p.role}`));
    resetAll();
    onOpenChange(false);
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        onOpenChange(o);
        if (!o) resetAll();
      }}
    >
      <DialogContent
        className="sm:max-w-2xl"
        style={{ background: "var(--bg-surface)", borderColor: "var(--border-color)" }}
      >
        <DialogHeader>
          <DialogTitle style={{ color: "var(--text-primary)" }}>참가자 직접 구성</DialogTitle>
        </DialogHeader>

        {/* 주제 컨텍스트 */}
        <p className="text-[12px] -mt-1" style={{ color: "var(--text-secondary)" }}>
          주제: <span style={{ color: "var(--accent-warm)" }}>{topic || "(추천 주제로 시작)"}</span>
          {" · "}참가자 {MIN_PERSONAS}~{MAX_PERSONAS}명을 만들어 시작하세요.
        </p>

        {/* 축 선택 + 닉네임 + 추가 */}
        <div className="grid grid-cols-2 sm:grid-cols-4 gap-2">
          <AxisSelect label="혈액형" value={blood} onChange={setBlood} options={BLOOD_OPTIONS} />
          <AxisSelect label="MBTI" value={mbti} onChange={setMbti} options={MBTI_OPTIONS} />
          <AxisSelect label="별자리" value={zodiac} onChange={setZodiac} options={ZODIAC_OPTIONS} />
          <AxisSelect label="역할" value={role} onChange={setRole} options={ROLE_OPTIONS} />
        </div>

        <div className="flex items-center gap-2">
          <div
            className="flex-1 min-h-[40px] flex items-center justify-center rounded-xl px-3"
            style={{ background: "var(--bg-base)", border: "1px solid var(--border-color)" }}
          >
            {preview ? (
              <span className="text-[15px] font-bold" style={{ color: "var(--accent-warm)" }}>
                {preview}
              </span>
            ) : (
              <span className="text-[12px]" style={{ color: "var(--text-secondary)" }}>
                네 가지를 고르면 닉네임이 만들어집니다
              </span>
            )}
          </div>
          <button
            onClick={handleAdd}
            disabled={!canAdd}
            className="h-10 px-4 rounded-xl text-[13px] font-semibold flex items-center justify-center gap-1.5 disabled:opacity-30"
            style={{ background: canAdd ? "var(--accent-warm)" : "var(--bg-elevated)", color: canAdd ? "#fff" : "var(--text-secondary)" }}
          >
            <UserPlus size={14} />
            추가
          </button>
        </div>

        {/* 구성된 참가자 목록 */}
        <div className="flex flex-col gap-1.5">
          {list.length === 0 ? (
            <p className="text-[12px] text-center py-2" style={{ color: "var(--text-secondary)" }}>
              아직 추가한 참가자가 없습니다 (최소 {MIN_PERSONAS}명)
            </p>
          ) : (
            list.map((p) => (
              <div
                key={p.name}
                className="flex items-center justify-between rounded-lg px-3 py-2"
                style={{ background: "var(--bg-base)", border: "1px solid var(--border-color)" }}
              >
                <span className="text-[13px] font-semibold" style={{ color: "var(--text-primary)" }}>
                  {p.name}
                </span>
                <button
                  onClick={() => handleRemove(p.name)}
                  className="p-1 rounded-md hover:bg-white/5"
                  aria-label={`${p.name} 제거`}
                >
                  <X size={14} style={{ color: "var(--text-secondary)" }} />
                </button>
              </div>
            ))
          )}
        </div>

        {/* 취소 / 토론 시작 */}
        <div className="grid grid-cols-2 gap-2">
          <button
            onClick={() => {
              resetAll();
              onOpenChange(false);
            }}
            className="h-10 rounded-xl text-[13px] font-semibold"
            style={{ background: "var(--bg-elevated)", color: "var(--text-secondary)" }}
          >
            취소
          </button>
          <button
            onClick={handleStart}
            disabled={!canStart}
            className="h-10 rounded-xl text-[13px] font-semibold flex items-center justify-center gap-1.5 disabled:opacity-30"
            style={{
              background: canStart ? "var(--accent-warm)" : "var(--bg-elevated)",
              color: canStart ? "#fff" : "var(--text-secondary)",
            }}
          >
            <Play size={14} />
            토론 시작 ({list.length}/{MAX_PERSONAS})
          </button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
