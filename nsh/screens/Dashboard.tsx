/** Dashboard: the read-only view of the machine, ported from the DOM screen. */

import { Text, View, cleanup, computed, effect, signal } from "nsh";
import type { JSX } from "nsh";

import type { AppStore } from "../app/composition/store";
import type { ActivityEntry, Snapshot } from "../app/domain/model";
import {
  activityWindow,
  deviceLabel,
  summarizeTelemetry,
  temperatureStats,
  appendSample,
} from "../app/domain/telemetry";
import type { TemperatureStats } from "../app/domain/telemetry";
import { expandFrame } from "../app/domain/lighting";
import { fanModeLabel, formatClock, formatRpm, formatTemp } from "../app/presentation/format";
import { layoutSize } from "../app/presentation/layout";
import { Sparkline } from "../components/Sparkline";
import { TemperatureGauge } from "../components/TemperatureGauge";
import { Bar, BoardBadge, Chip, LedStrip, PageHeader, Panel, Stat } from "../components/ui";

const HISTORY = 120;

function supportTone(confidence: string): "ok" | "warn" | "danger" {
  if (confidence === "verified") return "ok";
  if (confidence === "unsupported") return "danger";
  return "warn";
}

/** Three columns on a desktop, two on a medium window, one on a phone. */
function tileWidth(): `${number}%` {
  const size = layoutSize();
  return size === "wide" ? "32%" : size === "medium" ? "49%" : "100%";
}

