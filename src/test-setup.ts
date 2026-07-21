import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

// vitest.config.ts does not set test.globals, so Testing Library's
// auto-cleanup (which relies on a global `afterEach`) never registers.
// Without this, DOM from one test leaks into the next.
afterEach(() => {
  cleanup();
});
