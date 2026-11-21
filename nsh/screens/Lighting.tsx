/** Lighting: grouped device sections, per-channel colour, GPU zones and sync. */

import { Canvas, Text, View, computed, effect, signal } from "nsh";
import type { Accessor, JSX } from "nsh";
import {
  Button,
  ColorArea,
  ColorField,
  ColorPicker,
  ColorSlider,
  Collapsible,
  Dialog,
  Select,
  Slider,
  Switch,
} from "nsh/base";

import type { AppStore } from "../app/composition/store";
import type {
  ChannelId,
  ChannelStatus,
  GpuDeviceStatus,
  GpuZoneState,
  GpuZoneStatus,
} from "../app/domain/model";
import { LED_TOTAL, channelSignature, expandFrame, hexToRgb } from "../app/domain/lighting";
import {
  GPU_DEFAULT_MODE,
  GPU_DEFAULT_SPEED,
  GPU_MODES,
  gpuBrightnessToPercent,
  gpuPercentToBrightness,
} from "../app/domain/gpu";
import { confirmWrite } from "../components/confirm";
import { TablerIcon } from "../components/icons";
import { SetupsDialog } from "../components/SetupsDialog";
import {
  centeredDialog,
  Chip,
  LedStrip,
  PageHeader,
  Panel,
  SWITCH_THUMB,
  SWITCH_TRACK,
} from "../components/ui";
import { channelWidth, dialogWidth } from "../app/presentation/layout";

type Tone = "ok" | "warn" | "danger" | "muted";

const EFFECT_ITEMS = GPU_MODES.map(([value, title]) => ({ value: String(value), title }));

function ColorEditor(props: {
  value: Accessor<string>;
  onChange: (hex: string) => void;
}) {
  // Let the picker own its color while it is open (starting from the current
  // value) and commit only when the gesture ends. Feeding every intermediate
  // `onChange` back into a controlled value fights its internal colour-space
  // state and snaps the selection back to the previous colour.
  return (
    <ColorPicker
      defaultValue={props.value()}
      format="hex"
      onChangeEnd={({ value, formatted }) => props.onChange(formatted || value)}
    >
      <View class="gap-2" style={{ width: 220 }}>
        <ColorArea testID="colour-area" class="h-24 w-full rounded-[8px]" />
        <ColorSlider
          channel="hue"
          class="h-5"
          trackClass="h-3 rounded-full"
          thumbClass="w-4 h-4 rounded-full border-2 border-white"
        />
        <ColorField
          testID="colour-field"
          class="h-8 rounded-[6px] border border-border bg-muted px-2 text-xs text-foreground"
        />
      </View>
    </ColorPicker>
  );
}

/** The only colour control: the swatch you press to open the picker dialog. */
function SwatchButton(props: {
  value: Accessor<string>;
  onChange: (hex: string) => void;
  label?: string;
  width?: number;
  height?: number;
  testID?: string;
}) {
  const open = signal(false);
  return (
    <>
      <Button
        testID={props.testID}
        class="rounded-[5px] border border-border"
        style={{
          width: props.width ?? 38,
          height: props.height ?? 30,
          backgroundColor: props.value(),
        }}
        accessibilityLabel={props.label ?? "Change colour"}
        onPress={() => open.set(true)}
      />
      <Dialog
        open={open()}
        onOpenChange={({ open: next }) => open.set(next)}
        accessibilityLabel="Choose a colour"
        class="rounded-[12px] border border-border bg-popover p-4 gap-3"
        style={centeredDialog(dialogWidth(280))}
        backdropClass="bg-black/40"
      >
        <Text class="text-sm font-semibold text-foreground">Choose a colour</Text>
        <ColorEditor value={props.value} onChange={props.onChange} />
        <View class="flex-row justify-end">
          <Button
            testID="colour-done"
            class="rounded-[6px] border border-border px-3 py-1.5"
            onPress={() => open.set(false)}
          >
            <Text class="text-xs text-foreground">Done</Text>
          </Button>
        </View>
      </Dialog>
    </>
  );
}

