/** Settings: backend, polling, display, safety, startup, privileges and paths. */

import { Application } from "nsh/desktop";
import { Clipboard, Scroll, Text, View, effect, signal } from "nsh";
import { AlertDialog, Button, Progress, Slider, TextView } from "nsh/base";
import {
  checkForUpdate,
  downloadUpdate,
  installAndRestart,
  shellInfo,
  type AppUpdate,
} from "nsh/updater";

import type { AppStore } from "../app/composition/store";
import type {
  AboutInfo,
  AutostartState,
  BackendMode,
  ElevationState,
  Settings,
  SettingsPatch,
  TempUnit,
} from "../app/domain/model";
import { FAN_HEADERS } from "../app/domain/fan";
import { boardDisplayName, deviceLabel } from "../app/domain/telemetry";
import { SETTINGS_RESET_PATCH } from "../app/domain/settings";
import { isNarrow } from "../app/presentation/layout";
import { KeyValue, PageHeader, Panel, Toggle } from "../components/ui";

type Tone = "ok" | "warn" | "danger" | "muted";

const BACKENDS: { id: BackendMode; label: string; blurb: string }[] = [
  {
    id: "auto",
    label: "Auto detect",
    blurb: "Probe the Super I/O and HID controller, fall back to the simulator with a warning.",
  },
  {
    id: "hardware",
    label: "Hardware only",
    blurb: "Require real hardware; the UI shows an error banner instead of pretending.",
  },
  {
    id: "simulated",
    label: "Simulated",
    blurb: "Never touch hardware — safe for demos, screenshots and development.",
  },
];

const ACCENTS: readonly string[] = [
  "#4ade80",
  "#22d3ee",
  "#e879f9",
  "#fbbf24",
  "#f87171",
  "#a3e635",
];

const UNITS: { id: TempUnit; label: string }[] = [
  { id: "celsius", label: "Celsius" },
  { id: "fahrenheit", label: "Fahrenheit" },
];

function Choice(props: {
  label: string;
  selected: boolean;
  onPress: () => void;
}) {
  return (
    <Button
      class={
        props.selected
          ? "rounded-[7px] bg-primary px-3 py-1.5"
          : "rounded-[7px] border border-border px-3 py-1.5"
      }
      selected={props.selected}
      onPress={props.onPress}
    >
      <Text class={`text-xs ${props.selected ? "text-primary-foreground" : "text-foreground"}`}>
        {props.label}
      </Text>
    </Button>
  );
}

function PathRow(props: { label: string; value: string; onCopy: (value: string) => void }) {
  return (
    <View class="flex-row items-center justify-between gap-3">
      <Text class="text-xs text-muted-foreground">{props.label}</Text>
      <View class="flex-row items-center gap-2" style={{ flexShrink: 1 }}>
        <Text class="text-[11px] font-mono text-muted-foreground" style={{ flexShrink: 1 }}>
          {props.value}
        </Text>
        <Button
          class="rounded-[5px] border border-border px-2 py-0.5"
          onPress={() => props.onCopy(props.value)}
        >
          <Text class="text-[10px] text-foreground">copy</Text>
        </Button>
      </View>
    </View>
  );
}

