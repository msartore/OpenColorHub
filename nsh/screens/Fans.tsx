/** Fan control: live duty, the pump floor, profiles and the write guard. */

import { Text, View, computed, effect, signal } from "nsh";
import { AlertDialog, Button, Slider } from "nsh/base";
import type { Accessor } from "nsh";

import type { AppStore } from "../app/composition/store";
import type { FanHeaderId, FanProfileId, FanStatus, Settings } from "../app/domain/model";
import { PROFILES, PUMP_FLOOR_PERCENT, dutyWarnings, isProfileActive } from "../app/domain/fan";
import { dutyColor, fanModeLabel, formatRpm } from "../app/presentation/format";
import { Bar, Chip, Notice, Panel } from "../components/ui";

type Tone = "ok" | "warn" | "danger" | "muted";

async function confirmWrite(settings: Settings, message: string): Promise<boolean> {
  if (!settings.confirm_writes) return true;
  const answer = await AlertDialog.show({
    title: "Confirm hardware write",
    description: message,
    actions: [
      { id: "cancel", label: "Cancel", role: "cancel" },
      { id: "write", label: "Write", role: "destructive" },
    ],
  });
  return answer.action === "write";
}

function FanCard(props: {
  fan: Accessor<FanStatus>;
  onDuty: (header: FanHeaderId, percent: number) => Promise<void>;
}) {
  const draft = signal(props.fan().percent);
  const dragging = signal(false);

  effect(() => {
    const percent = props.fan().percent;
    if (!dragging()) draft.set(percent);
  });

  const commit = (value: number): void => {
    dragging.set(false);
    void props.onDuty(props.fan().header, value);
  };

  return (
    <View
      class={
        props.fan().pump
          ? "rounded-[10px] border border-[#4ade80]/40 bg-card p-3 gap-2"
          : "rounded-[10px] border border-border bg-card p-3 gap-2"
      }
    >
      <View class="flex-row items-start justify-between gap-2">
        <View class="gap-0.5">
          <Text class="text-sm font-semibold text-foreground">{props.fan().label}</Text>
          <Text class="text-[11px] text-muted-foreground">{props.fan().description}</Text>
        </View>
        <View class="flex-row items-center gap-1">
          <If when={props.fan().stalled}>
            <Chip label="stalled" tone="danger" />
          </If>
          <If when={props.fan().pump}>
            <Chip label={`pump · min ${PUMP_FLOOR_PERCENT}%`} tone="ok" />
          </If>
          <Chip label={fanModeLabel(props.fan().mode, props.fan().curve_percent)} tone="muted" />
        </View>
      </View>

      <View class="flex-row items-baseline justify-between gap-2">
        <View class="flex-row items-baseline gap-1">
          <Text class="text-2xl font-semibold text-foreground">{formatRpm(props.fan().rpm)}</Text>
          <Text class="text-[11px] text-muted-foreground">rpm</Text>
        </View>
        <Text class="text-sm font-mono text-foreground">{`${props.fan().percent}%`}</Text>
      </View>

      <Bar percent={props.fan().percent} color={dutyColor(props.fan().percent)} />

      <Slider
        value={draft()}
        min={0}
        max={100}
        step={1}
        accessibilityLabel={`${props.fan().label} duty`}
        trackClass="h-5 justify-center"
        indicatorClass="h-1 rounded-full bg-muted"
        rangeClass="h-1 rounded-full bg-primary"
        thumbClass="w-3.5 h-3.5 rounded-full bg-foreground"
        onChange={(event) => {
          const value = Array.isArray(event.value) ? event.value[0] : event.value;
          dragging.set(true);
          draft.set(value);
        }}
        onChangeEnd={(event) => {
          const value = Array.isArray(event.value) ? event.value[0] : event.value;
          commit(value);
        }}
      >
        <Text class="text-[11px] text-muted-foreground">
          {props.fan().pump
            ? `Manual duty · floored at ${PUMP_FLOOR_PERCENT}%`
            : "Manual duty"}
        </Text>
      </Slider>
    </View>
  );
}