/** The collapsible Motherboard / GPU card that heads each device section. */
function DeviceGroup(props: {
  title: string;
  subtitle: string;
  dot: string;
  color: Accessor<string>;
  onColor: (hex: string) => void;
  onSync: () => void;
  onApply: () => void;
  children?: JSX.Element;
}) {
  const open = signal(false);
  return (
    <Collapsible
      open={open()}
      class="rounded-[12px] border border-border bg-card"
      style={{ overflow: "hidden" }}
      content={props.children}
    >
      <View
        class="flex-row items-center gap-3 px-4 py-3"
        style={{ borderBottomWidth: open() ? 1 : 0, borderBottomColor: "#1a2128" }}
      >
        <View
          class="rounded-full"
          style={{
            width: 26,
            height: 26,
            backgroundColor: props.dot,
            boxShadow: `0 0 18px ${props.dot}`,
          }}
        />
        <View class="gap-0.5" style={{ flex: 1, flexShrink: 1, minWidth: 120 }}>
          <Text class="text-base font-semibold text-foreground">{props.title}</Text>
          <Text class="text-xs text-muted-foreground">{props.subtitle}</Text>
        </View>
        <SwatchButton value={props.color} onChange={props.onColor} />
        <Button
          class="rounded-[6px] border border-border px-3 py-1.5"
          onPress={props.onSync}
        >
          <Text class="text-xs text-foreground">Sync</Text>
        </Button>
        <Button
          class="rounded-[6px] bg-primary px-3 py-1.5"
          onPress={props.onApply}
        >
          <Text class="text-xs text-primary-foreground">Apply</Text>
        </Button>
        <Button
          testID={`${props.title.toLowerCase()}-details`}
          class="rounded-[6px] border border-border px-2 py-1"
          onPress={() => open.set(!open())}
        >
          <Text class="text-[11px] text-muted-foreground">{open() ? "Hide" : "Details"}</Text>
        </Button>
      </View>
    </Collapsible>
  );
}