export function Dashboard(props: { store: AppStore }) {
  const samples: number[] = [];
  const stats = signal<TemperatureStats | null>(null);
  const history = signal<number[]>([]);
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
    history.set([...samples]);
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

  const tiles = (): JSX.Element => {
    const snap = props.store.snapshot();
    const report = summary();
    return (
      <View
        class="gap-3"
        style={{ flexDirection: "row", flexWrap: "wrap", alignItems: "stretch" }}
      >
        <View style={{ width: tileWidth() }}>
          <Stat
            label="Average duty"
            value={String(snap?.avg_percent ?? 0)}
            unit="%"
            hint={`across ${report?.present.length ?? 0} headers`}
            tone={(snap?.avg_percent ?? 0) > 85 ? "warn" : "ok"}
          />
        </View>
        <View style={{ width: tileWidth() }}>
          <Stat
            label="Total rpm"
            value={formatRpm(report?.totalRpm ?? 0)}
            unit="rpm"
            hint={`${report?.idle ?? 0} idle`}
          />
        </View>
        <View style={{ width: tileWidth() }}>
          <Stat
            label="Idle headers"
            value={String(report?.idle ?? 0)}
            hint={`${report?.spinning.length ?? 0} spinning`}
          />
        </View>
        <View style={{ width: tileWidth() }}>
          <Stat
            label="Channels lit"
            value={String(report?.lit.length ?? 0)}
            hint={`${report?.litLeds ?? 0} of ${report?.totalLeds ?? 0} LEDs`}
            tone={report?.lit.length ? "ok" : "muted"}
          />
        </View>
        <View style={{ width: tileWidth() }}>
          <Stat
            label="Curve driven"
            value={String(report?.curved ?? 0)}
            hint="headers on SmartFan"
          />
        </View>
        <View style={{ width: tileWidth() }}>
          <Stat
            label="LEDs lit"
            value={String(report?.litLeds ?? 0)}
            hint={`of ${report?.totalLeds ?? 0}`}
            tone={report?.litLeds ? "ok" : "muted"}
          />
        </View>
      </View>
    );
  };

  return (
    <View class="gap-4">
      <PageHeader
        title="Dashboard"
        actions={
          <If when={props.store.snapshot()}>
            {(snapshot) => <BoardBadge board={snapshot().board} />}
          </If>
        }
      />

      <If when={props.store.snapshot()} else={<Panel title="Waiting for the first reading" />}>
        {(snapshot) => (
          <View class="gap-4">
            <If when={snapshot().capabilities.notes.length > 0}>
              <View
                class="flex-row items-center gap-3 rounded-[10px] border border-border bg-card px-4 py-3"
                style={{ borderLeftWidth: 2, borderLeftColor: "#7dd3fc" }}
              >
                <Text class="text-base text-[#7dd3fc]">ⓘ</Text>
                <Text class="text-sm font-semibold text-foreground">Hardware notes</Text>
                <Text class="text-xs text-muted-foreground" style={{ flex: 1 }}>
                  {snapshot().capabilities.notes.join(" · ")}
                </Text>
                <Text class="text-[11px] text-muted-foreground">details</Text>
              </View>
            </If>

            <View
              class="gap-4"
              style={{
                flexDirection: layoutSize() === "narrow" ? "column" : "row",
                alignItems: "stretch",
              }}
            >
              <View style={{ flex: 1.35 }}>
                <Panel title="Motherboard temperature">
                  <If when={snapshot().fan_device.simulated}>
                    <View style={{ alignSelf: "flex-start" }}>
                      <Chip label="simulated" tone="warn" />
                    </View>
                  </If>
                  <View class="flex-row items-center gap-4" style={{ flexWrap: "wrap" }}>
                    <TemperatureGauge
                      temperature={() => snapshot().temperature_c}
                      unit={() => props.store.settings().temp_unit}
                    />
                    <View class="gap-1" style={{ flex: 1, minWidth: 180 }}>
                      <Text class="text-xs text-muted-foreground">
                        {snapshot().temperature_source ?? "unknown sensor"}
                        {snapshot().temperature_register
                          ? ` (${snapshot().temperature_register})`
                          : ""}
                      </Text>
                      <If when={stats()}>
                        {(current) => (
                          <Text class="text-[11px] font-mono text-muted-foreground">
                            {`session min ${tempText(current().min)} · mean ${tempText(current().mean)} · max ${tempText(current().max)}`}
                          </Text>
                        )}
                      </If>
                    </View>
                  </View>
                  <Sparkline samples={history} />
                  <View class="flex-row items-baseline justify-between">
                    <Text class="text-xs text-primary">{`now ${tempText(snapshot().temperature_c ?? 0)}`}</Text>
                    <Text class="text-[11px] text-muted-foreground">
                      {`${stats()?.count ?? 0} samples`}
                    </Text>
                    <Text class="text-[11px] text-muted-foreground">
                      {`range ${tempText(stats()?.min ?? 0)} – ${tempText(stats()?.max ?? 0)}`}
                    </Text>
                  </View>
                </Panel>
              </View>

              <View style={{ flex: 1, minWidth: 0 }}>{tiles()}</View>
            </View>

            <View
              class="gap-4"
              style={{
                flexDirection: layoutSize() === "narrow" ? "column" : "row",
                alignItems: "flex-start",
              }}
            >
              <View style={{ flex: 1, minWidth: 0 }}>
                <Panel
                  title="Fan headers"
                  actions={
                    <Text class="text-xs text-muted-foreground">Open fan control</Text>
                  }
                >
                  <View class="gap-2">
                    <Each of={summary()?.present ?? []} key={(fan) => fan.header}>
                      {(fan) => (
                        <View class="gap-1">
                          <View class="flex-row items-baseline justify-between gap-3">
                            <View class="flex-row items-baseline gap-1.5" style={{ flexShrink: 1 }}>
                              <Text class="text-xs text-foreground">{fan().label}</Text>
                              <Text class="text-[11px] text-muted-foreground">
                                {fanModeLabel(fan().mode, fan().curve_percent)}
                              </Text>
                            </View>
                            <View class="flex-row items-baseline gap-2">
                              <Text class="text-xs font-mono text-muted-foreground">
                                {`${formatRpm(fan().rpm)} rpm`}
                              </Text>
                              <Text class="text-xs font-mono text-foreground">{`${fan().percent}%`}</Text>
                            </View>
                          </View>
                          <Bar percent={fan().percent} color="#4ade80" thin />
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

              <View style={{ flex: 1, minWidth: 0 }}>
                <Panel title="Lighting" actions={<Text class="text-xs text-muted-foreground">Edit lighting</Text>}>
                  <View
                    style={{
                      padding: 4,
                      borderRadius: 4,
                      backgroundColor: "#0a0e11",
                      borderWidth: 1,
                      borderColor: "#1a2128",
                    }}
                  >
                    <LedStrip cells={expandFrame(snapshot().channels)} height={22} />
                  </View>
                  <Text class="text-xs text-muted-foreground">
                    {`${summary()?.lit.length ?? 0} of ${snapshot().channels.length} channels lit · ${summary()?.litLeds ?? 0} LEDs`}
                  </Text>
                  <View class="gap-0">
                    <Each of={snapshot().channels} key={(channel) => channel.id}>
                      {(channel) => (
                        <View
                          class="flex-row items-center gap-3 border-b border-border py-1"
                        >
                          <Text class="text-xs text-foreground" style={{ width: 70 }}>
                            {channel().label}
                          </Text>
                          <Text class="text-[11px] font-mono text-muted-foreground" style={{ width: 34 }}>
                            {String(channel().leds)}
                          </Text>
                          <View
                            class="h-2.5 w-2.5 rounded-[2px]"
                            style={{ backgroundColor: channel().render_hex }}
                          />
                          <Text class="text-[11px] font-mono text-muted-foreground" style={{ flex: 1 }}>
                            {channel().hex.toUpperCase()}
                          </Text>
                          <Text class="text-[11px] font-mono text-foreground">
                            {`${channel().brightness}%`}
                          </Text>
                        </View>
                      )}
                    </Each>
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

            <View class="flex-row gap-4" style={{ flexWrap: "wrap" }}>
              <KeyValueLine label="fan backend" value={deviceLabel(snapshot().fan_device)} />
              <KeyValueLine label="lighting backend" value={deviceLabel(snapshot().led_device)} />
            </View>
          </View>
        )}
      </If>
    </View>
  );
}

function KeyValueLine(props: { label: string; value: string }) {
  return (
    <View class="flex-row items-baseline gap-2">
      <Text class="text-[11px] text-muted-foreground">{`${props.label}:`}</Text>
      <Text class="text-[11px] font-mono text-foreground">{props.value}</Text>
    </View>
  );
}
