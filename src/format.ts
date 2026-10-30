/** Display formatting shared by the views. */

import type { TempUnit } from "./types";

export function tempValue(celsius: number | null | undefined, unit: TempUnit): number | null {
  if (celsius === null || celsius === undefined || Number.isNaN(celsius)) return null;
  return unit === "fahrenheit" ? celsius * 1.8 + 32 : celsius;
}

export function tempSymbol(unit: TempUnit): string {
  return unit === "fahrenheit" ? "°F" : "°C";
}

export function formatTemp(celsius: number | null | undefined, unit: TempUnit, digits = 0): string {
  const value = tempValue(celsius, unit);
  if (value === null) return "—";
  return `${value.toFixed(digits)}${tempSymbol(unit)}`;
}

export function formatRpm(rpm: number): string {
  return rpm.toLocaleString("en-US");
}

export function formatClock(ms: number): string {
  const d = new Date(ms);
  return d.toLocaleTimeString("en-GB", { hour12: false });
}

export function formatAgo(ms: number, now = Date.now()): string {
  const delta = Math.max(0, Math.round((now - ms) / 1000));
  if (delta < 5) return "just now";
  if (delta < 60) return `${delta}s ago`;
  const minutes = Math.round(delta / 60);
  if (minutes < 60) return `${minutes}m ago`;
  return `${Math.round(minutes / 60)}h ago`;
}

export function pluralize(count: number, singular: string, plural = `${singular}s`): string {
  return `${count} ${count === 1 ? singular : plural}`;
}

/** Duty colour ramp used by bars and gauges (cool → hot). */
export function dutyColor(percent: number): string {
  if (percent < 35) return "var(--accent)";
  if (percent < 70) return "#7dd3fc";
  if (percent < 90) return "var(--warn)";
  return "var(--danger)";
}

export function tempColor(celsius: number | null): string {
  if (celsius === null) return "var(--text-faint)";
  if (celsius < 45) return "var(--accent)";
  if (celsius < 65) return "#7dd3fc";
  if (celsius < 80) return "var(--warn)";
  return "var(--danger)";
}
