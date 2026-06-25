---
title: Redis multisession runtime bus
type: plan
status: in_progress
priority: P1
updated_at: 2026-06-25
owner: shared
summary: Redis is introduced as a volatile multisession coordination layer, not as the long-term memory store. SQLite remains the source of truth for room checkpoints and persona memory; Redis carries hot room commands, events, ownership, presence, and worker recovery metadata.
---

# Redis Multisession Runtime Bus

## Conclusion

Redis should be used for **multisession runtime coordination**, not for long-term memory.

The durable stores keep their current responsibility:

- `memory.db`: long-term persona memory, recall, BM25/vector data.
- `rooms.db`: durable room checkpoints and restart restore.
- Redis: hot room command/event bus, presence, owner lease, short-lived snapshots, and optional LLM job queue.

This keeps the memory engine authoritative and lets the web layer scale from one local room to many concurrent browser sessions and room workers.

## Current Facts

- The web server currently owns one in-process `LiveSession` and bridges it to WebSocket clients through `tokio::mpsc` commands and `broadcast` frames.
- `LiveSession` is the correct single-writer unit for a room: it owns Hawkes state, pending LLM generation, topics, participants, and recall insertion timing.
- `RoomStore` already saves and loads by `room_id`, but the current web path still defaults to the single room name `salon`.
- The existing SQLite room checkpoint is durable, simple, and suitable as source of truth. Replacing it with Redis would weaken the long-term memory boundary.

## Target Shape

```text
Browser WebSocket
  -> Web Gateway
  -> Redis stream room:{room_id}:cmd
  -> Room Worker that owns LiveSession(room_id)
  -> Redis stream/pubsub room:{room_id}:events
  -> Web Gateway fanout
  -> Browser WebSocket
```

The owner of a room is the only process allowed to mutate that room's `LiveSession`.
Other processes send commands into Redis and subscribe to events.

## Debate Startup Rule

A debate room needs an explicit topic and a deterministic opening path.

- A new debate room may be started with `--topic <text>`; comma-separated topics
  and repeated `--topic` flags are normalized into at most five topic tags.
- On an empty room, the web worker emits a `system` frame with the topic and
  injects a moderator-style opening prompt into the session. The first visible
  spoken answer is still selected by the engine, so no persona is hard-coded as
  the opener.
- On a restored room, an explicit `--topic` updates the active topic, but the
  worker does not replay an artificial opening over existing history.
- The topic/opening path is web startup policy. `LiveSession` remains the core
  model and only receives normalized topics plus normal human-focus stimulation.

## Redis Responsibilities

| Key | Type | Purpose | Durability |
|---|---|---|---|
| `room:{room_id}:owner` | string with TTL | worker lease / single-writer guard | volatile |
| `room:{room_id}:cmd` | stream | user messages, topic, invite, remove, pause, pace | replayable short retention |
| `room:{room_id}:cmd:cursor` | string | last command stream id consumed by the owner | volatile recovery marker |
| `room:{room_id}:events` | stream or pubsub | utterance, state, system frames | short retention / live fanout |
| `room:{room_id}:presence` | set/hash with TTL fields | connected users/tabs and heartbeats | volatile |
| `room:{room_id}:hot_snapshot` | string/json with TTL | fast reconnect without DB read | volatile cache |
| `llm:jobs` | stream | optional later queue for generation workers | replayable short retention |

Use Redis Streams for commands because lost commands are worse than duplicated state frames.
Use PubSub or capped event streams for fanout; event replay only needs a small recent window.

## Non-Responsibilities

Redis must not become:

- the authoritative long-term memory store;
- the vector/BM25 recall index;
- the only copy of room history;
- the place where API keys or provider secrets are stored.

## Implementation Phases

### Phase 0 - Room ID Boundary

Goal: remove the last hard dependency on `"salon"` from the web runtime.

- Add `room_id` to web state frames.
- Add `--room-id <id>` for `--web`.
- Make `LiveSession` carry an injected room id while preserving existing constructors with default `salon`.
- Save/load `RoomStore` with the selected room id.