function ChannelCard(props: {
  channel: Accessor<ChannelStatus>;
  onApply: (
    channel: ChannelId,
    hex: string,
    brightness: number,
    enabled: boolean,
  ) => Promise<boolean>;
  onOff: (channel: ChannelId) => Promise<void>;
}) {
  const hex = signal(props.channel().hex);
  const brightness = signal(props.channel().brightness);
  const enabled = signal(props.channel().enabled);
  const dirty = signal(false);
  let lastSignature = channelSignature(props.channel());

  effect(() => {
    const next = props.channel();
    const signature = channelSignature(next);
    if (signature === lastSignature) return;
    lastSignature = signature;
    if (dirty()) return;
    hex.set(next.hex);
    brightness.set(next.brightness);
    enabled.set(next.enabled);
  });

  const apply = async (): Promise<void> => {
    const applied = await props.onApply(
      props.channel().id,
      hex(),
      brightness(),
      enabled(),
    );
    if (applied) dirty.set(false);
  };

  return (
    <View
      class="rounded-[10px] border border-border bg-card p-3 gap-2"
      style={{
        width: channelWidth(),
        boxShadow: enabled() ? `0 0 22px ${hex().slice(0, 7)}22` : "none",
      }}
    >
      <View
        style={{
          height: 3,
          borderRadius: 2,
          backgroundColor: enabled() ? hex().slice(0, 7) : "#1a2128",
        }}
      />
      <View class="flex-row items-start justify-between gap-2">
        <View class="gap-0.5" style={{ flexShrink: 1 }}>
          <Text class="text-sm font-semibold text-foreground">{props.channel().name}</Text>
          <Text class="text-[11px] text-muted-foreground">{props.channel().description}</Text>
        </View>
        <View class="flex-row items-center gap-1">
          <Chip label={`${props.channel().leds} LED${props.channel().leds === 1 ? "" : "s"}`} tone="muted" />
          <Chip
            label={enabled() ? `${brightness()}%` : "off"}
            tone={enabled() ? "ok" : "muted"}
          />
        </View>
      </View>

      <View class="flex-row items-center justify-between gap-2">
        <View class="flex-row items-center gap-2">
          <SwatchButton
            testID={`channel-colour-${props.channel().id}`}
            value={hex}
            label={`${props.channel().name} colour`}
            width={40}
            height={28}
            onChange={(value) => {
              dirty.set(true);
              enabled.set(true);
              hex.set(value);
            }}
          />
          <Text class="text-xs font-mono text-muted-foreground">{hex().toUpperCase()}</Text>
        </View>
        <Switch
          testID={`channel-enabled-${props.channel().id}`}
          checked={enabled()}
          accessibilityLabel={`${props.channel().label} enabled`}
          trackClass={SWITCH_TRACK}
          thumbClass={SWITCH_THUMB}
          onChange={({ checked }) => {
            dirty.set(true);
            enabled.set(checked);
          }}
        />
      </View>

      <View class="gap-0.5">
        <View class="flex-row items-baseline justify-between">
          <Text class="text-[11px] uppercase text-muted-foreground">Brightness</Text>
          <Text class="text-[11px] font-mono text-foreground">{`${brightness()}%`}</Text>
        </View>
        <Slider
          value={brightness()}
          min={0}
          max={100}
          step={1}
          disabled={!enabled()}
          accessibilityLabel={`${props.channel().label} brightness`}
          trackClass="h-5 justify-center"
          indicatorClass="h-1 rounded-full bg-muted"
          rangeClass="h-1 rounded-full bg-primary"
          thumbClass="w-3.5 h-3.5 rounded-full bg-foreground"
          onChange={(event) => {
            const value = Array.isArray(event.value) ? event.value[0] : event.value;
            dirty.set(true);
            brightness.set(value);
          }}
          onChangeEnd={(event) => {
            const value = Array.isArray(event.value) ? event.value[0] : event.value;
            brightness.set(value);
          }}
        />
      </View>

      <View class="flex-row items-center gap-2" style={{ flexWrap: "wrap" }}>
        <Button
          testID={`channel-apply-${props.channel().id}`}
          class="rounded-[6px] bg-primary px-3 py-1.5 disabled:opacity-40"
          onPress={() => void apply()}
        >
          <Text class="text-[11px] text-primary-foreground">Apply</Text>
        </Button>
        <Button
          testID={`channel-off-${props.channel().id}`}
          class="rounded-[6px] border border-border px-3 py-1.5"
          onPress={() => void props.onOff(props.channel().id)}
        >
          <Text class="text-[11px] text-foreground">Off</Text>
        </Button>
        <If when={dirty()}>
          <Chip label="unsaved" tone="warn" />
        </If>
      </View>
    </View>
  );
}

function gpuSignature(zone: GpuZoneStatus): string {
  return `${zone.hex}|${zone.mode}|${zone.brightness}|${zone.enabled}`;
}

