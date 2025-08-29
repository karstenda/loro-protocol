import { describe, it, expect } from "vitest";
import { LoroWebsocketServer } from "./index";

describe("LoroWebsocketServer", () => {
  it("starts on a port (placeholder)", () => {
    const s = new LoroWebsocketServer();
    const res = s.start(1234);
    expect(res).toBe("listening:1234");
  });
});
