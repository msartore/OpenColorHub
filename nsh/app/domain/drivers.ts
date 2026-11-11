/**
 * Driver-catalogue rules: ordering, how much firmware is shown before "show
 * all", and how progress becomes a percentage. The network and the filesystem
 * stay in the native adapter.
 */

import type { DriverKind, DriverRow } from "./model";

export const DRIVER_KIND_ORDER: readonly DriverKind[] = [
  "bios",
  "driver",
  "utility",
  "manual",
  "other",
];

export const DRIVER_KIND_LABELS: Record<DriverKind, string> = {
  bios: "Firmware",
  driver: "Drivers",
  utility: "Utilities",
  manual: "Manuals",
  other: "Other",
};

export const VISIBLE_FIRMWARE = 4;

export function newerDrivers(files: readonly DriverRow[]): DriverRow[] {
  return files.filter((file) => file.newer);
}

export function rowsOfKind(files: readonly DriverRow[], kind: DriverKind): DriverRow[] {
  return files.filter((file) => file.kind === kind);
}

export interface VisibleRows {
  shown: DriverRow[];
  collapsed: boolean;
  hiddenCount: number;
}

/** Only firmware collapses; a long driver list is still worth scrolling. */
export function visibleRows(
  kind: DriverKind,
  rows: readonly DriverRow[],
  expanded: boolean,
): VisibleRows {
  const collapsed = kind === "bios" && rows.length > VISIBLE_FIRMWARE && !expanded;
  const shown = collapsed ? rows.slice(0, VISIBLE_FIRMWARE) : [...rows];
  return { shown, collapsed, hiddenCount: rows.length - shown.length };
}

/** `null` when the server sent no length; never invent a percentage. */
export function progressPercent(received: number, total: number | null): number | null {
  if (!total || total <= 0) return null;
  return Math.min(100, Math.round((received / total) * 100));
}

/**
 * The screen shows a cached answer immediately and asks the vendor once in the
 * background. That policy lives here so it is not buried in a component.
 */
export function shouldRefreshInBackground(cached: boolean): boolean {
  return cached;
}
