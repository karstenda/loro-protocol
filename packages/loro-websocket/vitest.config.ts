import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "node",
    include: ["{tests,src}/**/*.test.{ts,tsx}"],
    pool: "threads",
    poolOptions: { threads: { minThreads: 1, maxThreads: 1 } },
  },
});
