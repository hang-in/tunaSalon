import { useCallback, useState } from "react";
import { useChat } from "@/hooks/useChat";
import { Header } from "@/components/Header";
import { ChatArea } from "@/components/ChatArea";
import { SidePanel } from "@/components/SidePanel";
import { Composer } from "@/components/Composer";
import { ThreeBackground } from "@/components/ThreeBackground";
import { PanelRightOpen } from "lucide-react";

function App() {
  const {
    messages,
    engineState,
    connected,
    sidebarOpen,
    setSidebarOpen,
    sendMessage,
    sendTopics,
    sendPause,
    getPersonaConfig,
    personaConfigs,
    humanPulse,
  } = useChat();

  const handleSend = useCallback(
    (text: string) => {
      sendMessage(text);
    },
    [sendMessage]
  );

  // Three.js 배경: 기본 off(GPU 절약). 헤더 토글로 켜며, 켜도 메시지가 적을 때만 보인다.
  const [bg3d, setBg3d] = useState(false);
  const showThreeBg = messages.length < 6;

  return (
    <div className="h-screen w-screen flex flex-col overflow-hidden" style={{ background: "var(--bg-base)" }}>
      {/* Three.js living room background (기본 off, 켜졌을 때만 마운트) */}
      {bg3d && <ThreeBackground intensities={engineState.intensities} visible={showThreeBg} />}

      {/* Header */}
      <Header
        topics={engineState.topics}
        connected={connected}
        participantCount={engineState.participants.length}
        onToggleSidebar={() => setSidebarOpen(true)}
        bg3d={bg3d}
        onToggle3d={() => setBg3d((v) => !v)}
        paused={engineState.paused}
        onTogglePause={() => sendPause(!engineState.paused)}
      />

      {/* P2-1: 연결 끊김 배너 */}
      {!connected && (
        <div
          className="fixed top-16 left-0 right-0 z-30 flex items-center justify-center gap-2 px-4 py-2 text-sm font-medium"
          style={{
            background: "rgba(217, 100, 90, 0.12)",
            borderBottom: "1px solid rgba(217, 100, 90, 0.25)",
            color: "#D9645A",
          }}
        >
          <span
            className="w-3.5 h-3.5 rounded-full border-2 border-current border-t-transparent animate-spin shrink-0"
            aria-hidden="true"
          />
          연결이 끊겼습니다. 재연결 중...
        </div>
      )}

      {/* Main content - 배너가 있을 때 top offset 추가 */}
      <div className={`flex flex-1 overflow-hidden mt-16 relative z-10 transition-all duration-300 ${!connected ? "pt-9" : ""}`}>
        {/* Chat column */}
        <main className="flex flex-col flex-1 min-w-0">
          <ChatArea
            messages={messages}
            engineState={engineState}
            getPersonaConfig={getPersonaConfig}
            connected={connected}
          />
          <Composer
            onSend={handleSend}
            onSetTopics={sendTopics}
            currentTopics={engineState.topics}
          />
        </main>

        {/* Sidebar */}
        <SidePanel
          engineState={engineState}
          personaConfigs={personaConfigs}
          open={sidebarOpen}
          onClose={() => setSidebarOpen(false)}
          humanPulse={humanPulse}
        />
      </div>

      {/* Mobile FAB to open sidebar */}
      <button
        onClick={() => setSidebarOpen(true)}
        className={`
          lg:hidden fixed bottom-20 right-4 z-30
          w-10 h-10 rounded-full flex items-center justify-center
          shadow-lg transition-all
          ${sidebarOpen ? "opacity-0 pointer-events-none" : "opacity-100"}
        `}
        style={{
          background: "var(--bg-surface)",
          color: "var(--accent-warm)",
          border: "1px solid var(--border-color)",
        }}
        aria-label="상태 패널 열기"
      >
        <PanelRightOpen size={18} />
      </button>
    </div>
  );
}

export default App;
