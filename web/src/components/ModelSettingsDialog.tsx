import { useState, useEffect } from "react";
import { Cpu, Check } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { MODEL_OPTIONS, MODELS_REQUIRED, getSelectedModels, setSelectedModels } from "@/lib/models";

interface ModelSettingsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** 저장 후 선택 모델을 부모에 알린다(다음 새 방부터 적용). */
  onSaved?: (models: string[]) => void;
}

// 페르소나가 쓸 LLM 모델 3개를 고르는 설정 모달. 새 방부터 적용(기존 방은 만들 때 모델 고정).
export function ModelSettingsDialog({ open, onOpenChange, onSaved }: ModelSettingsDialogProps) {
  const [selected, setSelected] = useState<string[]>([]);

  useEffect(() => {
    if (open) setSelected(getSelectedModels());
  }, [open]);

  const toggle = (value: string) => {
    setSelected((prev) => {
      if (prev.includes(value)) return prev.filter((m) => m !== value);
      if (prev.length >= MODELS_REQUIRED) return prev; // 3개 초과 금지
      return [...prev, value];
    });
  };

  const canSave = selected.length === MODELS_REQUIRED;

  const handleSave = () => {
    if (!canSave) return;
    setSelectedModels(selected);
    onSaved?.(selected);
    onOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="sm:max-w-lg"
        style={{ background: "var(--bg-surface)", borderColor: "var(--border-color)" }}
      >
        <DialogHeader>
          <DialogTitle style={{ color: "var(--text-primary)" }}>
            <span className="inline-flex items-center gap-2">
              <Cpu size={16} style={{ color: "var(--accent-warm)" }} />
              토론 모델 고르기 ({selected.length}/{MODELS_REQUIRED})
            </span>
          </DialogTitle>
        </DialogHeader>

        <p className="text-[12px] -mt-1 mb-1" style={{ color: "var(--text-secondary)" }}>
          3개를 고르면 새 토론방의 참가자 3명에게 각각 배정됩니다. (기존 방은 만들 때 모델이 고정됩니다)
        </p>

        <div className="flex flex-col gap-1.5 max-h-[55vh] overflow-y-auto">
          {MODEL_OPTIONS.map((m) => {
            const on = selected.includes(m.value);
            const full = !on && selected.length >= MODELS_REQUIRED;
            return (
              <button
                key={m.value}
                type="button"
                onClick={() => toggle(m.value)}
                disabled={full}
                className="flex items-center gap-3 rounded-xl px-3 py-2.5 text-left transition-colors disabled:opacity-40"
                style={{
                  background: on ? "var(--bg-elevated)" : "var(--bg-base)",
                  border: `1px solid ${on ? "var(--accent-warm)" : "var(--border-color)"}`,
                }}
              >
                <span
                  className="w-4 h-4 rounded flex items-center justify-center shrink-0"
                  style={{
                    background: on ? "var(--accent-warm)" : "transparent",
                    border: `1.5px solid ${on ? "var(--accent-warm)" : "var(--text-secondary)"}`,
                  }}
                >
                  {on && <Check size={11} color="#fff" />}
                </span>
                <span className="flex-1 min-w-0">
                  <span className="block text-[13px] font-semibold" style={{ color: "var(--text-primary)" }}>
                    {m.label}
                  </span>
                  <span className="block text-[11px]" style={{ color: "var(--text-secondary)" }}>
                    {m.value}
                    {m.note ? ` · ${m.note}` : ""}
                  </span>
                </span>
              </button>
            );
          })}
        </div>

        <div className="grid grid-cols-2 gap-2 mt-1">
          <button
            onClick={() => onOpenChange(false)}
            className="h-10 rounded-xl text-[13px] font-semibold"
            style={{ background: "var(--bg-elevated)", color: "var(--text-secondary)" }}
          >
            취소
          </button>
          <button
            onClick={handleSave}
            disabled={!canSave}
            className="h-10 rounded-xl text-[13px] font-semibold transition-all disabled:opacity-30"
            style={{
              background: canSave ? "var(--accent-warm)" : "var(--bg-elevated)",
              color: canSave ? "#fff" : "var(--text-secondary)",
            }}
          >
            저장
          </button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
