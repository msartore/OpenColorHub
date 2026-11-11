import { expect, test } from "nsh/test";

import { normalizeSetupName, summarizeSetup } from "./setup";
import type { Setup } from "./model";

test("a preset name is trimmed, bounded and never empty", () => {
  expect(normalizeSetupName("  Silent  ")).toBe("Silent");
  expect(normalizeSetupName("   ")).toBe(null);
  expect(normalizeSetupName("x".repeat(80))?.length).toBe(48);
});

test("a fan preset counts manual and curve headers", () => {
  const setup: Setup = {
    id: "a",
    name: "Silent",
    kind: "fans",
    saved_at_ms: 1,
    fans: [
      { header: "sys", percent: 30, curve: null },
      { header: "cpu", percent: 50, curve: [{ temp_c: 20, percent: 20 }] },
      { header: "aux0", percent: 40, curve: null },
    ],
    channels: [],
  };
  const summary = summarizeSetup(setup);
  expect(summary.manual).toBe(2);
  expect(summary.curves).toBe(1);
  expect(summary.headers).toBe(3);
});

test("a lighting preset counts lit channels and distinct colours", () => {
  const setup: Setup = {
    id: "b",
    name: "Night",
    kind: "lighting",
    saved_at_ms: 1,
    fans: [],
    channels: [
      { channel: "led", color: [34, 211, 238], brightness: 50, enabled: true },
      { channel: "argb1", color: [34, 211, 238], brightness: 50, enabled: true },
      { channel: "argb2", color: [0, 0, 0], brightness: 0, enabled: false },
    ],
  };
  const summary = summarizeSetup(setup);
  expect(summary.lit).toBe(2);
  expect(summary.channels).toBe(3);
  expect(summary.colors).toEqual(["#22d3ee"]);
});