export function Fans(props: {
  store: AppStore;
  toast: (message: string, tone?: Tone) => void;
}) {
  const summary = computed(() => {
    const snapshot = props.store.snapshot();
    return snapshot ? snapshot.fans.filter((fan) => fan.detected) : [];
  });

  const warnings = computed(() =>
    dutyWarnings(summary(), props.store.settings().low_duty_warning),
  );

  const onDuty = async (header: FanHeaderId, percent: number): Promise<void> => {
    const fan = props.store.snapshot()?.fans.find((candidate) => candidate.header === header);
    const label = fan?.label ?? header;
    if (!(await confirmWrite(props.store.settings(), `Set ${label} to ${percent}%?`))) return;
    try {
      const message = await props.store.fans.setDuty(header, percent);
      props.toast(message, "ok");
    } catch (error) {
      props.toast(`Set failed: ${String(error)}`, "danger");
    }
  };

  const onProfile = async (profile: FanProfileId): Promise<void> => {
    const label = PROFILES.find((candidate) => candidate.id === profile)?.label ?? profile;
    if (
      !(await confirmWrite(
        props.store.settings(),
        `Apply the ${label} profile to every header?`,
      ))
    ) {
      return;
    }
    try {
      const message = await props.store.fans.applyProfile(profile);
      props.toast(message, "ok");
    } catch (error) {
      props.toast(`Profile failed: ${String(error)}`, "danger");
    }
  };

  return (
    <View class="gap-4">
      <View class="gap-1">
        <Text class="text-xl font-semibold text-foreground">Fan control</Text>
        <Text class="text-xs text-muted-foreground">
          {summary().length === 0
            ? "No header is reporting a speed."
            : `${summary().length} of ${props.store.snapshot()?.fans.length ?? 0} headers in use`}
        </Text>
      </View>

      <If when={warnings().stalled.length > 0}>
        <Notice tone="danger" title="A fan is driven but not rotating">
          <Text class="text-xs text-muted-foreground">
            {warnings()
              .stalled.map((fan) => `${fan.label} (${fan.percent}%)`)
              .join(", ")}
            {" — check the connection or lower the duty that stalled it."}
          </Text>
        </Notice>
      </If>

      <If when={warnings().lowDuty.length > 0}>
        <Notice tone="warn" title={`Below ${props.store.settings().low_duty_warning}% duty`}>
          <Text class="text-xs text-muted-foreground">
            {warnings()
              .lowDuty.map((fan) => fan.label)
              .join(", ")}
            {" — verify the fans keep spinning under load."}
          </Text>
        </Notice>
      </If>

      <Panel title="Profiles" subtitle="Applied to every header the board exposes">
        <View class="flex-row gap-2" style={{ flexWrap: "wrap" }}>
          <Each of={PROFILES} key={(profile) => profile.id}>
            {(profile) => (
              <Button
                class={
                  isProfileActive(summary(), profile().id)
                    ? "rounded-[7px] bg-primary px-3 py-1.5"
                    : "rounded-[7px] border border-border px-3 py-1.5"
                }
                onPress={() => void onProfile(profile().id)}
              >
                <Text
                  class={
                    isProfileActive(summary(), profile().id)
                      ? "text-xs text-primary-foreground"
                      : "text-xs text-foreground"
                  }
                >
                  {profile().label}
                </Text>
              </Button>
            )}
          </Each>
        </View>
        <Text class="text-[11px] text-muted-foreground">
          Per-header curve editing (the draggable four-point editor) is the next
          slice. Manual duty, the pump floor and profiles are live.
        </Text>
      </Panel>

      <View class="gap-3">
        <Each of={summary()} key={(fan) => fan.header}>
          {(fan) => (
            <FanCard fan={fan} onDuty={onDuty} />
          )}
        </Each>
        <If when={summary().length === 0}>
          <Panel title="No fans detected">
            <Text class="text-xs text-muted-foreground">
              Nothing is reporting a tachometer. With the driver installed this board
              normally exposes five headers; check that the fans are seated.
            </Text>
          </Panel>
        </If>
      </View>
    </View>
  );
}
