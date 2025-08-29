# loro-adaptors

Shared adaptors that bridge the Loro protocol to `loro-crdt` documents and the ephemeral store.

## Install

```bash
pnpm add loro-adaptors loro-protocol loro-crdt
```

## Why

The websocket client (`loro-websocket`) speaks the binary wire protocol. These adaptors connect that client to concrete CRDT state:

- `LoroAdaptor`: wraps a `LoroDoc` and streams local updates to the connection; applies remote updates on receipt
- `LoroEphemeralAdaptor`: wraps an `EphemeralStore` for transient presence/cursor data

## Usage

```ts
import { LoroWebsocketClient } from "loro-websocket/client";
import { LoroAdaptor, LoroEphemeralAdaptor } from "loro-adaptors";
import { LoroDoc, EphemeralStore } from "loro-crdt";

const client = new LoroWebsocketClient({ url: "ws://localhost:8787" });
await client.waitConnected();

// Document state
const doc = new LoroDoc();
const docAdaptor = new LoroAdaptor(doc, { peerId: 1 });
const roomDoc = await client.join({ roomId: "demo", crdtAdaptor: docAdaptor });

// Ephemeral presence
const eph = new EphemeralStore();
const ephAdaptor = new LoroEphemeralAdaptor(eph);
const roomEph = await client.join({ roomId: "demo", crdtAdaptor: ephAdaptor });

// Make edits
doc.getText("content").insert(0, "hello");
doc.commit();

// Cleanup
await roomEph.destroy();
await roomDoc.destroy();
```

## API

- `new LoroAdaptor(doc?: LoroDoc, config?: { peerId?, recordTimestamp?, changeMergeInterval?, onUpdateError? })`
- `new LoroEphemeralAdaptor(store?: EphemeralStore, config?: { timeout?, onUpdateError? })`
- Helpers: `createLoroAdaptor`, `createLoroAdaptorFromDoc`, `createLoroEphemeralAdaptor`, `createLoroEphemeralAdaptorFromStore`

## Development

```bash
pnpm build
pnpm test
pnpm typecheck
```

## License

MIT
