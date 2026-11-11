/**
 * Lighting writes. The core stages and applies in one call; these guard the
 * values (brightness, colour bytes) before they leave the app.
 */

import type { ActionResult, ChannelId } from "../../domain/model";
import {
  clampBrightness,
  hexToRgb,
  normalizeColor,
  type Rgb,
} from "../../domain/lighting";
import type { LightingPort } from "../ports";

export interface SetChannelInput {
  channel: ChannelId;
  color: Rgb;
  brightness: number;
  enabled: boolean;
}

export async function setChannel(
  port: LightingPort,
  input: SetChannelInput,
): Promise<ActionResult> {
  return port.setChannel(
    input.channel,
    normalizeColor(input.color),
    clampBrightness(input.brightness),
    input.enabled,
  );
}

export interface SetAllInput {
  color: Rgb;
  brightness: number;
  enabled: boolean;
}

export async function setAllChannels(
  port: LightingPort,
  input: SetAllInput,
): Promise<ActionResult> {
  return port.setAllChannels(
    normalizeColor(input.color),
    clampBrightness(input.brightness),
    input.enabled,
  );
}

/** Common case: a hex colour from a picker, as three bytes. */
export async function setAllFromHex(
  port: LightingPort,
  hex: string,
  brightness: number,
): Promise<ActionResult> {
  return setAllChannels(port, {
    color: hexToRgb(hex),
    brightness,
    enabled: true,
  });
}

export async function dimChannels(
  port: LightingPort,
  percent: number,
): Promise<ActionResult> {
  return port.setBrightness(clampBrightness(percent));
}

export async function turnOffChannel(
  port: LightingPort,
  channel: ChannelId,
): Promise<ActionResult> {
  return port.turnOffChannel(channel);
}

export async function allOff(port: LightingPort): Promise<ActionResult> {
  return setAllChannels(port, { color: [0, 0, 0], brightness: 0, enabled: false });
}
