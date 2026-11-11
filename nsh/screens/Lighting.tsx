/** Lighting: per-channel colour, a global write, brightness and the frame. */

import { Canvas, Text, View, computed, effect, signal } from "nsh";
import {
  Button,
  ColorArea,
  ColorField,
  ColorPicker,
  ColorSlider,
  ColorSwatch,
  Dialog,
  Slider,
  Switch,
} from "nsh/base";
import type { Accessor } from "nsh";

import type { AppStore } from "../app/composition/store";
import type { ChannelId, ChannelStatus } from "../app/domain/model";
import {
  LED_TOTAL,
  channelSignature,
  expandFrame,
} from "../app/domain/lighting";
import { confirmWrite } from "../components/confirm";
import { SetupsDialog } from "../components/SetupsDialog";
import { Chip, Panel } from "../components/ui";
import { channelWidth, dialogWidth } from "../app/presentation/layout";

type Tone = "ok" | "warn" | "danger" | "muted";

const PRESETS: readonly string[] = [
  "#22d3ee",
  "#e879f9",
  "#fbbf24",
  "#a3e635",
  "#dbeafe",
  "#f87171",
  "#4ade80",
  "#000000",
];

function ColorEditor(props: {
  value: Accessor<string>;
  onChange: (hex: string) => void;
}) {
  return (
    <ColorPicker
      value={props.value()}
      format="hex"
      onChange={({ value, formatted }) => props.onChange(formatted || value)}
      onChangeEnd={({ value, formatted }) => props.onChange(formatted || value)}
    >
      <View class="gap-2" style={{ width: 220 }}>
        <ColorArea class="h-24 w-full rounded-[8px]" />
        <ColorSlider
          channel="hue"
          class="h-5"
          trackClass="h-3 rounded-full"
          thumbClass="w-4 h-4 rounded-full border-2 border-white"
        />
        <ColorField class="h-8 rounded-[6px] border border-border bg-muted px-2 text-xs text-foreground" />
        <View class="flex-row gap-1.5" style={{ flexWrap: "wrap" }}>
          <Each of={PRESETS} key="index">
            {(preset, index) => (
              <ColorSwatch
                testID={`color-preset-${index()}`}
                color={preset()}
                accessibilityLabel={preset()}
                class="h-5 w-5 rounded-[4px] border border-border"
              />
            )}
          </Each>
        </View>
      </View>
    </ColorPicker>
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
  const pickerOpen = signal(false);
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
          <View
            testID={`channel-colour-${props.channel().id}`}
            class="h-7 w-10 rounded-[5px] border border-border"
            style={{ backgroundColor: hex() }}
          />
          <Text class="text-xs font-mono text-muted-foreground">{hex().toUpperCase()}</Text>
          <Button
            class="rounded-[5px] border border-border px-2 py-0.5"
            onPress={() => pickerOpen.set(true)}
          >
            <Text class="text-[10px] text-foreground">Colour</Text>
          </Button>
        </View>
        <Switch
          testID={`channel-enabled-${props.channel().id}`}
          checked={enabled()}
          accessibilityLabel={`${props.channel().label} enabled`}
          trackClass="h-4 w-7 rounded-full bg-muted checked:bg-primary"
          thumbClass="w-3 h-3 rounded-full bg-foreground"
          onChange={({ checked }) => {
            dirty.set(true);
            enabled.set(checked);
          }}
        />
      </View>

      <View class="flex-row items-center gap-1.5" style={{ flexWrap: "wrap" }}>
        <Each of={PRESETS} key="index">
          {(preset) => (
            <ColorSwatch
              color={preset()}
              accessibilityLabel={preset()}
              class="h-5 w-5 rounded-[4px] border border-border"
              onPress={({ color }) => {
                dirty.set(true);
                enabled.set(true);
                hex.set(color);
              }}
            />
          )}
        </Each>
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
      >
        <Text class="text-[11px] text-muted-foreground">Brightness</Text>
      </Slider>

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

      <Dialog
        open={pickerOpen()}
        onOpenChange={({ open }) => pickerOpen.set(open)}
        accessibilityLabel={`${props.channel().name} colour`}
        class="rounded-[12px] border border-border bg-popover p-4 gap-3"
        style={{ width: dialogWidth(280) }}
        backdropClass="bg-black/40"
      >
        <Text class="text-sm font-semibold text-foreground">
          {`${props.channel().name} colour`}
        </Text>
        <ColorEditor
          value={hex}
          onChange={(value) => {
            dirty.set(true);
            enabled.set(true);
            hex.set(value);
          }}
        />
        <View class="flex-row justify-end">
          <View
            role="button"
            class="rounded-[6px] border border-border px-3 py-1.5"
            onPress={() => pickerOpen.set(false)}
          >
            <Text class="text-[11px] text-foreground">Done</Text>
          </View>
        </View>
      </Dialog>
    </View>
  );
}

