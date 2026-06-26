import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { AxisOption } from "@/lib/personaAxes";

// 참가자 축 셀렉터(혈액형/MBTI/별자리/역할 공용). 드롭다운에 명시적 배경을 줘
// 테마 미정의(--popover)로 인한 투명 문제를 피한다.
export function AxisSelect({
  label,
  value,
  onChange,
  options,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  options: AxisOption[];
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
            <SelectItem key={o.value} value={o.value} className="text-[12px]" title={o.hint}>
              {o.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  );
}
