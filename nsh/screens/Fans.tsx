/** Fan control: live duty, the pump floor, profiles and the four-point curve. */

import { Text, View, computed, effect, signal } from "nsh";
import { Button, NumberInput, Slider } from "nsh/base";
import type { Accessor } from "nsh";

import type { AppStore } from "../app/composition/store";
import type {
  CurvePoint,
  FanHeaderId,
  FanProfileId,
  FanStatus,
  PawnioStatus,
} from "../app/domain/model";
import {
  DEFAULT_CURVE,
  MAX_TEMP_C,
  PROFILES,
  PUMP_FLOOR_PERCENT,
  curveSignature,
  dutyWarnings,
  isProfileActive,
  setCurvePoint,
} from "../app/domain/fan";
import { dutyColor, fanModeLabel, formatRpm } from "../app/presentation/format";
import { curvePointWidth, fanCardWidth } from "../app/presentation/layout";
import { confirmWrite } from "../components/confirm";
import { CurveChart } from "../components/CurveChart";
import { PawnioDialog } from "../components/PawnioDialog";
import { SetupsDialog } from "../components/SetupsDialog";
import { Bar, Chip, Notice, PageHeader, Panel } from "../components/ui";
import { TablerIcon } from "../components/icons";

type Tone = "ok" | "warn" | "danger" | "muted";

function curveLabel(points: readonly CurvePoint[]): string {
  return points.map((point) => `${point.temp_c} °C · ${point.percent}%`).join("   ");
}

