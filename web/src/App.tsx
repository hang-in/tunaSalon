import { useCallback, useEffect, useMemo, useState } from "react";
import { useChat } from "@/hooks/useChat";
import { Header } from "@/components/Header";
import { ChatArea } from "@/components/ChatArea";
import { SidePanel } from "@/components/SidePanel";
import { Composer } from "@/components/Composer";
import { ThreeBackground } from "@/components/ThreeBackground";
import { CreateRoomDialog } from "@/components/CreateRoomDialog";
import { MessageSquareText, PanelRightOpen, Plus, Trash2, Users } from "lucide-react";

interface DebateRoom {
  id: string;
  title: string;
  topics: string[];
  summary?: string;
  /** 새 방 수동 구성 참가자 ["blood:mbti:zodiac:role", ...]. 비면 서버가 랜덤 3명 시딩. */
  personas?: string[];
}

/** 서버 /api/suggested-topics 응답: 분야별 추천 토론 주제. */
interface SuggestedGroup {
  category: string;
  topics: string[];
}

const RECENT_ROOMS_KEY = "tunaSalon.recentRooms.v1";

const TOPIC_SUGGESTIONS = [
  "AI 친구는 진짜 친구가 될 수 있을까?",
  "죽은 사람의 말투를 복원한 AI는 위로일까, 모독일까?",
  "기본소득은 인간을 게으르게 만들까, 자유롭게 만들까?",
  "기억을 선택적으로 지울 수 있다면 지워도 될까?",
  "AI 판사가 인간 판사보다 공정할 수 있을까?",
  "연애 앱은 사랑을 돕는가, 소비하게 만드는가?",
  "아이에게 스마트폰을 주는 나이는 법으로 정해야 할까?",
  "인터넷 익명성은 보호해야 할 권리인가, 폐지해야 할 위험인가?",
  "완전 자동화 사회에서 일하지 않는 사람도 존중받을 수 있을까?",
  "가족보다 선택한 공동체가 더 중요해질 수 있을까?",
];

function randomTopicSuggestion(): string {
  return TOPIC_SUGGESTIONS[Math.floor(Math.random() * TOPIC_SUGGESTIONS.length)];
}

function hashString(value: string): string {
  let hash = 5381;
  for (const ch of value) hash = (hash * 33) ^ ch.charCodeAt(0);
  return (hash >>> 0).toString(36);
}

function roomIdFromTopics(topics: string[]): string {
  const title = topics[0] || "topic";
  const ascii = title
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 36);
  return `debate-${ascii || "topic"}-${hashString(topics.join("|"))}`;
}

