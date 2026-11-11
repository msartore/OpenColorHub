/** A screen that has not been ported yet, kept honest about what is live. */

import { Text, View } from "nsh";

import { Panel } from "../components/ui";

const REMAINING: Record<string, { title: string; body: string; next: string }> = {
  leds: {
    title: "Lighting",
    body: "Per-channel colour, the six-channel global write and the 200-LED preview.",
    next: "Slice 3: channel cards, the global colour, the frame preview and the HID apply.",
  },
  drivers: {
    title: "Drivers",
    body: "The vendor catalogue, cached-then-refreshed, with download progress.",
    next: "Slice 6: catalogue work comes from the core, as in the Tauri build.",
  },
  settings: {
    title: "Settings",
    body: "Backend mode, polling, units, accent, safety, elevation, autostart and paths.",
    next: "Slice 4: the settings document and the safety controls.",
  },
  about: {
    title: "About",
    body: "Build facts, the detected board and the paths this install uses.",
    next: "Slice 4: the about payload is already exposed by the controller.",
  },
};

export function Placeholder(props: { screen: string }) {
  const detail = REMAINING[props.screen] ?? {
    title: "Not ported yet",
    body: "This screen has no NSH implementation.",
    next: "See MIGRATION.md for the slice order.",
  };

  return (
    <View class="gap-4">
      <Text class="text-xl font-semibold text-foreground">{detail.title}</Text>
      <Panel title="Not ported yet" subtitle="This branch runs the NSH port beside the Tauri build">
        <Text class="text-xs text-muted-foreground">{detail.body}</Text>
        <Text class="text-xs text-muted-foreground">{detail.next}</Text>
      </Panel>
    </View>
  );
}
