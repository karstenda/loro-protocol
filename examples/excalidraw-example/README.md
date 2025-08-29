# Excalidraw + Loro Collaboration Example

This example demonstrates real-time collaborative drawing using [Excalidraw](https://excalidraw.com/) with [Loro CRDT](https://loro.dev/) for state synchronization.

## Features

- **Real-time collaboration**: Multiple users can draw simultaneously
- **CRDT-based sync**: Uses Loro for conflict-free collaborative editing
- **Cursor tracking**: See other users' cursors in real-time using Loro's EphemeralStore
- **Automatic reconnection**: WebSocket client automatically reconnects on disconnect
- **Room-based collaboration**: Users can join different rooms via URL parameters

## Architecture

- **Frontend**: React + Excalidraw + Loro CRDT
- **Backend**: Simple WebSocket server with Loro document persistence
- **Sync Protocol**: Direct Loro binary updates over WebSocket
- **Ephemeral State**: Cursor positions and user presence using EphemeralStore

## Getting Started

### Install dependencies

```bash
pnpm install
```

### Start the WebSocket server

```bash
pnpm server
```

The server will start on `ws://localhost:8080`.

### Start the development server

In a new terminal:

```bash
pnpm dev
```

The app will open at `http://localhost:3000`.

### Join a room

By default, users join the "default-room". To join a specific room, add the `room` query parameter:

```
http://localhost:3000?room=my-custom-room
```

Share this URL with others to collaborate in the same room.

## How it works

1. **Loro Document Structure**:
   - `elements` (LoroList): Stores Excalidraw drawing elements
   - `appState` (LoroMap): Stores Excalidraw app state (colors, themes, etc.)

2. **Ephemeral State**:
   - User cursors and presence are synced using Loro's EphemeralStore
   - This data is not persisted and expires after 30 seconds of inactivity

3. **Sync Flow**:
   - Local changes → Loro document → Binary updates → WebSocket → Other clients
   - Remote updates → WebSocket → Loro import → Update Excalidraw scene

## Development

### Type checking

```bash
pnpm typecheck
```

### Linting

```bash
pnpm lint
```

### Building

```bash
pnpm build
```

## Notes

- The server currently stores documents in memory only. In production, you would persist them to a database.
- The example uses a simplified WebSocket protocol. For production, consider using the full loro-protocol implementation.
- Cursor positions are tracked at the container level, not transformed to Excalidraw's coordinate system.