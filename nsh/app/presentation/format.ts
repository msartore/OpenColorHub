/**
 * Display formatting and colour ramps. Presentation only: nothing here
 * decides what a value means.
 */

import type { FanModeId, TempUnit } from "../domain/model";

export function tempValue(celsius: number | null | undefined, unit: TempUnit): number | null {
  if (celsius === null || celsius === undefined || Number.isNaN(celsius)) return null;
  return unit === "fahrenheit" ? celsius * 1.8 + 32 : celsius;
}

export function tempSymbol(unit: TempUnit): string {
  return unit === "fahrenheit" ? "°F" : "°C";
}

export function formatTemp(
  celsius: number | null | undefined,
  unit: TempUnit,
  digits = 0,
): string {
  const value = tempValue(celsius, unit);
  if (value === null) return "—";
  return `${value.toFixed(digits)}${tempSymbol(unit)}`;
}

export function formatRpm(rpm: number): string {
  return rpm.toLocaleString("en-US");
}

export function formatClock(ms: number): string {
  const date = new Date(ms);
  return date.toLocaleTimeString("en-GB", { hour12: false });
}

export function pluralize(count: number, singular: string, plural = `${singular}s`): string {
  return `${count} ${count === 1 ? singular : plural}`;
}

export function fanModeLabel(mode: FanModeId, curvePercent: number | null): string {
  if (mode === "smart_fan") return `curve → ${curvePercent ?? "—"}%`;
  if (mode === "manual") return "manual";
  return "unknown";
}

/** Duty ramp used by bars and gauges (cool → hot). */
export function dutyColor(percent: number): string {
  if (percent < 35) return "#4ade80";
  if (percent < 70) return "#7dd3fc";
  if (percent < 90) return "#fbbf24";
  return "#f87171";
}

export function tempColor(celsius: number | null): string {
  if (celsius === null) return "#6f8090";
  if (celsius < 45) return "#4ade80";
  if (celsius < 65) return "#7dd3fc";
  if (celsius < 80) return "#fbbf24";
  return "#f87171";
}
