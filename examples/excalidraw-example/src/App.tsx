import { useCallback, useRef, useState } from "react";
import { Excalidraw } from "@excalidraw/excalidraw";
import { useLoroSync } from "./hooks/useLoroSync";
import CollaboratorsCursors from "./components/CollaboratorsCursors";
import { ExcalidrawImperativeAPI } from "@excalidraw/excalidraw/types/types";
import { LoroDoc } from "loro-crdt";

// Generate random user color
function getRandomColor() {
  const colors = [
    "#FF6B6B",
    "#4ECDC4",
    "#45B7D1",
    "#96CEB4",
    "#FFEAA7",
    "#DDA0DD",
    "#98D8C8",
    "#FFD93D",
  ];
  return colors[Math.floor(Math.random() * colors.length)];
}

// Generate random user ID
function generateUserId() {
  return new LoroDoc().peerIdStr;
}

// Generate random user name
function generateUserName() {
  const adjectives = ["Happy", "Creative", "Smart", "Fast", "Cool", "Brave"];
  const nouns = ["Artist", "Designer", "Developer", "Creator", "Builder"];
  const adjective = adjectives[Math.floor(Math.random() * adjectives.length)];
  const noun = nouns[Math.floor(Math.random() * nouns.length)];
  return `${adjective} ${noun}`;
}

function App() {
  const [roomId] = useState(() => {
    // Get room ID from URL or generate new one
    const params = new URLSearchParams(window.location.search);
    return params.get("room") || "default-room";
  });

  const [userId] = useState(generateUserId);
  const [userName] = useState(generateUserName);
  const [userColor] = useState(getRandomColor);
  const excalidrawAPI = useRef<ExcalidrawImperativeAPI | null>(null);

  const { isConnected, collaborators, onChange, updateCursor } = useLoroSync({
    roomId,
    userId,
    userName,
    userColor,
    wsUrl: import.meta.env.VITE_WS_URL,
    excalidrawAPI,
  });

  // Handle pointer move for cursor tracking
  const handlePointerMove = useCallback(
    (event: React.PointerEvent) => {
      const rect = event.currentTarget.getBoundingClientRect();
      const x = event.clientX - rect.left;
      const y = event.clientY - rect.top;
      updateCursor({ x, y });
    },
    [updateCursor]
  );

  return (
    <div className="h-screen w-screen flex flex-col bg-gray-50">
      {/* Header */}
      <div className="bg-white shadow-sm border-b border-gray-200 px-4 py-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center space-x-4">
            <h1 className="text-xl font-semibold text-gray-800">
              Excalidraw + Loro Collaboration
            </h1>
            <div className="flex items-center space-x-2">
              <div
                className={`w-2 h-2 rounded-full ${isConnected ? "bg-green-500" : "bg-red-500"
                  }`}
              />
              <span className="text-sm text-gray-600">
                {isConnected ? "Connected" : "Disconnected"}
              </span>
            </div>
          </div>

          <div className="flex items-center space-x-4">
            {/* User info */}
            <div className="flex items-center space-x-2">
              <div
                className="w-8 h-8 rounded-full flex items-center justify-center text-white font-semibold text-sm"
                style={{ backgroundColor: userColor }}
              >
                {userName.charAt(0)}
              </div>
              <span className="text-sm text-gray-700">{userName}</span>
            </div>

            {/* Collaborators */}
            {collaborators.size > 0 && (
              <div className="flex items-center space-x-2">
                <span className="text-sm text-gray-600">
                  {collaborators.size} other{collaborators.size === 1 ? "" : "s"} online
                </span>
                <div className="flex -space-x-2">
                  {Array.from(collaborators.values())
                    .slice(0, 5)
                    .map((collaborator) => (
                      <div
                        key={collaborator.userId}
                        className="w-8 h-8 rounded-full flex items-center justify-center text-white font-semibold text-sm border-2 border-white"
                        style={{ backgroundColor: collaborator.userColor }}
                        title={collaborator.userName}
                      >
                        {collaborator.userName.charAt(0)}
                      </div>
                    ))}
                  {collaborators.size > 5 && (
                    <div className="w-8 h-8 rounded-full flex items-center justify-center bg-gray-400 text-white font-semibold text-sm border-2 border-white">
                      +{collaborators.size - 5}
                    </div>
                  )}
                </div>
              </div>
            )}

            {/* Room info */}
            <div className="text-sm text-gray-600 bg-gray-100 px-3 py-1 rounded">
              Room: <span className="font-mono">{roomId}</span>
            </div>
          </div>
        </div>
      </div>

      {/* Excalidraw */}
      <div className="flex-1 relative" onPointerMove={handlePointerMove}>
        <Excalidraw
          onChange={onChange}
          excalidrawAPI={(api) => {
            excalidrawAPI.current = api;
          }}
        />

        {/* Render collaborator cursors */}
        <CollaboratorsCursors collaborators={collaborators} />
      </div>
    </div>
  );
}

export default App;
