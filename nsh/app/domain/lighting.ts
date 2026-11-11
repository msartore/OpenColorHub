/**
 * Colour and frame rules for lighting. A channel crosses three forms: the hex
 * string a picker speaks, three serde bytes, and the 200-LED frame the
 * controller receives. Conversions and the preview expansion live here.
 */

import type { ChannelId, ChannelStatus } from "./model";

export type Rgb = readonly [number, number, number];

export const LED_TOTAL = 200;
export const CHANNEL_ORDER: readonly ChannelId[] = [
  "led",
  "rgb1",
  "rgb2",
  "argb1",
  "argb2",
  "argb3",
];

export function clampByte(value: number): number {
  return Math.min(255, Math.max(0, Math.round(value)));
}

export function clampBrightness(percent: number): number {
  return Math.min(100, Math.max(0, Math.round(percent)));
}

export function rgbToHex(color: Rgb): string {
  return `#${color
    .map((part) => clampByte(part).toString(16).padStart(2, "0"))
    .join("")}`;
}

/** Three bytes from `#rgb` / `#rrggbb` (falls back to black). */
export function hexToRgb(hex: string): [number, number, number] {
  const clean = hex.trim().replace(/^#/, "");
  const full =
    clean.length === 3
      ? clean
          .split("")
          .map((char) => char + char)
          .join("")
      : clean;
  if (!/^[0-9a-fA-F]{6}$/.test(full)) return [0, 0, 0];
  return [
    parseInt(full.slice(0, 2), 16),
    parseInt(full.slice(2, 4), 16),
    parseInt(full.slice(4, 6), 16),
  ];
}

export function isHexColor(value: string): boolean {
  return /^#[0-9a-fA-F]{6}$/.test(value);
}

export function normalizeColor(color: Rgb): Rgb {
  return [clampByte(color[0]), clampByte(color[1]), clampByte(color[2])];
}

/** Changes whenever a snapshot would repaint a channel card. */
export function channelSignature(channel: ChannelStatus): string {
  return `${channel.hex}|${channel.brightness}|${channel.enabled}`;
}

/**
 * The 200-LED frame for the preview: each channel owns its slice, filled with
 * the colour the controller renders (brightness already applied). Channels that
 * report no LEDs are skipped; a short frame is padded with black.
 */
export function expandFrame(channels: readonly ChannelStatus[]): string[] {
  const frame: string[] = [];
  for (const channel of channels) {
    const color = channel.enabled ? channel.render_hex : "#000000";
    for (let index = 0; index < channel.leds; index += 1) {
      if (frame.length >= LED_TOTAL) break;
      frame.push(color);
    }
  }
  while (frame.length < LED_TOTAL) frame.push("#000000");
  return frame;
}
