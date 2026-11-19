/**
 * Width-driven layout for the desktop window.
 *
 * NSH has no CSS media queries in the TSX we write, so the few places that need
 * to change shape read `windowSize()` and pick a size. The thresholds are about
 * the window, not the content: the sidebar width changes with the same value.
 */

import { windowSize } from "nsh";

export type LayoutSize = "wide" | "medium" | "narrow";

export function layoutSize(): LayoutSize {
  const width = windowSize().width;
  if (width >= 1200) return "wide";
  if (width >= 900) return "medium";
  return "narrow";
}

export function sidebarWidth(): number {
  const size = layoutSize();
  return size === "wide" ? 228 : size === "medium" ? 200 : 66;
}

/** 4, 2 or 1 per row, leaving room for the gap. */
export function statWidth(): `${number}%` {
  const size = layoutSize();
  return size === "wide" ? "24%" : size === "medium" ? "49%" : "100%";
}

/** Fan cards are dense; two fit beside each other on a desktop window. */
export function fanCardWidth(): `${number}%` {
  return layoutSize() === "narrow" ? "100%" : "49%";
}

/** Channel cards carry more controls than stat tiles. */
export function channelWidth(): `${number}%` {
  const size = layoutSize();
  return size === "wide" ? "32%" : size === "medium" ? "49%" : "100%";
}

/** Two curve points fit on one line inside a fan card; four do not. */
export function curvePointWidth(): `${number}%` {
  return "48%";
}

/** The two settings columns stack below this width. */
export function isNarrow(): boolean {
  return layoutSize() === "narrow";
}

/** A dialog is never wider than the window it is in. */
export function dialogWidth(preferred: number): number {
  return Math.max(260, Math.min(preferred, windowSize().width - 32));
}
