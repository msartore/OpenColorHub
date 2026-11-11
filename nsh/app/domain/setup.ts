/**
 * Saved-preset rules: the name field and the counts shown in the list. A preset
 * covers one half of the machine, so the summary never claims the other moved.
 */

import { rgbToHex } from "./lighting";
import type { Setup, SetupChannelSetting } from "./model";

export const SETUP_NAME_MAX = 48;

/** Trimmed name, or null when the field cannot be saved. */
export function normalizeSetupName(raw: string): string | null {
  const name = raw.trim().slice(0, SETUP_NAME_MAX);
  return name.length > 0 ? name : null;
}

export interface SetupSummary {
  /** Fans: manual vs curve-driven. */
  manual: number;
  curves: number;
  headers: number;
  /** Lighting: how many channels are lit, and in how many distinct colours. */
  lit: number;
  channels: number;
  colors: string[];
}

function litColor(channel: SetupChannelSetting): string {
  return rgbToHex(channel.color);
}

export function summarizeSetup(setup: Setup): SetupSummary {
  const curves = setup.fans.filter((fan) => fan.curve !== null).length;
  const lit = setup.channels.filter((channel) => channel.enabled);
  return {
    manual: setup.fans.length - curves,
    curves,
    headers: setup.fans.length,
    lit: lit.length,
    channels: setup.channels.length,
    colors: [...new Set(lit.map(litColor))],
  };
}