/** One GPU LED zone: colour, effect, brightness, enable, Apply / Off. */
function GpuZoneCard(props: {
  device: Accessor<GpuDeviceStatus>;
  zone: Accessor<GpuZoneStatus>;
  write: (device: string, zone: string, state: GpuZoneState) => Promise<void>;
  turnOff: (device: string, zone: string) => Promise<void>;
}) {
  const zoneId = props.zone().id;
  const hex = signal(props.zone().hex);
  const mode = signal(String(props.zone().mode));
  const brightness = signal(gpuBrightnessToPercent(props.zone().brightness));
  const enabled = signal(props.zone().enabled);
  const dirty = signal(false);
  let last = gpuSignature(props.zone());

  effect(() => {
    const next = props.zone();
    const signature = gpuSignature(next);
    if (signature === last) return;
    last = signature;
    if (dirty()) return;
    hex.set(next.hex);
    mode.set(String(next.mode));
    brightness.set(gpuBrightnessToPercent(next.brightness));
    enabled.set(next.enabled);
  });

  const write = async (on: boolean): Promise<void> => {
    const state: GpuZoneState = {
      color: hexToRgb(hex()),
      mode: Number(mode()) || GPU_DEFAULT_MODE,
      speed: GPU_DEFAULT_SPEED,
      brightness: gpuPercentToBrightness(brightness()),
      direction: 0,
      enabled: on,
    };
    await props.write(props.device().id, zoneId, state);
    dirty.set(false);
  };

  const subtitle =
    zoneId === "argb_header"
      ? "5V header for external devices — no onboard LEDs"
      : props.device().model || props.device().name;

  return (
    <View
      class="rounded-[10px] border border-border bg-card p-3 gap-2"
      style={{ width: channelWidth() }}
    >
      <View class="flex-row items-start justify-between gap-2">
        <View class="gap-0.5" style={{ flexShrink: 1 }}>
          <Text class="text-sm font-semibold text-foreground">{props.zone().label}</Text>
          <Text class="text-[11px] text-muted-foreground">{subtitle}</Text>
        </View>
        <View class="flex-row items-center gap-1">
          <Chip label={`${props.zone().leds} LED`} tone="muted" />
          <Chip label={props.zone().mode_label} tone={enabled() ? "ok" : "muted"} />
        </View>
      </View>

      <View class="flex-row items-center justify-between gap-2 border-t border-border pt-2">
        <View class="flex-row items-center gap-2">
          <SwatchButton
            value={hex}
            onChange={(value) => {
              dirty.set(true);
              enabled.set(true);
              hex.set(value);
            }}
          />
          <Text class="text-xs font-mono text-muted-foreground">{hex().toUpperCase()}</Text>
        </View>
        <Switch
          checked={enabled()}
          accessibilityLabel={`${props.zone().label} enabled`}
          trackClass={SWITCH_TRACK}
          thumbClass={SWITCH_THUMB}
          onChange={({ checked }) => {
            dirty.set(true);
            enabled.set(checked);
          }}
        />
      </View>

      <View class="gap-0.5">
        <Text class="text-[11px] uppercase text-muted-foreground">Effect</Text>
        <Select
          items={EFFECT_ITEMS}
          value={mode()}
          accessibilityLabel={`${props.zone().label} effect`}
          class="h-8 rounded-[6px] border border-border bg-muted"
          onChange={({ value }) => {
            if (value === null) return;
            dirty.set(true);
            mode.set(value);
          }}
        />
      </View>

      <View class="gap-0.5">
        <View class="flex-row items-baseline justify-between">
          <Text class="text-[11px] uppercase text-muted-foreground">Brightness</Text>
          <Text class="text-[11px] font-mono text-foreground">{`${brightness()}%`}</Text>
        </View>
        <Slider
          value={brightness()}
          min={0}
          max={100}
          step={1}
          disabled={!enabled()}
          accessibilityLabel={`${props.zone().label} brightness`}
          trackClass="h-5 justify-center"
          indicatorClass="h-1 rounded-full bg-muted"
          rangeClass="h-1 rounded-full bg-primary"
          thumbClass="w-3.5 h-3.5 rounded-full bg-foreground"
          onChange={(event) => {
            const value = Array.isArray(event.value) ? event.value[0] : event.value;
            dirty.set(true);
            brightness.set(value);
          }}
          onChangeEnd={(event) => {
            const value = Array.isArray(event.value) ? event.value[0] : event.value;
            brightness.set(value);
          }}
        />
      </View>

      <View class="flex-row items-center gap-2" style={{ flexWrap: "wrap" }}>
        <Button
          class="rounded-[6px] bg-primary px-3 py-1.5"
          onPress={() => void write(enabled())}
        >
          <Text class="text-[11px] text-primary-foreground">Apply</Text>
        </Button>
        <Button
          class="rounded-[6px] border border-border px-3 py-1.5"
          onPress={() => void props.turnOff(props.device().id, zoneId)}
        >
          <Text class="text-[11px] text-foreground">Off</Text>
        </Button>
        <If when={dirty()}>
          <Chip label="unsaved" tone="warn" />
        </If>
      </View>
    </View>
  );
}