This phase is Redis-free and should be fully testable with the current in-process bus.

Status: implemented on 2026-06-25. The web runtime can now be started against a selected room with `--room-id <id>`, while existing constructors still default to `salon`.

### Phase 1 - Bus Trait

Introduce an internal bus boundary without changing behavior.

```rust
trait SessionBus {
    fn publish_event(&self, room_id: &str, frame_json: &str);
    fn submit_command(&self, room_id: &str, command_json: &str);
}
```

Keep the first implementation in-process using the current `mpsc` and `broadcast`.
This prevents Redis from leaking directly into `LiveSession`.

Status: first boundary implemented on 2026-06-25. `SessionBus` is now a JSON
command/event boundary and `LiveSession` still remains Redis-free.

### Phase 2 - Redis Bus

Add an optional feature, likely `redis-bus`.

- Dependency: `redis` with async Tokio support and streams support.
- Config: `SALON_REDIS_URL`, default disabled.
- Command stream: `XADD room:{room_id}:cmd * payload <json>`.
- Worker read path: `XREAD BLOCK ... STREAMS room:{room_id}:cmd <last_id>`.
- Event path: either `PUBLISH room:{room_id}:events <json>` for live fanout, or `XADD` with `MAXLEN ~ N` for short replay.

The Redis crate supports Tokio async connections through multiplexed async connections and separate PubSub connections. Streams are available through command methods such as `xadd` and `xread_options`.

Status: first writer implementation added on 2026-06-25 behind `redis-bus`.
When `SALON_REDIS_URL` is set, the current web runtime mirrors accepted client
commands to `room:{room_id}:cmd` and emitted server frames to
`room:{room_id}:events` plus `room:{room_id}:events:pubsub`. The local
in-process worker still remains the owner; Redis command consumption and owner
failover are Phase 3 work.

Debate startup was added on 2026-06-25: `--web --topic <text>` sets initial
topics and starts empty rooms by injecting an opening prompt so the engine can
select the first persona response.

### Phase 3 - Room Worker Ownership

- Acquire `SET room:{room_id}:owner <worker_id> NX EX <ttl>`.
- Refresh owner TTL while the worker is healthy.
- If the owner dies, another worker restores from `rooms.db`, then resumes command stream processing.
- Persist checkpoints to `rooms.db` on dirty interval and before owner shutdown.

Status: partial owner/gateway split implemented on 2026-06-25. With
`redis-bus` and `SALON_REDIS_URL`, the process tries to acquire the owner lease.
The owner process starts the local `LiveSession`, reads accepted client commands
from `room:{room_id}:cmd`, refreshes the owner TTL, and publishes server frames
to Redis events. A non-owner process does not start a second engine; it writes
client commands to Redis and forwards `room:{room_id}:events:pubsub` to its local
WebSocket clients.

Command cursor persistence was added on 2026-06-25 with
`room:{room_id}:cmd:cursor`. The first owner starts at `$` to avoid replaying
stale retained commands. Once a cursor exists, a later owner reads from that id
so commands written during an owner gap can be replayed.

Remaining Phase 3 work: promote a gateway to owner when the lease disappears,
and add an integration test with a real Redis server for owner handoff.

### Phase 4 - Multisession UX

- Add room list/create/join UI.
- Route WebSocket by room id, for example `/ws/{room_id}` or initial `join` frame.
- Show presence separately from persona participants.
- Let multiple browser tabs observe and write to the same room.

## Invariants

- `LiveSession` remains single-writer per room.
- DB remains the durable source of truth.
- Redis data can be flushed without destroying long-term memory.
- A duplicated command must be detectable or harmless before Redis replay is enabled.
- Headless/driver golden paths stay Redis-free.

## Open Questions

- Whether event fanout should be PubSub-only or capped Streams with replay.
- Whether command stream consumer groups are needed immediately, or a simple owner-read stream is enough.
- Whether room workers live inside the web server process at first or become a separate binary.
- How much of `state` should be emitted periodically versus reconstructed from hot snapshot.