export function Lighting(props: {
  store: AppStore;
  toast: (message: string, tone?: Tone) => void;
}) {
  const channels = computed(() => props.store.snapshot()?.channels ?? []);
  const device = computed(() => props.store.snapshot()?.led_device ?? null);
  const frame = computed(() => expandFrame(channels()));

  const globalHex = signal("#22d3ee");
  const globalBright = signal(80);
  const touched = signal(false);
  const presetsOpen = signal(false);

  effect(() => {
    const list = channels();
    if (touched() || list.length === 0) return;
    const source = list.find((channel) => channel.enabled) ?? list[0];
    globalHex.set(source.hex);
    globalBright.set(source.brightness);
  });

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
    if (
      !(await confirmWrite(props.store.settings(), "Turn every lighting channel off?"))
    ) {
      return;
    }
    try {
      props.toast(await props.store.lighting.allOff(), "ok");
    } catch (error) {
      props.toast(`Lighting write failed: ${String(error)}`, "danger");
    }
  };

  return (
    <View class="gap-4">
      <View class="flex-row items-center justify-between gap-3">
        <View class="gap-1">
          <Text class="text-2xl font-semibold text-foreground">Lighting</Text>
          <Text class="text-xs text-muted-foreground">
            {channels().length === 0
              ? (device()?.error ?? "The controller reported no lighting channels.")
              : `${channels().length} channels · ${LED_TOTAL} LEDs in the frame`}
          </Text>
        </View>
        <Button
          testID="lighting-presets"
          class="rounded-[7px] border border-border px-3 py-1.5"
          onPress={() => presetsOpen.set(true)}
        >
          <Text class="text-xs text-foreground">Presets</Text>
        </Button>
      </View>

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

          <View class="flex-row items-center gap-2">
            <Button
              testID="lighting-apply-all"
              class="rounded-[7px] bg-primary px-3 py-1.5"
              onPress={() => void applyAll()}
            >
              <Text class="text-xs text-primary-foreground">Apply to all six</Text>
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
        </View>

        <View class="flex-row items-start gap-4" style={{ flexWrap: "wrap" }}>
          <View class="flex-row items-center gap-1.5" style={{ flexWrap: "wrap" }}>
            <Each of={PRESETS} key="index">
              {(preset) => (
                <ColorSwatch
                  color={preset()}
                  accessibilityLabel={preset()}
                  class="h-6 w-6 rounded-[4px] border border-border"
                  onPress={({ color }) => {
                    touched.set(true);
                    globalHex.set(color);
                  }}
                />
              )}
            </Each>
          </View>
          <ColorEditor
            value={globalHex}
            onChange={(value) => {
              touched.set(true);
              globalHex.set(value);
            }}
          />
        </View>
      </Panel>

      <Panel title="Frame preview" subtitle="What the controller receives — 10 packets of 20 LEDs">
        <Canvas
          testID="led-frame"
          style={{ height: 96, width: "100%" }}
          draw={(cx, size) => {
            const cells = frame();
            const columns = 20;
            const rows = 10;
            const width = size.width / columns;
            const height = size.height / rows;
            cx.fillStyle = "#07090b";
            cx.fillRect(0, 0, size.width, size.height);
            for (let index = 0; index < LED_TOTAL; index += 1) {
              const column = index % columns;
              const row = Math.floor(index / columns);
              cx.fillStyle = cells[index] ?? "#000000";
              cx.fillRect(column * width + 1, row * height + 1, width - 2, height - 2);
            }
          }}
        />
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