interface SyncSource {
  label: string;
  hex: string;
  brightness: number;
  target: "gpu" | "board";
}

function SyncDialog(props: {
  open: Accessor<boolean>;
  onOpenChange: (open: boolean) => void;
  sources: Accessor<SyncSource[]>;
  onPick: (source: SyncSource) => void;
}) {
  return (
    <Dialog
      open={props.open()}
      onOpenChange={({ open }) => props.onOpenChange(open)}
      accessibilityLabel="Sync colour"
      class="rounded-[12px] border border-border bg-popover p-4 gap-3"
      style={centeredDialog(dialogWidth(420))}
      backdropClass="bg-black/50"
    >
      <View class="gap-1">
        <Text class="text-sm font-semibold text-foreground">Sync colour</Text>
        <Text class="text-xs text-muted-foreground">
          Pick a source colour. It will be applied to every zone in the other group.
        </Text>
      </View>
      <View class="gap-1.5">
        <Each of={props.sources()} key={(source) => source.label}>
          {(source) => (
            <Button
              class="flex-row items-center gap-2 rounded-[7px] border border-border px-3 py-2"
              onPress={() => props.onPick(source())}
            >
              <View
                class="h-4 w-4 rounded-[3px] border border-border"
                style={{ backgroundColor: source().hex }}
              />
              <Text class="text-xs text-foreground" style={{ flex: 1 }}>
                {source().label}
              </Text>
              <Text class="text-[11px] font-mono text-muted-foreground">{source().hex.toUpperCase()}</Text>
            </Button>
          )}
        </Each>
        <If when={props.sources().length === 0}>
          <Text class="text-xs text-muted-foreground">
            No lit source found. Enable a channel or a GPU zone first.
          </Text>
        </If>
      </View>
      <View class="flex-row justify-end">
        <Button
          class="rounded-[6px] border border-border px-3 py-1.5"
          onPress={() => props.onOpenChange(false)}
        >
          <Text class="text-xs text-foreground">Cancel</Text>
        </Button>
      </View>
    </Dialog>
  );
}

