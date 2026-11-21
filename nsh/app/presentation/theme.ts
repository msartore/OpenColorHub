/**
 * Runtime theme. `nsh.json` fixes the theme at build time, so changing the
 * accent in Settings has to push a new theme through `setTheme` to repaint the
 * whole window. The values mirror `theme.json` (light and dark are the same
 * dark palette, as the app forces the dark appearance).
 */

import { setTheme, type Theme } from "nsh";

const BASE: Theme = {
  cssVars: {
    theme: {
      radius: "10px",
      "font-sans": 'Inter, "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif',
      "font-mono":
        'ui-monospace, "SF Mono", "JetBrains Mono", "Cascadia Mono", Menlo, monospace',
    },
    light: {
      background: "#07090b",
      foreground: "#e7eef4",
      card: "#0c1013",
      "card-foreground": "#e7eef4",
      popover: "#11161b",
      "popover-foreground": "#e7eef4",
      primary: "#4ade80",
      "primary-foreground": "#07090b",
      secondary: "#11161b",
      "secondary-foreground": "#e7eef4",
      muted: "#090c0f",
      "muted-foreground": "#9fb0bd",
      accent: "#11161b",
      "accent-foreground": "#e7eef4",
      destructive: "#f87171",
      border: "#1a2128",
      input: "#2a343d",
      ring: "#4ade80",
      "chart-1": "#4ade80",
      "chart-2": "#7dd3fc",
      "chart-3": "#fbbf24",
      "chart-4": "#f87171",
      "chart-5": "#e879f9",
      sidebar: "#090c0f",
      "sidebar-foreground": "#9fb0bd",
      "sidebar-primary": "#4ade80",
      "sidebar-accent": "#11161b",
      "sidebar-border": "#1a2128",
      warn: "#fbbf24",
      info: "#7dd3fc",
    },
    dark: {
      background: "#07090b",
      foreground: "#e7eef4",
      card: "#0c1013",
      "card-foreground": "#e7eef4",
      popover: "#11161b",
      "popover-foreground": "#e7eef4",
      primary: "#4ade80",
      "primary-foreground": "#07090b",
      secondary: "#11161b",
      "secondary-foreground": "#e7eef4",
      muted: "#090c0f",
      "muted-foreground": "#9fb0bd",
      accent: "#11161b",
      "accent-foreground": "#e7eef4",
      destructive: "#f87171",
      border: "#1a2128",
      input: "#2a343d",
      ring: "#4ade80",
      "chart-1": "#4ade80",
      "chart-2": "#7dd3fc",
      "chart-3": "#fbbf24",
      "chart-4": "#f87171",
      "chart-5": "#e879f9",
      sidebar: "#090c0f",
      "sidebar-foreground": "#9fb0bd",
      "sidebar-primary": "#4ade80",
      "sidebar-accent": "#11161b",
      "sidebar-border": "#1a2128",
      warn: "#fbbf24",
      info: "#7dd3fc",
    },
  },
};

/** The tokens that carry the product accent through the UI. */
function withAccent(accent: string): Record<string, string> {
  return {
    ...BASE.cssVars.dark!,
    primary: accent,
    "primary-foreground": "#07090b",
    ring: accent,
    "chart-1": accent,
    "sidebar-primary": accent,
  };
}

/** Applies the chosen accent to every accent token, in both appearances. */
export function applyAccent(accent: string): void {
  const colors = withAccent(accent);
  setTheme({
    cssVars: {
      theme: { ...BASE.cssVars.theme },
      light: colors,
      dark: colors,
    },
  });
}
