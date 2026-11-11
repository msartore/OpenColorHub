import type { ScreenId, SettingsPatch, Settings, Snapshot } from "./types";
import type { Tone } from "./components/ui";

export interface ViewContext {
  settings(): Settings;
  /** Version string of the running build, e.g. `0.2.0`. */
  version(): string;
  snapshot(): Snapshot | null;
  /** Transient message in the bottom-right stack. */
  toast(message: string, tone?: Tone): void;
  /** True when the action may proceed (always true unless the user opted into confirmations). */
  confirmWrite(message: string): boolean;
  navigate(screen: ScreenId): void;
  saveSettings(patch: SettingsPatch): Promise<void>;
  applySnapshot(snapshot: Snapshot): void;
  refresh(): Promise<void>;
}

export interface View {
  el: HTMLElement;
  update(snapshot: Snapshot): void;
}

export type ViewFactory = (ctx: ViewContext) => View;
