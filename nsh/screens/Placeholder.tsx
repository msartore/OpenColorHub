/** A screen that has not been ported yet, kept honest about what is live. */

import { Text, View } from "nsh";

import { Panel } from "../components/ui";

const REMAINING: Record<string, { title: string; body: string; next: string }> = {
  drivers: {
    title: "Drivers",
    body: "The vendor catalogue, cached-then-refreshed, with download progress.",
    next: "Slice 6: catalogue work comes from the core, as in the Tauri build.",
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
