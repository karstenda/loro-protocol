import React from "react";

interface Collaborator {
  userId: string;
  userName: string;
  userColor: string;
  cursor?: { x: number; y: number };
  selectedElementIds?: string[];
  lastActive: number;
}

interface Props {
  collaborators: Map<string, Collaborator>;
}

const CollaboratorsCursors: React.FC<Props> = ({ collaborators }) => {
  return (
    <>
      {Array.from(collaborators.values()).map((collaborator) => {
        if (!collaborator.cursor) return null;
        
        return (
          <div
            key={collaborator.userId}
            className="absolute pointer-events-none z-50"
            style={{
              left: collaborator.cursor.x,
              top: collaborator.cursor.y,
              transform: "translate(-50%, -50%)",
            }}
          >
            {/* Cursor pointer */}
            <svg
              width="24"
              height="24"
              viewBox="0 0 24 24"
              fill="none"
              style={{
                filter: "drop-shadow(0 2px 4px rgba(0,0,0,0.2))",
              }}
            >
              <path
                d="M3 3L10.07 19.97L12.58 12.58L19.97 10.07L3 3Z"
                fill={collaborator.userColor}
                stroke="white"
                strokeWidth="1.5"
                strokeLinejoin="round"
              />
            </svg>
            
            {/* User name label */}
            <div
              className="absolute top-5 left-5 px-2 py-1 rounded text-xs text-white font-medium whitespace-nowrap"
              style={{
                backgroundColor: collaborator.userColor,
                boxShadow: "0 2px 4px rgba(0,0,0,0.2)",
              }}
            >
              {collaborator.userName}
            </div>
          </div>
        );
      })}
    </>
  );
};

export default CollaboratorsCursors;