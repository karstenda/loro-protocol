import { defineWorkspace } from "vitest/config";

export default defineWorkspace([
  "./packages/loro-adaptors/vitest.config.ts",
  "./packages/loro-websocket/vitest.config.ts",
  "./packages/loro-protocol/vitest.config.ts",
]);
