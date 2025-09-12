# loro-websocket

WebSocket client and a minimal SimpleServer for syncing Loro CRDTs. Supports message fragmentation/reassembly (≤256 KiB), connection‑scoped keepalive ("ping"/"pong"), permission hooks, optional persistence hooks, and routing of %ELO end‑to‑end encrypted updates.

## Install

```bash
pnpm add loro-websocket loro-adaptors loro-protocol
# plus peer dep in your app
pnpm add loro-crdt
```

## Client

```ts
// In Node, provide a WebSocket implementation
import { WebSocket } from "ws";
(globalThis as any).WebSocket = WebSocket as unknown as typeof globalThis.WebSocket;

import { LoroWebsocketClient } from "loro-websocket";
import { createLoroAdaptor } from "loro-adaptors";

const client = new LoroWebsocketClient({ url: "ws://localhost:8787" });
await client.waitConnected();

const adaptor = createLoroAdaptor({ peerId: 1 });
const room = await client.join({ roomId: "demo", crdtAdaptor: adaptor });

// Edit
const text = adaptor.getDoc().getText("content");
text.insert(0, "Hello, Loro!");
adaptor.getDoc().commit();

await room.destroy();
```

%ELO (end‑to‑end encrypted Loro) using `EloLoroAdaptor`:

```ts
import { LoroWebsocketClient } from "loro-websocket";
import { EloLoroAdaptor } from "loro-adaptors";

const key = new Uint8Array(32); key[0] = 1;
const client = new LoroWebsocketClient({ url: "ws://localhost:8787" });
await client.waitConnected();

const adaptor = new EloLoroAdaptor({ getPrivateKey: async () => ({ keyId: "k1", key }) });
const room = await client.join({ roomId: "secure-room", crdtAdaptor: adaptor });

adaptor.getDoc().getText("t").insert(0, "secret");
adaptor.getDoc().commit();
```

## SimpleServer

```ts
import { SimpleServer } from "loro-websocket/server";

const server = new SimpleServer({
  port: 8787,
  authenticate: async (_roomId, _crdt, auth) => {
    // return "read" | "write" | null
    return new TextDecoder().decode(auth) === "readonly" ? "read" : "write";
  },
  onLoadDocument: async (_roomId, _crdt) => null,
  onSaveDocument: async (_roomId, _crdt, _data) => {},
  saveInterval: 60_000,
});
await server.start();
```

## Behavior

- Fragmentation: oversize `DocUpdate` payloads are split into `DocUpdateFragmentHeader` + `DocUpdateFragment` frames and reassembled.
- Keepalive: text frames "ping" and "pong" are connection‑scoped and bypass the envelope.
- Permissions: pass an `authenticate` hook to return `"read" | "write" | null` per join.
- Persistence: optionally `onLoadDocument`/`onSaveDocument` snapshots for `%LOR` documents.
- %ELO: server indexes plaintext headers to backfill encrypted deltas; ciphertext is not decrypted.

## Node/Web Compatibility

- Client works in browsers (native WebSocket) and Node (supply `globalThis.WebSocket`).
- %ELO crypto relies on Web Crypto via `loro-protocol` helpers (Node 18+ provides it).

## License

MIT

