/**
 * The app-state facade. Screens read signals and call commands here; they
 * never hold a `Controller` or touch a port.
 */

import { signal, type Accessor } from "nsh";

import type { ColorHubPort } from "../application/ports";
import { applyFanProfile, setFanCurve, setFanDuty } from "../application/usecases/fans";
import {
  allOff as turnAllOff,
  dimChannels,
  setAllFromHex,
  setChannel as setChannelUseCase,
  turnOffChannel,
} from "../application/usecases/lighting";
import { setAutostart as setAutostartUseCase } from "../application/usecases/system";
import { loadDrivers } from "../application/usecases/drivers";
import {
  applySetup as applySetupUseCase,
  deleteSetup as deleteSetupUseCase,
  listSetups as listSetupsUseCase,
  saveSetup as saveSetupUseCase,
} from "../application/usecases/setups";
import type {
  AboutInfo,
  ActionResult,
  AutostartState,
  ChannelId,
  CurvePoint,
  DownloadedFile,
  DriverFile,
  DriverList,
  DriverProgress,
  ElevationState,
  FanHeaderId,
  FanProfileId,
  GpuZoneState,
  PawnioProgress,
  PawnioStatus,
  Settings,
  SettingsPatch,
  SetupKind,
  SetupList,
  Snapshot,
  StoredFile,
} from "../domain/model";
import { isPumpHeader } from "../domain/fan";
import { hexToRgb } from "../domain/lighting";
import { DEFAULT_SETTINGS, mergeSettings } from "../domain/settings";

export interface FanCommands {
  /** Resolves with the message the Rust side chose, verbatim. */
  setDuty(header: FanHeaderId, percent: number): Promise<string>;
  setCurve(header: FanHeaderId, points: CurvePoint[]): Promise<string>;
  applyProfile(profile: FanProfileId): Promise<string>;
}

export interface LightingCommands {
  setChannel(
    channel: ChannelId,
    hex: string,
    brightness: number,
    enabled: boolean,
  ): Promise<string>;
  setAll(hex: string, brightness: number): Promise<string>;
  dim(percent: number): Promise<string>;
  off(channel: ChannelId): Promise<string>;
  allOff(): Promise<string>;
}

export interface GpuCommands {
  setZone(device: string, zone: string, state: GpuZoneState, apply: boolean): Promise<string>;
  apply(): Promise<string>;
  syncFromBoard(): Promise<string>;
}

export interface SystemCommands {
  about(): Promise<AboutInfo>;
  elevation(): Promise<ElevationState>;
  requestElevation(): Promise<string>;
  autostart(): Promise<AutostartState>;
  setAutostart(enabled: boolean): Promise<AutostartState>;
}

export interface SetupCommands {
  list(kind: SetupKind): Promise<SetupList>;
  save(kind: SetupKind, name: string, id?: string): Promise<SetupList>;
  apply(id: string): Promise<string>;
  remove(kind: SetupKind, id: string): Promise<SetupList>;
}

export interface DriverCommands {
  list(refresh: boolean): Promise<DriverList>;
  download(
    file: DriverFile,
    onProgress: (progress: DriverProgress) => void,
  ): Promise<DownloadedFile>;
  stored(): Promise<StoredFile[]>;
  reveal(path: string): Promise<void>;
}

export interface PawnioCommands {
  status(): Promise<PawnioStatus>;
  install(onProgress: (progress: PawnioProgress) => void): Promise<PawnioStatus>;
}

export interface AppStore {
  snapshot: Accessor<Snapshot | null>;
  settings: Accessor<Settings>;
  version: Accessor<string>;
  loaded: Accessor<boolean>;
  error: Accessor<string | null>;
  fans: FanCommands;
  lighting: LightingCommands;
  gpu: GpuCommands;
  system: SystemCommands;
  setups: SetupCommands;
  drivers: DriverCommands;
  pawnio: PawnioCommands;
  /** Loads settings, the first snapshot and build facts, then subscribes. */
  start(): Promise<void>;
  refresh(): Promise<void>;
  activity(): Promise<import("../domain/model").ActivityEntry[]>;
  saveSettings(patch: SettingsPatch): Promise<void>;
  dispose(): void;
}

