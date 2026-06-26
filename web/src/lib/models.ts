// 설정 페이지에서 고를 수 있는 클라우드 모델 목록(백엔드 model.rs CLOUD_MODELS와 동일 태그).
// 사용자가 3개를 고르면 새 방의 페르소나 3명에 1:1로 배정된다.

export interface ModelOption {
  value: string; // ollama 모델 태그(백엔드와 일치해야 함)
  label: string; // 표시명
  note?: string; // 짧은 특징
}

export const MODEL_OPTIONS: ModelOption[] = [
  { value: "gemma4:31b-cloud", label: "Gemma4 31B", note: "한국어 강점·기본" },
  { value: "nemotron-3-super:cloud", label: "Nemotron-3 Super", note: "NVIDIA 추론" },
  { value: "qwen3.5:cloud", label: "Qwen3.5", note: "다국어·thinking" },
  { value: "glm-5.1:cloud", label: "GLM-5.1", note: "범용" },
  { value: "kimi-k2.6:cloud", label: "Kimi K2.6", note: "장문·추론" },
  { value: "deepseek-v4-flash:cloud", label: "DeepSeek V4 Flash", note: "빠른 추론" },
  { value: "devstral-small-2:24b-cloud", label: "Devstral Small 2 24B", note: "경량" },
];

export const MODELS_REQUIRED = 3;
const STORAGE_KEY = "salon.models";
export const DEFAULT_MODELS = ["gemma4:31b-cloud", "qwen3.5:cloud", "glm-5.1:cloud"];

/** 저장된 선택 모델(정확히 3개). 없거나 망가졌으면 기본값. */
export function getSelectedModels(): string[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULT_MODELS;
    const parsed = JSON.parse(raw);
    const valid = Array.isArray(parsed)
      ? parsed.filter((m) => MODEL_OPTIONS.some((o) => o.value === m))
      : [];
    return valid.length === MODELS_REQUIRED ? valid : DEFAULT_MODELS;
  } catch {
    return DEFAULT_MODELS;
  }
}

export function setSelectedModels(models: string[]): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(models));
  } catch {
    // localStorage 불가 환경은 무시(기본값 사용)
  }
}
