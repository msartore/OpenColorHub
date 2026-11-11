import { expect, test } from "nsh/test";

import {
  clampBrightness,
  expandFrame,
  hexToRgb,
  isHexColor,
  rgbToHex,
} from "./lighting";
import type { ChannelStatus } from "./model";

test("hex and bytes round-trip", () => {
  expect(hexToRgb("#22D3EE")).toEqual([34, 211, 238]);
  expect(hexToRgb("f80")).toEqual([255, 136, 0]);
  expect(hexToRgb("not a colour")).toEqual([0, 0, 0]);
  expect(rgbToHex([34, 211, 238])).toBe("#22d3ee");
  expect(rgbToHex([300, -10, 12.6])).toBe("#ff000d");
  expect(isHexColor("#4ade80")).toBe(true);
  expect(isHexColor("#4ade8")).toBe(false);
});

test("brightness is clamped to the wire range", () => {
  expect(clampBrightness(120)).toBe(100);
  expect(clampBrightness(-4)).toBe(0);
  expect(clampBrightness(42.6)).toBe(43);
});

function channel(id: ChannelStatus["id"], leds: number, enabled: boolean): ChannelStatus {
  return {
    id,
    label: id,
    name: id,
    description: "",
    leds,
    color: [1, 2, 3],
    hex: "#010203",
    render_hex: "#112233",
    brightness: 50,
    enabled,
  };
}

test("the preview expands each channel's slice to 200 LEDs", () => {
  const frame = expandFrame([
    channel("led", 18, true),
    channel("rgb1", 1, false),
    channel("argb1", 60, true),
  ]);
  expect(frame.length).toBe(200);
  expect(frame[0]).toBe("#112233");
  expect(frame[18]).toBe("#000000");
  expect(frame[19]).toBe("#112233");
  expect(frame[199]).toBe("#000000");
});

test("a short frame is padded, a long one is capped", () => {
  expect(expandFrame([]).length).toBe(200);
  const frame = expandFrame([channel("led", 500, true)]);
  expect(frame.length).toBe(200);
  expect(frame[199]).toBe("#112233");
});