function FanCard(props: {
  fan: Accessor<FanStatus>;
  temperature: Accessor<number | null>;
  onDuty: (header: FanHeaderId, percent: number) => Promise<boolean>;
  onCurve: (header: FanHeaderId, points: CurvePoint[]) => Promise<boolean>;
}) {
  const initial = props.fan().curve ?? DEFAULT_CURVE;
  const draft = signal<CurvePoint[]>(initial.map((point) => ({ ...point })));
  const dirty = signal(false);
  const editing = signal(false);
  const dutyDraft = signal(props.fan().percent);
  const dragging = signal(false);
  let lastSignature = curveSignature(props.fan().curve);

  effect(() => {
    const percent = props.fan().percent;
    if (!dragging()) dutyDraft.set(percent);
  });

  // A new curve from the board (a profile, another client) replaces an
  // untouched draft; a draft being edited is never overwritten by telemetry.
  effect(() => {
    const curve = props.fan().curve;
    const signature = curveSignature(curve);
    if (signature === lastSignature) return;
    lastSignature = signature;
    if (dirty()) return;
    draft.set(curve ? curve.map((point) => ({ ...point })) : DEFAULT_CURVE.map((point) => ({ ...point })));
    dirty.set(false);
  });

  const move = (index: number, patch: Partial<CurvePoint>): void => {
    draft.set(setCurvePoint(draft(), index, patch));
    dirty.set(true);
  };

  const revert = (): void => {
    const source = props.fan().curve ?? DEFAULT_CURVE;
    draft.set(source.map((point) => ({ ...point })));
    dirty.set(false);
  };

  const apply = async (): Promise<void> => {
    const applied = await props.onCurve(props.fan().header, draft());
    if (applied) {
      dirty.set(false);
      editing.set(false);
    }
  };

  return (
    <View
      class={
        props.fan().pump
          ? "rounded-[10px] border border-[#4ade80]/40 bg-card p-3 gap-2"
          : "rounded-[10px] border border-border bg-card p-3 gap-2"
      }
      style={{
        width: "100%",
        backgroundImage: "linear-gradient(180deg, rgba(255,255,255,0.025), transparent 50%)",
      }}
    >
      <View class="flex-row items-start justify-between gap-2" style={{ flexWrap: "wrap" }}>
        <View class="gap-0.5">
          <Text class="text-sm font-semibold text-foreground">{props.fan().label}</Text>
          <Text class="text-[11px] text-muted-foreground">{props.fan().description}</Text>
        </View>
        <View class="flex-row items-center gap-1" style={{ flexWrap: "wrap" }}>
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
        value={dutyDraft()}
        min={props.fan().pump ? PUMP_FLOOR_PERCENT : 0}
        max={100}
        step={1}
        accessibilityLabel={`${props.fan().label} duty`}
        trackClass="h-5 justify-center"
        indicatorClass="h-1 rounded-full bg-muted"
        rangeClass="h-1 rounded-full bg-primary"
        thumbClass="w-3.5 h-3.5 rounded-full bg-foreground"
        onChange={(event) => {
          const raw = Array.isArray(event.value) ? event.value[0] : event.value;
          const value = props.fan().pump ? Math.max(PUMP_FLOOR_PERCENT, raw) : raw;
          dragging.set(true);
          dutyDraft.set(value);
        }}
        onChangeEnd={(event) => {
          const value = Array.isArray(event.value) ? event.value[0] : event.value;
          dragging.set(false);
          void props.onDuty(props.fan().header, value).then((applied) => {
            if (!applied) dutyDraft.set(props.fan().percent);
          });
        }}
      >
        <Text class="text-[11px] text-muted-foreground">
          {props.fan().pump
            ? `Manual duty · floored at ${PUMP_FLOOR_PERCENT}%`
            : "Manual duty"}
        </Text>
      </Slider>

      <View class="flex-row items-center justify-between gap-2">
        <Text class="text-[11px] text-muted-foreground">
          SmartFan curve · {props.fan().curve ? curveLabel(props.fan().curve!) : "not set"}
        </Text>
        <View class="flex-row items-center gap-1">
          <If when={dirty()}>
            <Chip label="unsaved" tone="warn" />
          </If>
          <Button
            testID={`curve-edit-${props.fan().header}`}
            class="rounded-[6px] border border-border px-2 py-1"
            onPress={() => editing.set(!editing())}
          >
            <Text class="text-[11px] text-foreground">
              {editing() ? "Hide points" : "Edit points"}
            </Text>
          </Button>
        </View>
      </View>

      <CurveChart
        testID={`curve-${props.fan().header}`}
        curve={draft}
        temperature={props.temperature}
        onMove={move}
      />

      <If when={editing()}>
        <View class="flex-row gap-2" style={{ flexWrap: "wrap" }}>
          <Each of={draft()} key="index">
            {(point, index) => (
              <View class="gap-1" style={{ width: curvePointWidth() }}>
                <Text class="text-[10px] text-muted-foreground">{`P${index() + 1}`}</Text>
                <NumberInput
                  value={point().temp_c}
                  min={0}
                  max={MAX_TEMP_C}
                  step={1}
                  accessibilityLabel={`${props.fan().label} point ${index() + 1} temperature`}
                  inputClass="h-7 rounded-[6px] border border-border bg-muted px-1 text-center text-xs text-foreground"
                  suffix={<Text class="text-[10px] text-muted-foreground">°C</Text>}
                  onChange={({ value }) => move(index(), { temp_c: value ?? point().temp_c })}
                />
                <NumberInput
                  value={point().percent}
                  min={0}
                  max={100}
                  step={1}
                  accessibilityLabel={`${props.fan().label} point ${index() + 1} duty`}
                  inputClass="h-7 rounded-[6px] border border-border bg-muted px-1 text-center text-xs text-foreground"
                  suffix={<Text class="text-[10px] text-muted-foreground">%</Text>}
                  onChange={({ value }) => move(index(), { percent: value ?? point().percent })}
                />
              </View>
            )}
          </Each>
        </View>
      </If>

      <View class="flex-row items-center gap-2" style={{ flexWrap: "wrap" }}>
          <Button
            testID={`curve-apply-${props.fan().header}`}
            class="rounded-[6px] bg-primary px-3 py-1.5 disabled:opacity-40"
            disabled={!dirty()}
            onPress={() => void apply()}
          >
            <Text class="text-[11px] text-primary-foreground">Apply curve</Text>
          </Button>
          <Button
            testID={`curve-revert-${props.fan().header}`}
            class="rounded-[6px] border border-border px-3 py-1.5 disabled:opacity-40"
            disabled={!dirty()}
            onPress={revert}
          >
            <Text class="text-[11px] text-foreground">Revert</Text>
          </Button>
          <If when={props.fan().pump}>
            <Text class="text-[10px] text-[#fbbf24]">
              {`A pump curve must stay at or above ${PUMP_FLOOR_PERCENT}%`}
            </Text>
          </If>
      </View>
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

  const presetsOpen = signal(false);
  const pawnio = signal<PawnioStatus | null>(null);
  const pawnioOpen = signal(false);

  void props.store.pawnio
    .status()
    .then((status) => pawnio.set(status))
    .catch(() => {
      // The probe must not break the screen.
    });

  // The CPU header's curve follows the CPU die sensor; the rest follow the
  // board thermistor. This matches the core's curve evaluation exactly.
  const temperatureFor = (header: FanHeaderId): number | null => {
    const snap = props.store.snapshot();
    if (!snap) return null;
    return header === "cpu"
      ? (snap.cpu_temp_c ?? snap.board_temp_c ?? snap.temperature_c)
      : (snap.board_temp_c ?? snap.temperature_c);
  };

  const onDuty: (header: FanHeaderId, percent: number) => Promise<boolean> = async (
    header,
    percent,
  ) => {
    const fan = props.store.snapshot()?.fans.find((candidate) => candidate.header === header);
    const label = fan?.label ?? header;
    // The core floors a pump; confirm what will actually be written.
    const applied = fan?.pump ? Math.max(PUMP_FLOOR_PERCENT, percent) : percent;
    const floor = fan?.pump && applied !== percent ? ` (pump floor ${PUMP_FLOOR_PERCENT}%)` : "";
    if (!(await confirmWrite(props.store.settings(), `Set ${label} to ${applied}%${floor}?`))) {
      return false;
    }
    try {
      const message = await props.store.fans.setDuty(header, applied);
      props.toast(message, "ok");
      return true;
    } catch (error) {
      props.toast(`Set failed: ${String(error)}`, "danger");
      return false;
    }
  };

  const onCurve = async (header: FanHeaderId, points: CurvePoint[]): Promise<boolean> => {
    const fan = props.store.snapshot()?.fans.find((candidate) => candidate.header === header);
    if (
      !(await confirmWrite(
        props.store.settings(),
        `Set the ${fan?.label ?? header} curve to ${curveLabel(points)}?`,
      ))
    ) {
      return false;
    }
    try {
      const message = await props.store.fans.setCurve(header, points);
      props.toast(message, "ok");
      return true;
    } catch (error) {
      props.toast(`Curve rejected: ${String(error)}`, "danger");
      return false;
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
      <PageHeader
        title="Fan control"
        subtitle={
          summary().length === 0
            ? "No header is reporting a speed."
            : `${summary().length} of ${props.store.snapshot()?.fans.length ?? 0} headers in use`
        }
        actions={
        <Button
          testID="fan-presets"
          class="rounded-[7px] border border-border px-3 py-1.5"
          onPress={() => presetsOpen.set(true)}
        >
          <View class="flex-row items-center gap-1.5">
            <TablerIcon name="bookmark" size={16} color="#9fb0bd" />
            <Text class="text-xs text-foreground">Presets</Text>
          </View>
        </Button>
        }
      />

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

      <If
        when={
          // Simulation needs neither the driver nor elevation; the same policy
          // the startup elevation prompt follows.
          props.store.settings().backend !== "simulated" &&
          pawnio() !== null &&
          (pawnio()!.state === "not_installed" || pawnio()!.state === "denied")
        }
      >
        <Notice
          tone="warn"
          title={
            pawnio()?.state === "denied"
              ? "Fan readings need administrator rights"
              : "Fan readings are simulated"
          }
        >
          <View class="gap-2">
            <Text class="text-xs text-muted-foreground">
              {pawnio()?.state === "denied"
                ? "PawnIO is installed but this process cannot open it."
                : "PawnIO is not installed, so fan control reads synthetic values."}
            </Text>
            <Button
              testID="pawnio-open"
              class="rounded-[6px] bg-primary px-2.5 py-1"
              style={{ width: 220, alignSelf: "flex-start" }}
              onPress={() => pawnioOpen.set(true)}
            >
              <Text class="text-[11px] text-primary-foreground">
                {pawnio()?.state === "denied" ? "Restart as administrator" : "Install PawnIO"}
              </Text>
            </Button>
          </View>
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
          Profiles replace every header's curve. Drag a point to make your own, or use
          the point fields for exact values.
        </Text>
      </Panel>

      <View class="gap-3">
        <View class="flex-row gap-3" style={{ flexWrap: "wrap" }}>
          <Each of={summary()} key={(fan) => fan.header}>
            {(fan) => (
              <View style={{ width: fanCardWidth() }}>
                <FanCard
                  fan={fan}
                  temperature={() => temperatureFor(fan().header)}
                  onDuty={onDuty}
                  onCurve={onCurve}
                />
              </View>
            )}
          </Each>
        </View>
        <If when={summary().length === 0}>
          <Panel title="No fans detected">
            <Text class="text-xs text-muted-foreground">
              Nothing is reporting a tachometer. With the driver installed this board
              normally exposes five headers; check that the fans are seated.
            </Text>
          </Panel>
        </If>
      </View>

      <SetupsDialog
        kind="fans"
        open={presetsOpen}
        onOpenChange={presetsOpen.set}
        store={props.store}
        toast={props.toast}
      />

      <If when={pawnio()}>
        {(status) => (
          <PawnioDialog
            status={status()}
            open={pawnioOpen}
            onOpenChange={pawnioOpen.set}
            onStatus={pawnio.set}
            store={props.store}
            toast={props.toast}
          />
        )}
      </If>
    </View>
  );
}