export function createStore(port: ColorHubPort): AppStore {
  const snapshot = signal<Snapshot | null>(null);
  const settings = signal<Settings>(DEFAULT_SETTINGS);
  const version = signal("");
  const loaded = signal(false);
  const error = signal<string | null>(null);
  let stopTelemetry: (() => void) | null = null;

  const applySnapshot = (next: Snapshot): void => snapshot.set(next);
  const applyResult = (result: ActionResult): string => {
    applySnapshot(result.snapshot);
    return result.message;
  };

  const pumpFor = (header: FanHeaderId): boolean => {
    const fan = snapshot()?.fans.find((candidate) => candidate.header === header);
    return fan?.pump ?? isPumpHeader(settings(), header);
  };

  const fans: FanCommands = {
    async setDuty(header, percent) {
      const result = await setFanDuty(port, {
        header,
        percent,
        pump: pumpFor(header),
      });
      return applyResult(result);
    },
    async setCurve(header, points) {
      const result = await setFanCurve(port, {
        header,
        points,
        pump: pumpFor(header),
      });
      return applyResult(result);
    },
    async applyProfile(profile) {
      const headers = snapshot()?.fans.map((fan) => fan.header) ?? [];
      const result = await applyFanProfile(port, { headers, profile });
      return applyResult(result);
    },
  };

  const lighting: LightingCommands = {
    async setChannel(channel, hex, brightness, enabled) {
      const result = await setChannelUseCase(port, {
        channel,
        color: hexToRgb(hex),
        brightness,
        enabled,
      });
      return applyResult(result);
    },
    async setAll(hex, brightness) {
      return applyResult(await setAllFromHex(port, hex, brightness));
    },
    async dim(percent) {
      return applyResult(await dimChannels(port, percent));
    },
    async off(channel) {
      return applyResult(await turnOffChannel(port, channel));
    },
    async allOff() {
      return applyResult(await turnAllOff(port));
    },
  };

  const gpu: GpuCommands = {
    async setZone(device, zone, state, apply) {
      return applyResult(await port.setGpuZone(device, zone, state, apply));
    },
    async apply() {
      return applyResult(await port.applyGpu());
    },
    async syncFromBoard() {
      return applyResult(await port.syncGpuFromBoard());
    },
  };

  const system: SystemCommands = {
    about: () => port.about(),
    elevation: () => port.elevation(),
    requestElevation: () => port.requestElevation(),
    autostart: () => port.autostart(),
    async setAutostart(enabled) {
      const outcome = await setAutostartUseCase(port, enabled);
      if (outcome.settings) settings.set(outcome.settings);
      return outcome.state;
    },
  };

  const setups: SetupCommands = {
    list: (kind) => listSetupsUseCase(port, kind),
    save: (kind, name, id) => saveSetupUseCase(port, kind, name, id),
    async apply(id) {
      const result = await applySetupUseCase(port, id);
      return applyResult(result);
    },
    remove: (kind, id) => deleteSetupUseCase(port, kind, id),
  };

  const drivers: DriverCommands = {
    list: (refresh) => loadDrivers(port, refresh),
    download: (file, onProgress) => port.downloadDriver(file, onProgress),
    stored: () => port.downloadedDrivers(),
    reveal: (path) => port.revealDriver(path),
  };

  const pawnio: PawnioCommands = {
    status: () => port.pawnioStatus(),
    install: (onProgress) => port.installPawnio(onProgress),
  };

  return {
    snapshot,
    settings,
    version,
    loaded,
    error,
    fans,
    lighting,
    gpu,
    system,
    setups,
    drivers,
    pawnio,

    async start() {
      try {
        const [nextSettings, nextSnapshot, about] = await Promise.all([
          port.settings(),
          port.snapshot(),
          port.about(),
        ]);
        settings.set(nextSettings);
        snapshot.set(nextSnapshot);
        version.set(about.version);
        stopTelemetry = port.onTelemetry((next) => snapshot.set(next));
      } catch (cause) {
        console.error("session start failed", cause);
        error.set(
          cause instanceof Error
            ? `${cause.message}\n${cause.stack ?? ""}`
            : String(cause),
        );
      } finally {
        loaded.set(true);
      }
    },

    async refresh() {
      const [nextSettings, nextSnapshot] = await Promise.all([
        port.settings(),
        port.reloadState(),
      ]);
      settings.set(nextSettings);
      snapshot.set(nextSnapshot);
    },

    activity: () => port.activity(),

    async saveSettings(patch) {
      settings.set(mergeSettings(settings(), patch));
      const saved = await port.saveSettings(patch);
      settings.set(saved);
    },

    dispose() {
      stopTelemetry?.();
      stopTelemetry = null;
      port.shutdown();
    },
  };
}