export function Lighting(props: {
  store: AppStore;
  toast: (message: string, tone?: Tone) => void;
}) {
  const channels = computed(() => props.store.snapshot()?.channels ?? []);
  const device = computed(() => props.store.snapshot()?.led_device ?? null);
  const gpu = computed(() => props.store.snapshot()?.gpu_led ?? null);
  const frame = computed(() => expandFrame(channels()));

  const globalHex = signal("#22d3ee");
  const globalBright = signal(80);
  const boardHex = signal("#ff0000");
  const gpuHex = signal("#eeff00");
  const touched = signal(false);
  const presetsOpen = signal(false);
  const syncOpen = signal(false);

  effect(() => {
    const list = channels();
    if (touched() || list.length === 0) return;
    const source = list.find((channel) => channel.enabled) ?? list[0];
    globalHex.set(source.hex);
    globalBright.set(source.brightness);
    boardHex.set(source.hex);
  });
  const gpuTouched = signal(false);
  effect(() => {
    const status = gpu();
    if (gpuTouched() || !status || status.devices.length === 0) return;
    for (const dev of status.devices) {
      const zone = dev.zones.find((candidate) => candidate.enabled) ?? dev.zones[0];
      if (zone) gpuHex.set(zone.hex);
    }
  });

  const sources = computed<SyncSource[]>(() => {
    const result: SyncSource[] = [];
    for (const channel of channels()) {
      if (channel.enabled) {
        result.push({
          label: `Motherboard · ${channel.name}`,
          hex: channel.hex,
          brightness: channel.brightness,
          target: "gpu",
        });
      }
    }
    for (const dev of gpu()?.devices ?? []) {
      for (const zone of dev.zones) {
        if (zone.enabled) {
          result.push({
            label: `GPU · ${dev.model || dev.name} · ${zone.label}`,
            hex: zone.hex,
            brightness: gpuBrightnessToPercent(zone.brightness),
            target: "board",
          });
        }
      }
    }
    return result;
  });

  const onSyncPick = async (source: SyncSource): Promise<void> => {
    syncOpen.set(false);
    try {
      if (source.target === "gpu") {
        let message = "";
        for (const dev of gpu()?.devices ?? []) {
          for (const zone of dev.zones) {
            const state: GpuZoneState = {
              color: hexToRgb(source.hex),
              mode: zone.mode,
              speed: zone.speed,
              brightness: gpuPercentToBrightness(source.brightness),
              direction: zone.direction,
              enabled: true,
            };
            message = await props.store.gpu.setZone(dev.id, zone.id, state, true);
          }
        }
        props.toast(message || `Synced ${source.hex.toUpperCase()} to every GPU zone`, "ok");
      } else {
        props.toast(await props.store.lighting.setAll(source.hex, source.brightness), "ok");
      }
    } catch (error) {
      props.toast(`Sync failed: ${String(error)}`, "danger");
    }
  };

  const onApply = async (
    channel: ChannelId,
    hex: string,
    brightness: number,
    enabled: boolean,
  ): Promise<boolean> => {
    const current = channels().find((candidate) => candidate.id === channel);
    if (
      !(await confirmWrite(
        props.store.settings(),
        `Write ${hex.toUpperCase()} at ${brightness}% to ${current?.label ?? channel}?`,
      ))
    ) {
      return false;
    }
    try {
      const message = await props.store.lighting.setChannel(
        channel,
        hex,
        brightness,
        enabled,
      );
      props.toast(message, "ok");
      return true;
    } catch (error) {
      props.toast(`Lighting write failed: ${String(error)}`, "danger");
      return false;
    }
  };

  const onOff = async (channel: ChannelId): Promise<void> => {
    try {
      props.toast(await props.store.lighting.off(channel), "ok");
    } catch (error) {
      props.toast(`Lighting write failed: ${String(error)}`, "danger");
    }
  };

  const writeGpuZone = async (
    device: string,
    zone: string,
    state: GpuZoneState,
  ): Promise<void> => {
    if (
      !(await confirmWrite(
        props.store.settings(),
        `Write ${hexFromRgb(state.color)} to GPU ${zone}?`,
      ))
    ) {
      return;
    }
    try {
      props.toast(await props.store.gpu.setZone(device, zone, state, true), "ok");
    } catch (error) {
      props.toast(`GPU lighting write failed: ${String(error)}`, "danger");
    }
  };

  const offGpuZone = async (device: string, zone: string): Promise<void> => {
    try {
      props.toast(await props.store.gpu.setZone(device, zone, gpuOffState(), true), "ok");
    } catch (error) {
      props.toast(`GPU lighting write failed: ${String(error)}`, "danger");
    }
  };

  const applyAll = async (): Promise<void> => {
    if (
      !(await confirmWrite(
        props.store.settings(),
        `Write ${globalHex().toUpperCase()} at ${globalBright()}% to all six channels?`,
      ))
    ) {
      return;
    }
    try {
      props.toast(await props.store.lighting.setAll(globalHex(), globalBright()), "ok");
      touched.set(true);
    } catch (error) {
      props.toast(`Lighting write failed: ${String(error)}`, "danger");
    }
  };

  const dim = async (): Promise<void> => {
    try {
      props.toast(await props.store.lighting.dim(globalBright()), "ok");
    } catch (error) {
      props.toast(`Brightness failed: ${String(error)}`, "danger");
    }
  };

  const allOff = async (): Promise<void> => {
    if (!(await confirmWrite(props.store.settings(), "Turn every lighting channel off?"))) {
      return;
    }
    try {
      props.toast(await props.store.lighting.allOff(), "ok");
    } catch (error) {
      props.toast(`Lighting write failed: ${String(error)}`, "danger");
    }
  };

  const applyBoardGroup = () => void applyAll();
  const applyGpuGroup = async (): Promise<void> => {
    let message = "";
    for (const dev of gpu()?.devices ?? []) {
      for (const zone of dev.zones) {
        const state: GpuZoneState = {
          color: hexToRgb(gpuHex()),
          mode: zone.mode,
          speed: zone.speed,
          brightness: zone.brightness,
          direction: zone.direction,
          enabled: true,
        };
        message = await props.store.gpu.setZone(dev.id, zone.id, state, true);
      }
    }
    if (message) props.toast(message, "ok");
  };

  const boardSubtitle =
    channels().length === 0
      ? (device()?.error ?? "The controller reported no lighting channels.")
      : `${channels().length} channels · ${LED_TOTAL} LEDs · onboard controller`;
  const gpuDevices = gpu()?.devices ?? [];
  const gpuSubtitle =
    gpuDevices.length === 0
      ? "No card detected · AMD / NVIDIA / Intel"
      : gpuDevices.slice(0, 2).map((dev) => dev.model || dev.name).join(", ") ||
        `${gpuDevices.length} GPU controller(s)`;

  return (
    <View class="gap-4">
      <PageHeader
        title="Lighting"
        actions={
          <Button
            testID="lighting-presets"
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

      <DeviceGroup
        title="Motherboard"
        subtitle={boardSubtitle}
        dot={boardHex()}
        color={boardHex}
        onColor={(hex) => {
          touched.set(true);
          globalHex.set(hex);
          boardHex.set(hex);
        }}
        onSync={() => syncOpen.set(true)}
        onApply={applyBoardGroup}
      >
        <View class="gap-4 px-4 pb-4">
          <Panel title="Global colour" subtitle="Every channel, one colour">
            <View class="flex-row items-end gap-4" style={{ flexWrap: "wrap" }}>
              <View
                testID="global-colour"
                class="h-10 w-14 rounded-[6px] border border-border"
                style={{ backgroundColor: globalHex() }}
              />
              <Text class="text-sm font-mono text-muted-foreground">
                {globalHex().toUpperCase()}
              </Text>

              <View style={{ width: 240 }} class="gap-1">
                <Slider
                  value={globalBright()}
                  min={0}
                  max={100}
                  step={1}
                  accessibilityLabel="Global brightness"
                  trackClass="h-5 justify-center"
                  indicatorClass="h-1 rounded-full bg-muted"
                  rangeClass="h-1 rounded-full bg-primary"
                  thumbClass="w-3.5 h-3.5 rounded-full bg-foreground"
                  onChange={(event) => {
                    const value = Array.isArray(event.value) ? event.value[0] : event.value;
                    globalBright.set(value);
                  }}
                  onChangeEnd={(event) => {
                    const value = Array.isArray(event.value) ? event.value[0] : event.value;
                    globalBright.set(value);
                  }}
                >
                  <Text class="text-[11px] text-muted-foreground">{`Brightness · ${globalBright()}%`}</Text>
                </Slider>
              </View>

            </View>

            <View class="flex-row items-center gap-2" style={{ flexWrap: "wrap" }}>
                <Button
                  testID="lighting-apply-all"
                  class="rounded-[7px] bg-primary px-3 py-1.5"
                  onPress={() => void applyAll()}
                >
                  <Text class="text-xs text-primary-foreground">Apply to all six channels</Text>
                </Button>
                <Button
                  testID="lighting-dim"
                  class="rounded-[7px] border border-border px-3 py-1.5"
                  onPress={() => void dim()}
                >
                  <Text class="text-xs text-foreground">Dim lit channels</Text>
                </Button>
                <Button
                  testID="lighting-all-off"
                  class="rounded-[7px] border border-[#f87171]/50 px-3 py-1.5"
                  onPress={() => void allOff()}
                >
                  <Text class="text-xs text-[#f87171]">All off</Text>
                </Button>
            </View>

            <View class="flex-row items-start gap-4" style={{ flexWrap: "wrap" }}>
              <SwatchButton
                testID="color-preset-1"
                value={globalHex}
                width={48}
                height={30}
                onChange={(value) => {
                  touched.set(true);
                  globalHex.set(value);
                  boardHex.set(value);
                }}
              />
            </View>
          </Panel>

          <Panel title="Frame preview" subtitle="What the controller receives — 10 packets of 20 LEDs">
            <View
              testID="led-frame"
              style={{
                width: "100%",
                padding: 6,
                borderRadius: 4,
                backgroundColor: "#0a0e11",
                borderWidth: 1,
                borderColor: "#1a2128",
              }}
            >
              <LedStrip cells={frame()} height={26} />
            </View>
            <View class="flex-row gap-3" style={{ flexWrap: "wrap" }}>
              <Each of={channels()} key={(channel) => channel.id}>
                {(channel) => (
                  <View class="flex-row items-center gap-1.5">
                    <View
                      class="h-2 w-2 rounded-full"
                      style={{ backgroundColor: channel().render_hex }}
                    />
                    <Text class="text-[10px] font-mono text-muted-foreground">
                      {channel().name} · {channel().hex}
                    </Text>
                  </View>
                )}
              </Each>
            </View>
          </Panel>

          <View class="flex-row gap-3" style={{ flexWrap: "wrap" }}>
            <Each of={channels()} key={(channel) => channel.id}>
              {(channel) => <ChannelCard channel={channel} onApply={onApply} onOff={onOff} />}
            </Each>
          </View>

          <If when={channels().length === 0}>
            <Panel title="No lighting channels">
              <Text class="text-xs text-muted-foreground">
                {device()?.available
                  ? "The controller reported no addressable channels. On this board six are expected; check that the RGB header is connected."
                  : "The USB HID controller did not answer. Replug the header, then try again."}
              </Text>
            </Panel>
          </If>
        </View>
      </DeviceGroup>

      <DeviceGroup
        title="GPU"
        subtitle={gpuSubtitle}
        dot={gpuHex()}
        color={gpuHex}
        onColor={(hex) => {
          gpuTouched.set(true);
          gpuHex.set(hex);
        }}
        onSync={() => syncOpen.set(true)}
        onApply={() => void applyGpuGroup()}
      >
        <View class="px-4 pb-4">
          <Panel
            title="GPU LED"
            subtitle="Nuvoton RGB at I²C 0x36 · AMD ADL / NVIDIA NVAPI / Intel IGCL"
          >
            <Text class="text-xs text-muted-foreground">
              {gpuDevices.length === 0
                ? "No GPU RGB controller detected"
                : `${gpuDevices.length} controller(s) · ${gpu()?.device.detail ?? ""}`}
            </Text>
            <If when={gpuDevices.length === 0}>
              <Text class="text-[11px] text-muted-foreground">
                {gpu()?.device.error ?? "AMD ADL / NVIDIA NVAPI / Intel IGCL"}
              </Text>
            </If>
            <View class="flex-row gap-3" style={{ flexWrap: "wrap" }}>
              <Each of={gpuDevices} key={(dev) => dev.id}>
                {(dev) => (
                  <Each of={dev().zones} key={(zone) => zone.id}>
                    {(zone) => (
                      <GpuZoneCard
                        device={dev}
                        zone={zone}
                        write={writeGpuZone}
                        turnOff={offGpuZone}
                      />
                    )}
                  </Each>
                )}
              </Each>
            </View>
          </Panel>
        </View>
      </DeviceGroup>

      <SyncDialog
        open={syncOpen}
        onOpenChange={syncOpen.set}
        sources={sources}
        onPick={(source) => void onSyncPick(source)}
      />

      <SetupsDialog
        kind="lighting"
        open={presetsOpen}
        onOpenChange={presetsOpen.set}
        store={props.store}
        toast={props.toast}
      />
    </View>
  );
}

function hexFromRgb(color: readonly [number, number, number]): string {
  return `#${color.map((part) => part.toString(16).padStart(2, "0")).join("")}`.toUpperCase();
}

function gpuOffState(): GpuZoneState {
  return {
    color: [0, 0, 0],
    mode: GPU_DEFAULT_MODE,
    speed: GPU_DEFAULT_SPEED,
    brightness: 0,
    direction: 0,
    enabled: false,
  };
}
