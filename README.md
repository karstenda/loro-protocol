# Loro Protocol Monorepo

loro-protocol is a small, transport-agnostic syncing protocol for collaborative CRDT documents. This repo hosts the protocol implementation, a WebSocket client, and minimal servers for local testing or self‑hosting.

- Protocol: multiplex multiple rooms on one connection, 256 KiB max per message, large update fragmentation supported
- CRDTs: Loro document, Loro ephemeral store; extensible (e.g., Yjs, Yjs Awareness)
- Transports: WebSocket or any integrity-preserving transport (e.g., WebRTC)

See `protocol.md` for the full wire spec.

## Packages

- `packages/loro-protocol` (MIT): Core TypeScript definitions, encoders/decoders for the wire protocol
- `packages/loro-websocket` (MIT): WebSocket client + a SimpleServer for local testing
- `packages/loro-adaptors` (MIT): Shared CRDT adaptors for Loro documents and ephemeral state

Rust workspace (AGPL):

- `rust/loro-protocol`: Rust encoder/decoder mirroring the TS implementation
- `rust/loro-websocket-client`: Async WS client for the protocol
- `rust/loro-websocket-server`: Minimal async WS server with optional SQLite snapshotting

## Quick Start (Local)

Use the minimal WebSocket server for local development and tests.

1. Install dependencies

```bash
pnpm install
pnpm -r build
```

2. Start a SimpleServer (Node.js)

Create `examples/simple-server.ts`:

```ts
import { SimpleServer } from "loro-websocket/server";

const server = new SimpleServer({ port: 8787 });
server.start().then(() => {
  // eslint-disable-next-line no-console
  console.log("SimpleServer listening on ws://localhost:8787");
});
```

Run it (Node 18+):

```bash
node --loader ts-node/esm examples/simple-server.ts
# or compile to JS first with your preferred setup
```

3. Connect a client and sync a Loro document

```ts
// examples/client.ts
import { LoroWebsocketClient, createLoroAdaptor } from "loro-websocket";

// In Node, provide a WebSocket implementation
import { WebSocket } from "ws";
(globalThis as any).WebSocket = WebSocket;

const client = new LoroWebsocketClient({ url: "ws://localhost:8787" });
await client.waitConnected();

const adaptor = createLoroAdaptor({ peerId: 1 });
const room = await client.join({ roomId: "demo-room", crdtAdaptor: adaptor });

// Edit the shared doc
const text = adaptor.getDoc().getText("content");
text.insert(0, "Hello, Loro!");
adaptor.getDoc().commit();

// Later…
await room.destroy();
```

Tip: For a working reference, see `packages/loro-websocket/src/e2e.test.ts` which spins up `SimpleServer` and syncs two clients end‑to‑end.

### Optional: SimpleServer hooks

`SimpleServer` accepts optional hooks for basic auth and persistence:

```ts
const server = new SimpleServer({
  port: 8787,
  authenticate: async (roomId, crdt, auth) => {
    // return 'read' | 'write' | null to deny
    return "write";
  },
  onLoadDocument: async (roomId, crdt) => null, // return snapshot bytes
  onSaveDocument: async (roomId, crdt, data) => {
    // persist snapshot bytes somewhere (e.g., filesystem/db)
  },
  saveInterval: 60_000, // ms
});
```

### Alternative: Rust server

The Rust workspace contains a minimal async WebSocket server (`loro-websocket-server`) with optional SQLite persistence. See `rust/loro-websocket-server/examples/simple-server.rs` for a CLI example.

## Protocol Highlights

- Magic bytes per CRDT: "%LOR" (Loro doc), "%EPH" (Loro ephemeral), "%YJS", "%YAW", …
- Messages: JoinRequest/JoinResponseOk/JoinError, DocUpdate, DocUpdateFragmentHeader/Fragment, UpdateError, Leave
- Limits: 256 KiB per message; large updates must be fragmented; default reassembly timeout 10s
- Multi‑room: room ID is part of every message; one connection can join multiple rooms

See `protocol.md` for the full description and error codes.

## Monorepo Dev

- Build all: `pnpm -r build`
- Test all: `pnpm -r test`
- Typecheck: `pnpm -r typecheck`
- Lint: `pnpm -r lint`

Node 18+ is required for local development.

## Licensing

- `loro-protocol`: MIT
- `loro-websocket`: MIT
- `loro-adaptors`: MIT
- Rust workspace crates under `rust/`: AGPL-3.0-only

## Project Structure

```
.
├── protocol.md                 # Wire protocol spec
├── packages/
│   ├── loro-protocol/          # Core encoders/decoders (MIT)
│   ├── loro-websocket/         # Client + SimpleServer (MIT)
│   ├── loro-adaptors/          # Shared CRDT adaptors (MIT)
├── rust/                       # Rust implementations (AGPL)
│   ├── loro-protocol/
│   ├── loro-websocket-client/
│   └── loro-websocket-server/
└── pnpm-workspace.yaml
```

## FAQ

- How do I test locally? Use `SimpleServer` in `loro-websocket` or the Rust server.
- Can I bring my own auth/storage? Yes — `SimpleServer` and the Rust server provide hooks for auth and persistence.
