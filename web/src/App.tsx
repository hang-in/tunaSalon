import { useCallback } from "react";
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
    getPersonaConfig,
    personaConfigs,
  } = useChat();

  const handleSend = useCallback(
    (text: string) => {
      sendMessage(text);
    },
    [sendMessage]
  );

  // Three.js background visible when few messages
  const showThreeBg = messages.length < 6;

  return (
    <div className="h-screen w-screen flex flex-col overflow-hidden" style={{ background: "var(--bg-base)" }}>
      {/* Three.js living room background */}
      <ThreeBackground intensities={engineState.intensities} visible={showThreeBg} />

      {/* Header */}
      <Header
        topics={engineState.topics}
        connected={connected}
        participantCount={engineState.participants.length}
        onToggleSidebar={() => setSidebarOpen(true)}
      />

      {/* Main content */}
      <div className="flex flex-1 overflow-hidden mt-16 relative z-10">
        {/* Chat column */}
        <main className="flex flex-col flex-1 min-w-0">
          <ChatArea
            messages={messages}
            engineState={engineState}
            getPersonaConfig={getPersonaConfig}
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
