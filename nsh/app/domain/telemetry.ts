/**
 * Read-model rules: one snapshot becomes the numbers every screen shows.
 */

import type { BoardInfo, ChannelStatus, DeviceInfo, FanStatus, Snapshot } from "./model";

export function presentFans(snapshot: Snapshot): FanStatus[] {
  return snapshot.fans.filter((fan) => fan.detected);
}

export function litChannels(snapshot: Snapshot): ChannelStatus[] {
  return snapshot.channels.filter((channel) => channel.enabled);
}

export interface TelemetrySummary {
  present: FanStatus[];
  absent: number;
  idle: number;
  totalRpm: number;
  spinning: FanStatus[];
  fastest: FanStatus | null;
  fastestRpm: number;
  curved: number;
  lit: ChannelStatus[];
  litLeds: number;
  totalLeds: number;
}

export function summarizeTelemetry(snapshot: Snapshot): TelemetrySummary {
  const present = presentFans(snapshot);
  const spinning = present.filter((fan) => fan.rpm > 0);
  const fastest = spinning.reduce<FanStatus | null>(
    (best, fan) => (best === null || fan.rpm > best.rpm ? fan : best),
    null,
  );
  const lit = litChannels(snapshot);
  return {
    present,
    absent: snapshot.fans.length - present.length,
    idle: present.filter((fan) => fan.rpm === 0).length,
    totalRpm: present.reduce((sum, fan) => sum + fan.rpm, 0),
    spinning,
    fastest,
    fastestRpm: fastest?.rpm ?? 0,
    curved: snapshot.fans.filter((fan) => fan.mode === "smart_fan").length,
    lit,
    litLeds: lit.reduce((sum, channel) => sum + channel.leds, 0),
    totalLeds: snapshot.channels.reduce((sum, channel) => sum + channel.leds, 0),
  };
}

export interface TemperatureStats {
  min: number;
  max: number;
  mean: number;
  last: number;
  count: number;
}

export function temperatureStats(samples: readonly number[]): TemperatureStats | null {
  if (samples.length === 0) return null;
  let min = samples[0];
  let max = samples[0];
  let sum = 0;
  for (const sample of samples) {
    if (sample < min) min = sample;
    if (sample > max) max = sample;
    sum += sample;
  }
  return {
    min,
    max,
    mean: sum / samples.length,
    last: samples[samples.length - 1],
    count: samples.length,
  };
}

export function appendSample(samples: number[], value: number, capacity: number): number[] {
  samples.push(value);
  if (samples.length > capacity) samples.shift();
  return samples;
}

export function boardDisplayName(board: BoardInfo): string {
  return board.identity?.name?.trim() || board.identity?.vendor?.trim() || "unknown board";
}

export function deviceLabel(device: DeviceInfo): string {
  if (!device.available) return "unavailable";
  return `${device.name}${device.simulated ? " · simulated" : ""}`;
}

/** Newest `limit` activity entries, newest first. */
export function activityWindow<T>(entries: readonly T[], limit = 6): T[] {
  return entries.slice(-limit).reverse();
}
