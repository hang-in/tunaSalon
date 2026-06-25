import { useState, useRef, useCallback } from "react";
import { Send, Hash, X } from "lucide-react";

interface ComposerProps {
  onSend: (text: string) => void;
  onSetTopics: (topics: string[]) => void;
  currentTopics: string[];
  disabled?: boolean;
}

export function Composer({ onSend, onSetTopics, currentTopics, disabled }: ComposerProps) {
  const [text, setText] = useState("");
  const [showTopicEditor, setShowTopicEditor] = useState(false);
  const [topicInput, setTopicInput] = useState("");
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const handleSend = useCallback(() => {
    if (!text.trim() || disabled) return;
    onSend(text.trim());
    setText("");
    if (textareaRef.current) {
      textareaRef.current.style.height = "auto";
    }
  }, [text, disabled, onSend]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      // 한글 IME 조합 중 Enter(조합 확정)는 제출로 처리하지 않는다(마지막 글자 중복/잘림 방지).
      if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
        e.preventDefault();
        handleSend();
      }
    },
    [handleSend]
  );

  const handleTextareaChange = useCallback(
    (e: React.ChangeEvent<HTMLTextAreaElement>) => {
      setText(e.target.value);
      // Auto-grow
      const el = e.target;
      el.style.height = "auto";
      el.style.height = `${Math.min(el.scrollHeight, 120)}px`;
    },
    []
  );

  const handleAddTopic = useCallback(() => {
    const trimmed = topicInput.trim();
    if (!trimmed) return;
    // 이미 있는 주제는 중복 추가하지 않는다(방어).
    if (currentTopics.includes(trimmed)) {
      setTopicInput("");
      return;
    }
    const newTopics = [trimmed, ...currentTopics].slice(0, 5);
    onSetTopics(newTopics);
    setTopicInput("");
  }, [topicInput, currentTopics, onSetTopics]);

  const handleRemoveTopic = useCallback(
    (topic: string) => {
      const newTopics = currentTopics.filter((t) => t !== topic);
      onSetTopics(newTopics);
    },
    [currentTopics, onSetTopics]
  );

  const handleTopicKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      // 한글 IME 조합 중 Enter는 무시(조합 확정 Enter + 제출 Enter 중복 -> 마지막 글자가 별도 토픽으로 들어가는 버그 방지).
      if (e.key === "Enter" && !e.nativeEvent.isComposing) {
        e.preventDefault();
        handleAddTopic();
      }
    },
    [handleAddTopic]
  );

  return (
    <div
      className="shrink-0 z-30"
      style={{
        background: "var(--bg-base)",
        borderTop: "1px solid var(--border-color)",
      }}
    >
      {/* Topic editor */}
      {showTopicEditor && (
        <div
          className="px-4 lg:px-6 py-3 flex items-center gap-2 overflow-x-auto"
          style={{ background: "var(--bg-surface)", borderBottom: "1px solid var(--border-color)" }}
        >
          <Hash size={14} className="text-[var(--accent-warm)] shrink-0" />
          <div className="flex items-center gap-1.5 shrink-0">
            {currentTopics.map((topic) => (
              <span
                key={topic}
                className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md text-[11px] font-medium"
                style={{ background: "var(--bg-elevated)", color: "var(--text-secondary)" }}
              >
                {topic}
                <button
                  onClick={() => handleRemoveTopic(topic)}
                  className="hover:text-[var(--text-primary)] transition-colors"
                  aria-label={`${topic} 제거`}
                >
                  <X size={10} />
                </button>
              </span>
            ))}
          </div>
          <input
            type="text"
            value={topicInput}
            onChange={(e) => setTopicInput(e.target.value)}
            onKeyDown={handleTopicKeyDown}
            placeholder="새 주제 입력..."
            className="flex-1 min-w-[120px] bg-transparent text-xs text-[var(--text-primary)] placeholder:text-[var(--text-secondary)] outline-none"
          />
          <button
            onClick={handleAddTopic}
            disabled={!topicInput.trim()}
            className="text-[11px] font-medium px-2 py-1 rounded-md shrink-0 transition-colors disabled:opacity-30"
            style={{ background: "var(--bg-elevated)", color: "var(--accent-warm)" }}
          >
            추가
          </button>
          <button
            onClick={() => setShowTopicEditor(false)}
            className="text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors shrink-0"
          >
            <X size={14} />
          </button>
        </div>
      )}

      {/* Input row */}
      <div className="px-4 lg:px-6 py-3 flex items-center gap-3">
        {/* Topic toggle button */}
        <button
          onClick={() => setShowTopicEditor(!showTopicEditor)}
          className="shrink-0 p-2.5 rounded-xl transition-colors"
          style={{
            background: showTopicEditor ? "rgba(229, 164, 74, 0.15)" : "var(--bg-surface)",
            color: showTopicEditor ? "var(--accent-warm)" : "var(--text-secondary)",
          }}
          aria-label="주제 설정"
          title="주제 설정"
        >
          <Hash size={16} />
        </button>

        {/* Textarea */}
        <div className="flex-1 relative">
          <textarea
            ref={textareaRef}
            value={text}
            onChange={handleTextareaChange}
            onKeyDown={handleKeyDown}
            placeholder="메시지를 입력하세요..."
            rows={1}
            disabled={disabled}
            className="w-full resize-none px-4 py-3 rounded-xl text-[15px] leading-relaxed outline-none transition-colors disabled:opacity-50"
            style={{
              background: "var(--bg-surface)",
              color: "var(--text-primary)",
              border: "1px solid var(--border-color)",
              minHeight: "44px",
              maxHeight: "120px",
            }}
            onFocus={(e) => {
              e.currentTarget.style.borderColor = "var(--accent-warm)";
            }}
            onBlur={(e) => {
              e.currentTarget.style.borderColor = "var(--border-color)";
            }}
          />
        </div>

        {/* Send button */}
        <button
          onClick={handleSend}
          disabled={!text.trim() || disabled}
          className="shrink-0 w-11 h-11 rounded-full flex items-center justify-center transition-all disabled:opacity-30 disabled:scale-100 hover:scale-105 active:scale-95"
          style={{
            background: text.trim() ? "var(--accent-warm)" : "var(--bg-elevated)",
            color: text.trim() ? "#fff" : "var(--text-secondary)",
          }}
          aria-label="보내기"
        >
          <Send size={18} />
        </button>
      </div>
    </div>
  );
}