function readRecentRooms(): DebateRoom[] {
  try {
    const raw = localStorage.getItem(RECENT_ROOMS_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as DebateRoom[];
    return Array.isArray(parsed)
      ? dedupeRooms(parsed.filter((room) => room?.id && room?.title && Array.isArray(room.topics))).slice(0, 6)
      : [];
  } catch {
    return [];
  }
}

function saveRecentRooms(rooms: DebateRoom[]) {
  localStorage.setItem(RECENT_ROOMS_KEY, JSON.stringify(dedupeRooms(rooms).slice(0, 6)));
}

function roomSummary(room: DebateRoom): string {
  if (room.summary?.trim()) return room.summary;
  const topic = room.topics[0] || room.title;
  return `"${topic}"에 대해 찬반과 조건을 나눠 보는 사용자 생성 토론방`;
}

function roomKey(room: DebateRoom): string {
  const topic = room.topics[0] || room.title;
  return `${room.title.trim().toLowerCase()}::${topic.trim().toLowerCase()}`;
}

function dedupeRooms(rooms: DebateRoom[]): DebateRoom[] {
  const seenIds = new Set<string>();
  const seenKeys = new Set<string>();
  const result: DebateRoom[] = [];
  for (const room of rooms) {
    const key = roomKey(room);
    if (seenIds.has(room.id) || seenKeys.has(key)) continue;
    seenIds.add(room.id);
    seenKeys.add(key);
    result.push(room);
  }
  return result;
}

function App() {
  const [inRoom, setInRoom] = useState(false);
  const [topicPlaceholder] = useState(randomTopicSuggestion);
  const [topicDraft, setTopicDraft] = useState("");
  const [activeRoom, setActiveRoom] = useState<DebateRoom | null>(null);
  const [recentRooms, setRecentRooms] = useState<DebateRoom[]>(readRecentRooms);
  const lobbyRooms = useMemo(() => dedupeRooms(recentRooms), [recentRooms]);
  const {
    messages,
    engineState,
    connected,
    sidebarOpen,
    setSidebarOpen,
    sendMessage,
    sendTopics,
    sendPause,
    sendPace,
    sendReset,
    sendInvite,
    sendRemove,
    getPersonaConfig,
    personaConfigs,
    humanPulse,
    resetChat,
  } = useChat({
    enabled: inRoom && !!activeRoom,
    roomId: activeRoom?.id,
    topics: activeRoom?.topics,
    personas: activeRoom?.personas,
  });
  const [builderOpen, setBuilderOpen] = useState(false);
  // 서버가 12h마다 웹서치+gemma로 생성하는 분야별 추천 주제. 비면 정적 TOPIC_SUGGESTIONS 폴백.
  const [suggestedGroups, setSuggestedGroups] = useState<SuggestedGroup[]>([]);
  useEffect(() => {
    let alive = true;
    fetch("/api/suggested-topics")
      .then((r) => (r.ok ? r.json() : []))
      .then((data: SuggestedGroup[]) => {
        if (alive && Array.isArray(data)) setSuggestedGroups(data.filter((g) => g?.topics?.length));
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, []);

  const rememberRoom = useCallback((room: DebateRoom) => {
    setRecentRooms((prev) => {
      const key = roomKey(room);
      const next = [
        room,
        ...prev.filter((item) => item.id !== room.id && roomKey(item) !== key),
      ].slice(0, 6);
      saveRecentRooms(next);
      return next;
    });
  }, []);

  const openRoom = useCallback((room: DebateRoom) => {
    resetChat();
    setActiveRoom(room);
    rememberRoom(room);
    setInRoom(true);
  }, [rememberRoom, resetChat]);

  const resolveTopics = useCallback(() => {
    // 토론 주제는 콤마를 포함할 수 있으므로 쪼개지 않고 입력 전체를 한 주제로 둔다.
    const source = topicDraft.trim() || topicPlaceholder.trim();
    return source ? [source] : [];
  }, [topicDraft, topicPlaceholder]);

  // "만들기": 랜덤 3명으로 시작(personas 미지정 → 서버가 room_id 기반 랜덤 3명 시딩).
  const handleCreateRoom = useCallback(() => {
    const topics = resolveTopics();
    if (topics.length === 0) return;
    openRoom({
      id: roomIdFromTopics(topics),
      title: topics[0],
      topics,
      summary: `"${topics[0]}"에 대해 랜덤 참가자들이 찬반과 조건을 나눠 보는 새 토론방`,
    });
  }, [openRoom, resolveTopics]);

  // "직접 고르기": 빌더에서 구성한 2~3명으로 시작. 같은 주제라도 구성이 다르면 다른 방.
  const handleCreateRoomWithPersonas = useCallback(
    (personas: string[]) => {
      const topics = resolveTopics();
      if (topics.length === 0 || personas.length === 0) return;
      openRoom({
        id: `${roomIdFromTopics(topics)}-${hashString(personas.join("|"))}`,
        title: topics[0],
        topics,
        personas,
        summary: `"${topics[0]}"에 대해 직접 구성한 참가자들이 토론하는 새 토론방`,
      });
    },
    [openRoom, resolveTopics]
  );

  const handleLeaveRoom = useCallback(() => {
    setInRoom(false);
  }, []);

  const deleteRoom = useCallback(async (room: DebateRoom) => {
    if (!window.confirm(`'${room.title}' 토론방을 삭제할까요? 저장된 대화와 장기기억도 지워집니다.`)) {
      return;
    }
    try {
      const response = await fetch(`/api/rooms/${encodeURIComponent(room.id)}`, { method: "DELETE" });
      if (!response.ok) throw new Error(`DELETE failed: ${response.status}`);
    } catch (error) {
      console.error(error);
      window.alert("방 삭제 요청이 실패했습니다. 서버 연결을 확인해주세요.");
      return;
    }
    setRecentRooms((prev) => {
      const next = prev.filter((item) => item.id !== room.id);
      saveRecentRooms(next);
      return next;
    });
    if (activeRoom?.id === room.id) {
      resetChat();
      setInRoom(false);
      setActiveRoom(null);
    }
  }, [activeRoom?.id, resetChat]);

  const handleDeleteActiveRoom = useCallback(() => {
    if (activeRoom) void deleteRoom(activeRoom);
  }, [activeRoom, deleteRoom]);

  const handleResetDebate = useCallback(() => {
    const topics = engineState.topics.length > 0 ? engineState.topics : activeRoom?.topics ?? [];
    sendReset(topics);
    if (activeRoom) {
      const nextRoom = { ...activeRoom, topics, title: topics[0] ?? activeRoom.title };
      setActiveRoom(nextRoom);
      rememberRoom(nextRoom);
    }
  }, [activeRoom, engineState.topics, rememberRoom, sendReset]);

  const handleSetTopics = useCallback((topics: string[]) => {
    sendTopics(topics);
    if (activeRoom) {
      const nextRoom = { ...activeRoom, topics, title: topics[0] ?? activeRoom.title };
      setActiveRoom(nextRoom);
      rememberRoom(nextRoom);
    }
  }, [activeRoom, rememberRoom, sendTopics]);

  const handleSend = useCallback(
    (text: string) => {
      sendMessage(text);
    },
    [sendMessage]
  );

  // Three.js 배경: 기본 off(GPU 절약). 헤더 토글로 켜며, 켜도 메시지가 적을 때만 보인다.
  const [bg3d, setBg3d] = useState(false);
  const showThreeBg = messages.length < 6;

  if (!inRoom) {
    return (
      <div className="h-screen w-screen overflow-hidden" style={{ background: "var(--bg-base)" }}>
        <main className="h-full overflow-y-auto px-4 py-8 flex items-center justify-center">
          <section
            className="w-full max-w-4xl"
            style={{ color: "var(--text-primary)" }}
          >
            <div className="flex items-center gap-3 mb-8">
              <div
                className="w-10 h-10 rounded-lg flex items-center justify-center"
                style={{ background: "rgba(229, 164, 74, 0.15)", color: "var(--accent-warm)" }}
              >
                <MessageSquareText size={20} />
              </div>
              <div>
                <h1 className="text-xl font-extrabold tracking-tight">tunaSalon</h1>
                <p className="text-sm text-[var(--text-secondary)]">주제 토론방 로비</p>
              </div>
            </div>

            {lobbyRooms.length > 0 ? (
              <div className="grid grid-cols-1 md:grid-cols-3 gap-3 mb-8">
                {lobbyRooms.map((room) => (
                  <div
                    key={room.id}
                    className="rounded-lg p-4"
                    style={{
                      background: "var(--bg-surface)",
                      border: "1px solid var(--border-color)",
                      color: "var(--text-primary)",
                    }}
                  >
                    <div className="flex items-center justify-between gap-3 mb-3">
                      <MessageSquareText size={18} className="text-[var(--accent-warm)] shrink-0" />
                      <button
                        onClick={() => void deleteRoom(room)}
                        className="p-1 rounded-md hover:bg-white/5 transition-colors"
                        aria-label={`${room.title} 삭제`}
                        title="방 삭제"
                      >
                        <Trash2 size={15} className="text-[var(--text-secondary)]" />
                      </button>
                    </div>
                    <h2 className="text-base font-bold leading-snug mb-2">{room.title}</h2>
                    <p className="text-sm leading-relaxed text-[var(--text-secondary)] min-h-[42px]">
                      {roomSummary(room)}
                    </p>
                    <div className="mt-3 flex flex-wrap gap-1.5">
                      {room.topics.slice(0, 3).map((topic) => (
                        <span
                          key={topic}
                          className="px-2 py-0.5 rounded-md text-[11px] font-medium"
                          style={{ background: "var(--bg-elevated)", color: "var(--text-secondary)" }}
                        >
                          {topic}
                        </span>
                      ))}
                    </div>
                    <button
                      onClick={() => openRoom(room)}
                      className="mt-4 w-full h-9 rounded-lg text-sm font-semibold"
                      style={{ background: "var(--bg-elevated)", color: "var(--accent-warm)" }}
                    >
                      토론방 입장
                    </button>
                  </div>
                ))}
              </div>
            ) : (
              <div
                className="rounded-lg p-5 mb-8"
                style={{ background: "var(--bg-surface)", border: "1px solid var(--border-color)" }}
              >
                <h2 className="text-base font-bold text-[var(--text-primary)] mb-1">
                  아직 만든 토론방이 없습니다
                </h2>
                <p className="text-sm text-[var(--text-secondary)]">
                  아래에서 주제를 입력해 첫 토론방을 만들면 이곳에 저장됩니다.
                </p>
              </div>
            )}

            <div
              className="rounded-lg p-4"
              style={{ background: "var(--bg-surface)", border: "1px solid var(--border-color)" }}
            >
              <label className="block text-sm font-semibold text-[var(--text-secondary)] mb-3">
                새 토론방
              </label>
              <div className="flex flex-col sm:flex-row gap-2">
                <input
                  value={topicDraft}
                  onChange={(event) => setTopicDraft(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") handleCreateRoom();
                  }}
                  className="flex-1 h-12 px-4 rounded-lg outline-none text-base"
                  style={{
                    background: "var(--bg-elevated)",
                    border: "1px solid var(--border-color)",
                    color: "var(--text-primary)",
                  }}
                  placeholder={topicPlaceholder}
                />
                <button
                  onClick={handleCreateRoom}
                  className="h-12 px-5 rounded-lg flex items-center justify-center gap-2 font-semibold transition-opacity"
                  style={{
                    background: "var(--accent-warm)",
                    color: "#1E1E1E",
                  }}
                  title="랜덤 3명으로 바로 시작"
                >
                  <Plus size={18} />
                  만들기
                </button>
                <button
                  onClick={() => setBuilderOpen(true)}
                  className="h-12 px-5 rounded-lg flex items-center justify-center gap-2 font-semibold transition-opacity"
                  style={{
                    background: "var(--bg-elevated)",
                    color: "var(--accent-warm)",
                    border: "1px solid var(--border-color)",
                  }}
                  title="참가자 2~3명을 직접 구성해 시작"
                >
                  <Users size={18} />
                  직접 고르기
                </button>
              </div>
              {suggestedGroups.length > 0 ? (
                <div className="mt-3 flex flex-col gap-2.5">
                  {suggestedGroups.map((group) => (
                    <div key={group.category}>
                      <div className="text-[10px] font-bold uppercase tracking-wider text-[var(--text-secondary)] opacity-70 mb-1">
                        {group.category}
                      </div>
                      <div className="flex flex-wrap gap-1.5">
                        {group.topics.map((topic) => (
                          <button
                            key={topic}
                            onClick={() => setTopicDraft(topic)}
                            className="px-2 py-1 rounded-md text-[11px] font-medium text-left"
                            style={{ background: "var(--bg-elevated)", color: "var(--text-secondary)" }}
                          >
                            {topic}
                          </button>
                        ))}
                      </div>
                    </div>
                  ))}
                </div>
              ) : (
                <div className="mt-3 flex flex-wrap gap-1.5">
                  {TOPIC_SUGGESTIONS.map((topic) => (
                    <button
                      key={topic}
                      onClick={() => setTopicDraft(topic)}
                      className="px-2 py-1 rounded-md text-[11px] font-medium"
                      style={{ background: "var(--bg-elevated)", color: "var(--text-secondary)" }}
                    >
                      {topic}
                    </button>
                  ))}
                </div>
              )}
            </div>
          </section>
        </main>
        <CreateRoomDialog
          open={builderOpen}
          onOpenChange={setBuilderOpen}
          topic={topicDraft.trim() || topicPlaceholder}
          onStart={handleCreateRoomWithPersonas}
        />
      </div>
    );
  }

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
        onLeave={handleLeaveRoom}
        onReset={handleResetDebate}
        onDelete={handleDeleteActiveRoom}
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
      <div
        className="flex flex-1 overflow-hidden relative z-10 transition-all duration-300"
        style={{ paddingTop: connected ? 64 : 100 }}
      >
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
            onSetTopics={handleSetTopics}
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
          onInvite={sendInvite}
          onRemove={sendRemove}
          onPace={sendPace}
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
