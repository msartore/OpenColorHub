/** Dashboard: the read-only view of the machine, ported from the DOM screen. */

import { Text, View, cleanup, computed, effect, signal } from "nsh";

import type { AppStore } from "../app/composition/store";
import type { ActivityEntry, Snapshot } from "../app/domain/model";
import {
  activityWindow,
  boardDisplayName,
  deviceLabel,
  summarizeTelemetry,
  temperatureStats,
  appendSample,
} from "../app/domain/telemetry";
import type { TemperatureStats } from "../app/domain/telemetry";
import { dutyColor, fanModeLabel, formatClock, formatRpm, formatTemp } from "../app/presentation/format";
import { layoutSize, statWidth } from "../app/presentation/layout";
import { Bar, Chip, KeyValue, Panel, Stat } from "../components/ui";

const HISTORY = 120;

function supportTone(confidence: string): "ok" | "warn" | "danger" {
  if (confidence === "verified") return "ok";
  if (confidence === "unsupported") return "danger";
  return "warn";
}

export function Dashboard(props: { store: AppStore }) {
  const samples: number[] = [];
  const stats = signal<TemperatureStats | null>(null);
  const activity = signal<ActivityEntry[]>([]);

  const summary = computed(() => {
    const snapshot = props.store.snapshot();
    return snapshot ? summarizeTelemetry(snapshot) : null;
  });

  effect(() => {
    const snapshot = props.store.snapshot();
    if (!snapshot) return;
    if (snapshot.temperature_c !== null) {
      appendSample(samples, snapshot.temperature_c, HISTORY);
    }
    stats.set(temperatureStats(samples));
  });

  const loadActivity = async (): Promise<void> => {
    try {
      activity.set(await props.store.activity());
    } catch {
      // Activity is diagnostic; a failed read must not break the screen.
    }
  };
  void loadActivity();
  const activityTimer = setInterval(() => void loadActivity(), 2000);
  cleanup(() => clearInterval(activityTimer));

  const tempText = (celsius: number): string =>
    formatTemp(celsius, props.store.settings().temp_unit, 0);

  return (
    <View class="gap-4">
      <View class="flex-row items-center justify-between gap-3">
        <Text class="text-xl font-semibold text-foreground">Dashboard</Text>
        <If when={props.store.snapshot()}>
          {(snapshot) => (
            <Chip
              label={`${boardDisplayName(snapshot().board)} · ${snapshot().board.confidence}`}
              tone={supportTone(snapshot().board.confidence)}
            />
          )}
        </If>
      </View>

      <If when={props.store.snapshot()} else={<Panel title="Waiting for the first reading" />}>
        {(snapshot) => (
          <View class="gap-4">
            <View class="flex-row gap-3" style={{ alignItems: "stretch" }}>
              <View style={{ flex: 1 }}>
                <Panel title="Motherboard temperature">
                  <View class="flex-row items-baseline gap-2">
                    <Text class="text-4xl font-semibold text-primary">
                      {formatTemp(snapshot().temperature_c, props.store.settings().temp_unit, 0)}
                    </Text>
                    <If when={snapshot().fan_device.simulated}>
                      <Chip label="simulated" tone="warn" />
                    </If>
                  </View>
                  <Text class="text-xs text-muted-foreground">
                    {snapshot().temperature_source ?? "unknown sensor"}
                    {snapshot().temperature_register ? ` (${snapshot().temperature_register})` : ""}
                  </Text>
                  <If when={stats()}>
                    {(current) => (
                      <Text class="text-[11px] font-mono text-muted-foreground">
                        {`session min ${tempText(current().min)} · mean ${tempText(current().mean)} · max ${tempText(current().max)} · ${current().count} samples`}
                      </Text>
                    )}
                  </If>
                </Panel>
              </View>
            </View>

            <View class="flex-row gap-3" style={{ flexWrap: "wrap" }}>
              <View style={{ width: statWidth() }}>
                <Stat
                  label="Average duty"
                  value={String(snapshot().avg_percent)}
                  unit="%"
                  hint={`across ${summary()?.present.length ?? 0} headers`}
                  tone={snapshot().avg_percent > 85 ? "warn" : "ok"}
                />
              </View>
              <View style={{ width: statWidth() }}>
                <Stat
                  label="Total rpm"
                  value={formatRpm(summary()?.totalRpm ?? 0)}
                  unit="rpm"
                  hint={`${summary()?.idle ?? 0} idle`}
                />
              </View>
              <View style={{ width: statWidth() }}>
                <Stat
                  label="Channels lit"
                  value={String(summary()?.lit.length ?? 0)}
                  hint={`${summary()?.litLeds ?? 0} of ${summary()?.totalLeds ?? 0} LEDs`}
                  tone={summary()?.lit.length ? "ok" : "muted"}
                />
              </View>
              <View style={{ width: statWidth() }}>
                <Stat
                  label="Curve driven"
                  value={String(summary()?.curved ?? 0)}
                  hint="headers on SmartFan"
                />
              </View>
            </View>

            <View
              class="gap-4"
              style={{
                flexDirection: layoutSize() === "narrow" ? "column" : "row",
                alignItems: "flex-start",
              }}
            >
              <View style={{ flex: 1 }}>
                <Panel title="Fan headers" subtitle="Live tachometer and duty">
                  <View class="gap-2">
                    <Each of={summary()?.present ?? []} key={(fan) => fan.header}>
                      {(fan) => (
                        <View class="gap-1">
                          <View class="flex-row items-baseline justify-between gap-3">
                            <Text class="text-xs text-foreground">{fan().label}</Text>
                            <View class="flex-row items-baseline gap-2">
                              <Text class="text-xs font-mono text-muted-foreground">
                                {`${formatRpm(fan().rpm)} rpm`}
                              </Text>
                              <Text class="text-xs font-mono text-foreground">{`${fan().percent}%`}</Text>
                            </View>
                          </View>
                          <Bar percent={fan().percent} color={dutyColor(fan().percent)} thin />
                          <Text class="text-[11px] text-muted-foreground">
                            {fanModeLabel(fan().mode, fan().curve_percent)}
                          </Text>
                        </View>
                      )}
                    </Each>
                    <If when={(summary()?.present.length ?? 0) === 0}>
                      <Text class="text-xs text-muted-foreground">
                        {snapshot().read_error
                          ? `The Super I/O did not answer: ${snapshot().read_error}`
                          : "No header is reporting a speed."}
                      </Text>
                    </If>
                  </View>
                  <If when={summary()?.fastest}>
                    {(fan) => (
                      <Text class="text-[11px] text-muted-foreground">
                        {`fastest ${fan().label} · ${formatRpm(fan().rpm)} rpm`}
                      </Text>
                    )}
                  </If>
                </Panel>
              </View>

              <View style={{ flex: 1 }}>
                <Panel title="System" subtitle="What this build is talking to">
                  <View class="gap-1.5">
                    <KeyValue label="board" value={boardDisplayName(snapshot().board)} />
                    <KeyValue
                      label="support"
                      value={snapshot().board.confidence}
                      tone={supportTone(snapshot().board.confidence)}
                    />
                    <KeyValue label="fan backend" value={deviceLabel(snapshot().fan_device)} />
                    <KeyValue label="lighting backend" value={deviceLabel(snapshot().led_device)} />
                    <KeyValue
                      label="bios"
                      value={snapshot().board.identity?.bios_version?.trim() || "—"}
                    />
                    <KeyValue
                      label="temp sensor"
                      value={snapshot().temperature_source ?? "—"}
                    />
                    <KeyValue
                      label="polling"
                      value={`${props.store.settings().poll_interval_ms} ms`}
                    />
                    <KeyValue label="last sample" value={formatClock(snapshot().timestamp_ms)} />
                  </View>
                </Panel>
              </View>
            </View>

            <Panel title="Activity" subtitle="The last writes this session">
              <View class="gap-1.5">
                <Each of={activityWindow(activity())} key="index">
                  {(entry) => (
                    <View class="flex-row items-baseline justify-between gap-3">
                      <View class="flex-row items-baseline gap-2" style={{ flexShrink: 1 }}>
                        <Chip label={entry().kind} tone="muted" />
                        <Text class="text-xs text-muted-foreground">{entry().message}</Text>
                      </View>
                      <Text class="text-[11px] font-mono text-muted-foreground">
                        {formatClock(entry().at_ms)}
                      </Text>
                    </View>
                  )}
                </Each>
                <If when={activity().length === 0}>
                  <Text class="text-xs text-muted-foreground">
                    No writes yet — changes you apply show up here.
                  </Text>
                </If>
              </View>
            </Panel>

            <If when={snapshot().capabilities.notes.length > 0}>
              <Panel title="Hardware notes" subtitle="Why a backend degraded">
                <Each of={snapshot().capabilities.notes} key="index">
                  {(note) => <Text class="text-xs text-muted-foreground">{note()}</Text>}
                </Each>
              </Panel>
            </If>
          </View>
        )}
      </If>
    </View>
  );
}