export function SettingsScreen(props: {
  store: AppStore;
  toast: (message: string, tone?: Tone) => void;
}) {
  const draft = signal<Settings>({ ...props.store.settings() });
  const poll = signal(props.store.settings().poll_interval_ms);
  const lowDuty = signal(props.store.settings().low_duty_warning);
  const elevation = signal<ElevationState | null>(null);
  const autostart = signal<AutostartState | null>(null);
  const about = signal<AboutInfo | null>(null);
  const update = signal<AppUpdate | null>(null);
  const checking = signal(false);
  const installing = signal(false);
  const updateLine = signal("");
  const updateProgress = signal<{ received: number; total: number | null } | null>(null);
  const shell = shellInfo();

  effect(() => {
    const next = props.store.settings();
    draft.set(next);
    poll.set(next.poll_interval_ms);
    lowDuty.set(next.low_duty_warning);
  });

  const loadSystem = async (): Promise<void> => {
    try {
      elevation.set(await props.store.system.elevation());
    } catch {
      elevation.set(null);
    }
    try {
      autostart.set(await props.store.system.autostart());
    } catch {
      autostart.set(null);
    }
    try {
      about.set(await props.store.system.about());
    } catch {
      about.set(null);
    }
  };
  void loadSystem();

  const save = async (patch: SettingsPatch): Promise<void> => {
    try {
      await props.store.saveSettings(patch);
    } catch (error) {
      props.toast(`Could not save: ${String(error)}`, "danger");
    }
  };

  const changeBackend = async (backend: BackendMode): Promise<void> => {
    await save({ backend });
    try {
      await props.store.refresh();
      props.toast(`Backend set to ${backend}`, "ok");
    } catch (error) {
      props.toast(`Backend changed, refresh failed: ${String(error)}`, "warn");
    }
  };

  const copy = (value: string): void => {
    void Clipboard.writeText(value)
      .then(() => props.toast("Copied to clipboard", "ok"))
      .catch(() => props.toast("Clipboard unavailable", "warn"));
  };

  const reset = async (): Promise<void> => {
    const answer = await AlertDialog.show({
      title: "Reset settings?",
      description: "Preferences go back to their defaults. Fan curves and colours are left alone.",
      actions: [
        { id: "cancel", label: "Cancel", role: "cancel" },
        { id: "reset", label: "Reset", role: "destructive" },
      ],
    });
    if (answer.action !== "reset") return;
    await save(SETTINGS_RESET_PATCH);
    await props.store.refresh();
    props.toast("Settings reset", "ok");
  };

  const reload = async (): Promise<void> => {
    try {
      await props.store.refresh();
      props.toast("Snapshot reloaded", "ok");
    } catch (error) {
      props.toast(`Reload failed: ${String(error)}`, "danger");
    }
  };

  const elevate = async (): Promise<void> => {
    try {
      const outcome = await props.store.system.requestElevation();
      if (outcome === "launched") {
        Application.quit();
      } else {
        props.toast("Elevation cancelled — still running as a standard user", "warn");
      }
    } catch (error) {
      props.toast(String(error), "danger");
    }
  };

  const toggleAutostart = async (enabled: boolean): Promise<void> => {
    try {
      autostart.set(await props.store.system.setAutostart(enabled));
      props.toast(
        enabled
          ? "Saved lighting will be applied at sign-in"
          : "No longer starting with the OS",
        enabled ? "ok" : "muted",
      );
    } catch (error) {
      props.toast(String(error), "danger");
    }
  };

  const check = async (): Promise<void> => {
    checking.set(true);
    updateLine.set("");
    try {
      const found = await checkForUpdate();
      update.set(found);
      if (found) {
        updateLine.set(`Version ${found.version} is available.`);
      } else if (!shell.version) {
        updateLine.set("Update checks need the packaged build.");
      } else {
        updateLine.set(`Up to date — ${shell.version} is the newest release.`);
      }
    } catch (error) {
      updateLine.set(`Check failed: ${String(error)}`);
    } finally {
      checking.set(false);
    }
  };

  const install = async (): Promise<void> => {
    installing.set(true);
    updateProgress.set(null);
    updateLine.set("Downloading the update…");
    try {
      await downloadUpdate((frame) => {
        updateProgress.set({ received: frame.received, total: frame.total ?? null });
        updateLine.set(
          frame.total
            ? `Downloading… ${Math.min(100, Math.round((frame.received / frame.total) * 100))}%`
            : `Downloading… ${frame.received} bytes`,
        );
      });
      updateLine.set("Restarting…");
      await installAndRestart();
    } catch (error) {
      updateLine.set(`Update failed: ${String(error)}`);
    } finally {
      installing.set(false);
    }
  };

  return (
    <View class="gap-4">
      <PageHeader title="Settings" />

      <View
        class="gap-4"
        style={{
          flexDirection: isNarrow() ? "column" : "row",
          alignItems: "flex-start",
        }}
      >
        <View
          class="gap-4"
          style={{
            flex: isNarrow() ? undefined : 1,
            width: isNarrow() ? "100%" : undefined,
          }}
        >
          <Panel title="Updates" subtitle="GitHub releases, verified before they replace the app">
        <View class="gap-1">
          <KeyValue label="version" value={shell.version ?? "development"} />
          <KeyValue label="NSH runtime" value={shell.nshVersion} />
        </View>
        <If when={updateLine()}>
          {(line) => <Text class="text-[11px] text-muted-foreground">{line()}</Text>}
        </If>
        <If when={updateProgress()}>
          {(frame) => (
            <View class="gap-1">
              <Progress
                value={
                  frame().total
                    ? Math.min(100, Math.round((frame().received / frame().total!) * 100))
                    : 0
                }
                indeterminate={!frame().total}
                class="h-1"
                trackClass="h-1 rounded-full bg-muted"
                indicatorClass="h-1 rounded-full bg-primary"
              />
            </View>
          )}
        </If>
        <If when={update()}>
          {(found) => (
            <View class="gap-2">
              <Text class="text-xs text-foreground">{`OpenColorHub ${found().version} is available.`}</Text>
              <If when={found().notes}>
                <Scroll style={{ maxHeight: 190 }}>
                  <View class="rounded-[7px] border border-border bg-muted/30 p-3">
                    <TextView markdown={found().notes?.slice(0, 8000) ?? ""} />
                  </View>
                </Scroll>
              </If>
              <Button
                testID="updates-install"
                class="rounded-[7px] bg-primary px-3 py-1.5 disabled:opacity-40"
                disabled={installing()}
                onPress={() => void install()}
              >
                <Text class="text-xs text-primary-foreground">
                  {installing() ? "Installing…" : "Download & install"}
                </Text>
              </Button>
            </View>
          )}
        </If>
        <View class="flex-row gap-2" style={{ flexWrap: "wrap" }}>
          <Button
            testID="updates-check"
            class="rounded-[7px] border border-border px-3 py-1.5 disabled:opacity-40"
            disabled={checking()}
            onPress={() => void check()}
          >
            <Text class="text-xs text-foreground">{checking() ? "Checking…" : "Check now"}</Text>
          </Button>
        </View>
      </Panel>

      <Panel title="Backend" subtitle="Where readings come from">
        <View class="flex-row gap-2" style={{ flexWrap: "wrap" }}>
          <Each of={BACKENDS} key={(backend) => backend.id}>
            {(backend) => (
              <Choice
                label={backend().label}
                selected={draft().backend === backend().id}
                onPress={() => void changeBackend(backend().id)}
              />
            )}
          </Each>
        </View>
        <Text class="text-[11px] text-muted-foreground">
          {BACKENDS.find((backend) => backend.id === draft().backend)?.blurb}
        </Text>
        <If when={props.store.snapshot()}>
          {(snapshot) => (
            <View class="gap-1 border-t border-border pt-2">
              <KeyValue label="board" value={boardDisplayName(snapshot().board)} />
              <KeyValue label="support" value={snapshot().board.confidence} />
              <KeyValue label="fan backend" value={deviceLabel(snapshot().fan_device)} />
              <KeyValue label="lighting backend" value={deviceLabel(snapshot().led_device)} />
              <KeyValue
                label="board temp"
                value={
                  snapshot().board_temp_c === null || snapshot().board_temp_c === undefined
                    ? "—"
                    : `${snapshot().board_temp_c!.toFixed(1)} °C`
                }
              />
              <KeyValue
                label="cpu temp"
                value={
                  snapshot().cpu_temp_c === null || snapshot().cpu_temp_c === undefined
                    ? "—"
                    : `${snapshot().cpu_temp_c!.toFixed(1)} °C`
                }
              />
            </View>
          )}
        </If>
      </Panel>

      <Panel title="Telemetry" subtitle="How often the controller is polled">
        <Slider
          value={poll()}
          min={250}
          max={10_000}
          step={250}
          accessibilityLabel="Telemetry interval"
          trackClass="h-5 justify-center"
          indicatorClass="h-1 rounded-full bg-muted"
          rangeClass="h-1 rounded-full bg-primary"
          thumbClass="w-3.5 h-3.5 rounded-full bg-foreground"
          onChange={(event) => {
            const value = Array.isArray(event.value) ? event.value[0] : event.value;
            poll.set(value);
          }}
          onChangeEnd={(event) => {
            const value = Array.isArray(event.value) ? event.value[0] : event.value;
            poll.set(value);
            void save({ poll_interval_ms: value });
          }}
        >
          <Text class="text-[11px] text-muted-foreground">{`Every ${poll()} ms`}</Text>
        </Slider>
        <Toggle
          checked={draft().smoothing}
          testID="settings-smoothing"
          label="Smooth readings"
          hint="Exponential moving average on temperature and RPM"
          onChange={(checked) => void save({ smoothing: checked })}
        />
      </Panel>

      <Panel title="Display" subtitle="Units and accent colour">
        <View class="gap-2">
          <Text class="text-xs text-muted-foreground">Temperature unit</Text>
          <View class="flex-row gap-2" style={{ flexWrap: "wrap" }}>
            <Each of={UNITS} key={(unit) => unit.id}>
              {(unit) => (
                <Choice
                  label={unit().label}
                  selected={draft().temp_unit === unit().id}
                  onPress={() => void save({ temp_unit: unit().id })}
                />
              )}
            </Each>
          </View>
        </View>

        <View class="gap-2">
          <Text class="text-xs text-muted-foreground">
            Accent colour — applied to buttons, selection, the gauge and the chart
            as soon as you pick it.
          </Text>
          <View class="flex-row gap-2" style={{ flexWrap: "wrap" }}>
            <Each of={ACCENTS} key="index">
              {(hex) => (
                <Button
                  class={
                    draft().accent === hex()
                      ? "h-7 w-7 rounded-[6px] border-2 border-foreground"
                      : "h-7 w-7 rounded-[6px] border border-border"
                  }
                  style={{ backgroundColor: hex() }}
                  accessibilityLabel={hex()}
                  testID={`accent-${hex()}`}
                  onPress={() => void save({ accent: hex() })}
                />
              )}
            </Each>
          </View>
        </View>
      </Panel>

        </View>
        <View
          class="gap-4"
          style={{
            flex: isNarrow() ? undefined : 1,
            width: isNarrow() ? "100%" : undefined,
          }}
        >
      <Panel title="Safety" subtitle="Guard rails for real hardware">
        <Toggle
          checked={draft().confirm_writes}
          testID="settings-confirm"
          label="Confirm hardware writes"
          hint="Ask before every fan or lighting change"
          onChange={(checked) => void save({ confirm_writes: checked })}
        />

        <Toggle
          checked={draft().gpu_led_enabled}
          testID="settings-gpu-led"
          label="GPU LED control"
          hint="Drive GPU RGB at I²C 0x36 through AMD ADL, NVIDIA NVAPI or Intel IGCL"
          onChange={(checked) => void save({ gpu_led_enabled: checked })}
        />

        <Toggle
          checked={draft().restore_setup_on_startup}
          testID="settings-restore-setup"
          label="Restore saved setup at startup"
          hint="Re-apply the last fan and lighting settings on every launch"
          onChange={(checked) => void save({ restore_setup_on_startup: checked })}
        />

        <Toggle
          checked={draft().release_fans_on_exit}
          testID="settings-release-fans"
          label="Release fans on exit"
          hint="Hand control back to the BIOS instead of keeping your setup"
          onChange={(checked) => void save({ release_fans_on_exit: checked })}
        />

        <Toggle
          checked={draft().reapply_led_on_launch}
          label="Re-apply lighting at launch"
          hint="Fallback for installs with no saved setup yet"
          onChange={(checked) => void save({ reapply_led_on_launch: checked })}
        />

        <Slider
          value={lowDuty()}
          min={0}
          max={60}
          step={5}
          accessibilityLabel="Warn below this duty"
          trackClass="h-5 justify-center"
          indicatorClass="h-1 rounded-full bg-muted"
          rangeClass="h-1 rounded-full bg-primary"
          thumbClass="w-3.5 h-3.5 rounded-full bg-foreground"
          onChange={(event) => {
            const value = Array.isArray(event.value) ? event.value[0] : event.value;
            lowDuty.set(value);
          }}
          onChangeEnd={(event) => {
            const value = Array.isArray(event.value) ? event.value[0] : event.value;
            lowDuty.set(value);
            void save({ low_duty_warning: value });
          }}
        >
          <Text class="text-[11px] text-muted-foreground">
            {`Warn below ${lowDuty()}% duty`}
          </Text>
        </Slider>

        <View class="gap-2">
          <Text class="text-xs text-muted-foreground">
            Pump channel — floored at 60%, so a stopped pump cannot be commanded
          </Text>
          <View class="flex-row gap-1.5" style={{ flexWrap: "wrap" }}>
            <Choice
              label="None"
              selected={draft().pump_header === null}
              onPress={() => void save({ pump_header: "none" })}
            />
            <Each of={FAN_HEADERS} key={(header) => header.id}>
              {(header) => (
                <Choice
                  label={header().label}
                  selected={draft().pump_header === header().id}
                  onPress={() => void save({ pump_header: header().id })}
                />
              )}
            </Each>
          </View>
        </View>
      </Panel>

      <Panel title="Startup" subtitle="Start with the OS and apply the saved light">
        <Toggle
          checked={autostart()?.enabled ?? false}
          disabled={autostart()?.supported === false}
          testID="settings-autostart"
          label={
            autostart()?.supported === false
              ? "Start with the OS — not supported on this platform"
              : "Start with the OS at sign-in"
          }
          hint="Launch when you sign in, then apply the saved lighting"
          onChange={(checked) => void toggleAutostart(checked)}
        />
        <Text class="text-[11px] text-muted-foreground">
          {autostart()?.detail ?? "Reading the logon entry…"}
        </Text>
      </Panel>

      <Panel title="Privileges" subtitle="Fan control needs access to the driver">
        <Text class="text-xs text-muted-foreground">
          {elevation() === null
            ? "Reading the elevation state…"
            : elevation()!.elevated
              ? "Running elevated — the driver is reachable."
              : elevation()!.detail}
        </Text>
        <If when={elevation() !== null && !elevation()!.elevated && elevation()!.can_request}>
          <View class="flex-row gap-2" style={{ flexWrap: "wrap" }}>
            <Button
              class="rounded-[7px] bg-primary px-3 py-1.5"
              style={{ width: 220, alignSelf: "flex-start" }}
              onPress={() => void elevate()}
            >
              <Text class="text-xs text-primary-foreground">Restart as administrator</Text>
            </Button>
          </View>
        </If>
      </Panel>

      <Panel title="Files & paths" subtitle="Where state and settings live">
        <View class="gap-1.5">
          <PathRow label="data dir" value={about()?.paths.data_dir ?? "—"} onCopy={copy} />
          <PathRow label="settings" value={about()?.paths.settings ?? "—"} onCopy={copy} />
          <PathRow label="lighting state" value={about()?.paths.state ?? "—"} onCopy={copy} />
        </View>
      </Panel>

      <Panel title="Maintenance" subtitle="Recovery actions">
        <View class="flex-row gap-2" style={{ flexWrap: "wrap" }}>
          <Button
            class="rounded-[7px] border border-border px-3 py-1.5"
            onPress={() => void reload()}
          >
            <Text class="text-xs text-foreground">Reload state file</Text>
          </Button>
          <Button
            class="rounded-[7px] border border-[#f87171]/50 px-3 py-1.5"
            onPress={() => void reset()}
          >
            <Text class="text-xs text-[#f87171]">Reset to defaults</Text>
          </Button>
        </View>
        <Text class="text-[11px] text-muted-foreground">
          Resetting only touches preferences — fan curves and colours are left alone.
        </Text>
      </Panel>
        </View>
      </View>
    </View>
  );
}

