/**
 * System policy. Starting with the OS is only useful if the saved lighting is
 * applied too, so enabling autostart also turns that setting on. This is the
 * same coupling the old settings screen had; it now lives where it can be
 * tested.
 */

import type { AutostartState, Settings, SettingsPatch } from "../../domain/model";

export interface AutostartPort {
  setAutostart(enabled: boolean): Promise<AutostartState>;
  saveSettings(patch: SettingsPatch): Promise<Settings>;
}

export interface AutostartOutcome {
  state: AutostartState;
  /** The settings after the coupling, when they changed. */
  settings: Settings | null;
}

export async function setAutostart(
  port: AutostartPort,
  enabled: boolean,
): Promise<AutostartOutcome> {
  const state = await port.setAutostart(enabled);
  if (!enabled) return { state, settings: null };
  const settings = await port.saveSettings({ reapply_led_on_launch: true });
  return { state, settings };
}
