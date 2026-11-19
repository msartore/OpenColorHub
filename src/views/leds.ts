import { api } from "../api";
import { el, fromHex } from "../dom";
import type { ChannelStatus, GpuDeviceStatus, GpuZoneState, GpuZoneStatus, Snapshot } from "../types";
import type { View, ViewContext } from "../view";
import { icon } from "../components/icons";
import { ledStrip } from "../components/charts";
import { presetsButton } from "../components/setups-panel";
import { stateSlot } from "../components/state";
import { button, cardHead, panel, pill, slider, type Slider } from "../components/ui";

interface ChannelCard {
  root: HTMLElement;
  colorInput: HTMLInputElement;
  colorWrap: HTMLElement;
  hexLabel: HTMLElement;
  bright: Slider;
  toggle: HTMLInputElement;
  stateChip: HTMLElement;
  lastApplied: string;
}

interface GpuCard {
  root: HTMLElement;
  colorInput: HTMLInputElement;
  modeSelect: HTMLSelectElement;
  bright: Slider;
  toggle: HTMLInputElement;
  chip: HTMLElement;
  lastApplied: string;
}

const GPU_MODES: [number, string][] = [
  [0x01, "Static"], [0x02, "Breathing"], [0x03, "Strobe"], [0x04, "RGB cycle"],
  [0x05, "Random"], [0x07, "Color shift"], [0x08, "Visor"], [0x09, "Stacking"],
  [0x0a, "Fill wave"], [0x0b, "Traveling wave"], [0x0c, "Marquee color"],
  [0x0d, "Marquee random"], [0x0e, "Color wave"], [0x0f, "Rainbow"],
];

const PRESETS: { label: string; hex: string }[] = [
  { label: "Cyan", hex: "#22d3ee" },
  { label: "Magenta", hex: "#e879f9" },
  { label: "Amber", hex: "#fbbf24" },
  { label: "Lime", hex: "#a3e635" },
  { label: "Ice", hex: "#dbeafe" },
  { label: "Off", hex: "#000000" },
];

export function createLeds(ctx: ViewContext): View {
  const cards = new Map<string, ChannelCard>();
  const strip = ledStrip();

  const globalColor = el("input", { class: "color-input", type: "color", value: "#22d3ee" });
  const globalBright = slider({ min: 0, max: 100, value: 80, suffix: "%", label: "Brightness" });
  const globalHex = el("span", { class: "color-hex" }, "#22D3EE");
  /** Set once the user picks a colour, so snapshots stop overwriting the field. */
  let pickerTouched = false;

  globalColor.addEventListener("input", () => {
    pickerTouched = true;
    globalHex.textContent = globalColor.value.toUpperCase();
  });

  const applyAll = button("Apply to all six channels", {
    variant: "primary",
    onClick: async () => {
      if (!ctx.confirmWrite("Write this colour to all six channels?")) return;
      try {
        const result = await api.setAllChannels(fromHex(globalColor.value), globalBright.value(), true);
        ctx.applySnapshot(result.snapshot);
        ctx.toast(result.message, "ok");
      } catch (error) {
        ctx.toast(`Lighting write failed: ${String(error)}`, "danger");
      }
    },
  });

  const allOff = button("All off", {
    variant: "danger",
    onClick: async () => {
      if (!ctx.confirmWrite("Turn every lighting channel off?")) return;
      try {
        const result = await api.setAllChannels([0, 0, 0], 0, false);
        ctx.applySnapshot(result.snapshot);
        ctx.toast(result.message, "ok");
      } catch (error) {
        ctx.toast(`Lighting write failed: ${String(error)}`, "danger");
      }
    },
  });

  const dim = button("Dim lit channels", {
    variant: "ghost",
    onClick: async () => {
      try {
        const result = await api.setBrightness(globalBright.value());
        ctx.applySnapshot(result.snapshot);
        ctx.toast(result.message, "ok");
      } catch (error) {
        ctx.toast(`Brightness failed: ${String(error)}`, "danger");
      }
    },
  });

  const presetRow = el(
    "div",
    { class: "swatch-row" },
    ...PRESETS.map((preset) =>
      el(
        "button",
        {
          type: "button",
          class: "swatch",
          title: preset.label,
          style: { "--swatch": preset.hex },
          onclick: () => {
            pickerTouched = true;
            globalColor.value = preset.hex;
            globalHex.textContent = preset.hex.toUpperCase();
          },
        },
        el("span", { class: "swatch-dot" }),
        preset.label,
      ),
    ),
  );

  const grid = el("div", { class: "channel-grid" });
  const ledState = stateSlot();

  // ---- GPU LED section (AMD ADL / NVIDIA NVAPI / Intel IGCL) ----
  const gpuCards = new Map<string, GpuCard>();
  const gpuList = el("div", { class: "channel-grid" });
  const gpuInfo = el("p", { class: "note" }, "Probing GPU I²C buses…");

  const makeGpuCard = (device: GpuDeviceStatus, zone: GpuZoneStatus): GpuCard => {
    const hexLabel = el(
      "button",
      {
        type: "button",
        class: "color-hex color-hex--button",
        title: "Change this zone's colour",
        "aria-label": "Change colour",
      },
      zone.hex.toUpperCase(),
    ) as HTMLButtonElement;
    const colorInput = el("input", {
      class: "color-input color-input--hidden",
      type: "color",
      value: zone.hex,
    });
    hexLabel.addEventListener("click", () => colorInput.click());
    colorInput.addEventListener("input", () => {
      hexLabel.textContent = colorInput.value.toUpperCase();
    });
    const colorWrap = el("span", { class: "channel-colour" }, colorInput);

    const modeSelect = el(
      "select",
      { class: "select", title: "Hardware effect" },
      ...GPU_MODES.map(([id, label]) => el("option", { value: String(id) }, label)),
    ) as HTMLSelectElement;
    const bright = slider({
      min: 0,
      max: 100,
      value: Math.round((zone.brightness * 100) / 255),
      suffix: "%",
      label: "Brightness",
    });
    const toggle = el("input", { type: "checkbox", class: "switch-input", checked: zone.enabled });
    const chip = el("span", { class: "mode-chip" }, zone.mode_label);

    const write = async (enabled: boolean) => {
      const config: GpuZoneState = {
        color: fromHex(colorInput.value),
        mode: Number(modeSelect.value),
        // Speed and direction are fixed until the UI exposes them.
        speed: 0x80,
        brightness: Math.round((bright.value() * 255) / 100),
        direction: 0,
        enabled,
      };
      try {
        const result = await api.setGpuZone(device.id, zone.id, config, true);
        ctx.applySnapshot(result.snapshot);
        ctx.toast(result.message, "ok");
      } catch (error) {
        ctx.toast(`GPU write failed: ${String(error)}`, "danger");
      }
    };

    // Like the motherboard cards: edits stage in the UI, Apply writes them.
    const apply = button("Apply", {
      variant: "primary",
      size: "sm",
      onClick: () => {
        if (!ctx.confirmWrite(`Write ${hexLabel.textContent} to GPU ${zone.label}?`)) return;
        void write(toggle.checked);
      },
    });
    const off = button("Off", {
      variant: "quiet",
      size: "sm",
      onClick: () => void write(false),
    });

    const root = el(
      "section",
      { class: "panel channel-card", "data-gpu-zone": zone.id },
      cardHead({
        title: zone.label,
        subtitle:
          zone.id === "argb_header"
            ? "5V header for external devices — no onboard LEDs"
            : device.model || device.name,
        right: [pill(`${zone.leds} LED`, "muted"), chip],
      }),
      el(
        "div",
        { class: "channel-card-body" },
        el(
          "div",
          { class: "channel-preview" },
          el(
            "div",
            { class: "channel-preview-meta" },
            el("span", { class: "channel-preview-label" }, zone.label),
            hexLabel,
          ),
          el(
            "label",
            { class: "switch switch--inline", title: "Enable zone" },
            toggle,
            el("span", { class: "switch-track" }),
          ),
        ),
        el("div", { class: "channel-controls" }, modeSelect, bright.el),
        el("div", { class: "card-actions" }, apply, off),
      ),
      colorWrap,
    );
    return { root, colorInput, modeSelect, bright, toggle, chip, lastApplied: "" };
  };

  const syncGpuCard = (device: GpuDeviceStatus, zone: GpuZoneStatus) => {
    const key = `${device.id}:${zone.id}`;
    let card = gpuCards.get(key);
    if (!card) {
      card = makeGpuCard(device, zone);
      gpuCards.set(key, card);
      gpuList.appendChild(card.root);
    }
    const signature = `${zone.hex}|${zone.mode}|${zone.brightness}|${zone.enabled}`;
    if (signature === card.lastApplied) return;
    card.lastApplied = signature;
    if (document.activeElement !== card.colorInput) card.colorInput.value = zone.hex;
    card.modeSelect.value = String(zone.mode);
    if (document.activeElement !== card.bright.el.querySelector("input")) {
      card.bright.setValue(Math.round((zone.brightness * 100) / 255));
    }
    card.toggle.checked = zone.enabled;
    card.chip.textContent = zone.mode_label;
  };

  const gpuPanel = panel({
    title: "GPU LED",
    subtitle: "Nuvoton RGB at I²C 0x36 · AMD ADL / NVIDIA NVAPI / Intel IGCL",
    body: [gpuInfo, gpuList],
  });

  const syncGpuPanel = (snapshot: Snapshot) => {
    const gpu = snapshot.gpu_led;
    if (!gpu || !gpu.enabled || gpu.devices.length === 0) {
      gpuPanel.hidden = true;
      gpuInfo.textContent = "No GPU RGB controller detected";
      gpuGroup.setSubtitle("No card detected · AMD / NVIDIA / Intel");
      return;
    }
    gpuPanel.hidden = false;
    gpuInfo.textContent = gpu.device.available
      ? `${gpu.devices.length} controller(s) · ${gpu.device.detail}`
      : gpu.device.error ?? gpu.device.detail;
    const names = gpu.devices.map((d) => d.model).filter((n) => n.length > 0);
    gpuGroup.setSubtitle(
      names.length > 0
        ? `${names.slice(0, 2).join(", ")}${names.length > 2 ? ` +${names.length - 2}` : ""}`
        : `${gpu.devices.length} GPU controller(s)`,
    );
    for (const device of gpu.devices) {
      for (const zone of device.zones) syncGpuCard(device, zone);
    }
    const firstZone = gpu.devices.flatMap((d) => d.zones).find((z) => z.enabled);
    if (firstZone) gpuGroup.setColor(firstZone.hex);
  };

  const makeCard = (id: string, label: string, name: string, description: string, leds: number): ChannelCard => {
    const colorInput = el("input", { class: "color-input color-input--hidden", type: "color", value: "#22d3ee" });
    const hexLabel = el(
      "button",
      {
        class: "color-hex color-hex--button",
        type: "button",
        title: "Change this channel's colour",
        "aria-label": "Change colour",
      },
      "#22D3EE",
    ) as HTMLButtonElement;
    hexLabel.addEventListener("click", () => colorInput.click());
    const colorWrap = el("span", { class: "channel-colour" }, colorInput);
    const bright = slider({ min: 0, max: 100, value: 60, suffix: "%", label: "Brightness" });
    const toggle = el("input", { type: "checkbox", class: "switch-input", checked: false });
    const stateChip = el("span", { class: "mode-chip" }, "off");

    colorInput.addEventListener("input", () => {
      hexLabel.textContent = colorInput.value.toUpperCase();
    });

    const apply = button("Apply", {
      variant: "primary",
      size: "sm",
      onClick: async () => {
        const enabled = toggle.checked;
        if (!ctx.confirmWrite(`Write ${hexLabel.textContent} to ${label}?`)) return;
        try {
          const result = await api.setChannel(id as ChannelStatus["id"], fromHex(colorInput.value), bright.value(), enabled);
          ctx.applySnapshot(result.snapshot);
          ctx.toast(result.message, "ok");
        } catch (error) {
          ctx.toast(`Lighting write failed: ${String(error)}`, "danger");
        }
      },
    });

    const off = button("Off", {
      variant: "quiet",
      size: "sm",
      onClick: async () => {
        try {
          const result = await api.turnOffChannel(id as ChannelStatus["id"]);
          ctx.applySnapshot(result.snapshot);
          ctx.toast(result.message, "ok");
        } catch (error) {
          ctx.toast(`Lighting write failed: ${String(error)}`, "danger");
        }
      },
    });

    const root = el(
      "section",
      { class: "panel channel-card", "data-channel": id },
      cardHead({
        title: name,
        subtitle: description,
        right: [pill(`${leds} ${leds === 1 ? "LED" : "LEDs"}`, "muted"), stateChip],
      }),
      el(
        "div",
        { class: "channel-card-body" },
        el(
          "div",
          { class: "channel-preview" },
          el(
            "div",
            { class: "channel-preview-meta" },
            el("span", { class: "channel-preview-label" }, label),
            hexLabel,
          ),
          el(
            "label",
            { class: "switch switch--inline", title: "Enable channel" },
            toggle,
            el("span", { class: "switch-track" }),
          ),
        ),
        el("div", { class: "channel-controls" }, bright.el),
        el("div", { class: "card-actions" }, apply, off),
      ),
      // Out of the layout: only exists to be opened by the hex label.
      colorWrap,
    );

    return {
      root,
      colorInput,
      colorWrap,
      hexLabel,
      bright,
      toggle,
      stateChip,
      lastApplied: "",
    };
  };

  type SyncSource = {
    label: string;
    hex: string;
    color: [number, number, number];
    brightness: number;
    target: "gpu" | "board";
  };

  const openSyncModal = () => {
    const snap = ctx.snapshot();
    const sources: SyncSource[] = [];
    for (const channel of snap?.channels ?? []) {
      if (!channel.enabled) continue;
      sources.push({
        label: `Motherboard · ${channel.name}`,
        hex: channel.hex,
        color: channel.color,
        brightness: channel.brightness,
        target: "gpu",
      });
    }
    for (const device of snap?.gpu_led?.devices ?? []) {
      for (const zone of device.zones) {
        sources.push({
          label: `GPU · ${device.model || device.name} · ${zone.label}`,
          hex: zone.hex,
          color: zone.color,
          brightness: zone.brightness,
          target: "board",
        });
      }
    }

    const list = el("div", { class: "preset-list" });
    const closeIcon = el(
      "button",
      { class: "icon-btn dialog-close", type: "button", title: "Close", "aria-label": "Close" },
      "×",
    );
    const card = el(
      "div",
      { class: "update-card presets-card" },
      el("div", { class: "update-head" }, el("h2", { class: "update-title" }, "Sync colour"), closeIcon),
      el(
        "p",
        { class: "update-lead" },
        "Pick a source colour. It will be applied to every zone in the other group.",
      ),
      list,
      el("div", { class: "update-actions" }, button("Cancel", { variant: "quiet", onClick: () => close() })),
    );
    const backdrop = el("div", { class: "update-backdrop", role: "dialog", "aria-modal": "true" }, card);

    function close(): void {
      backdrop.remove();
      document.removeEventListener("keydown", onKey);
    }
    function onKey(event: KeyboardEvent): void {
      if (event.key === "Escape") close();
    }
    closeIcon.addEventListener("click", () => close());
    document.addEventListener("keydown", onKey);

    if (sources.length === 0) {
      list.appendChild(el("p", { class: "note" }, "No lit source found. Enable a channel or a GPU zone first."));
    }
    for (const source of sources) {
      const swatch = el("span", { class: "sync-swatch" });
      swatch.style.setProperty("--swatch", source.hex);
      const row = el(
        "button",
        { class: "sync-source", type: "button" },
        swatch,
        el("span", { class: "sync-source-label" }, source.label),
        el("span", { class: "color-hex" }, source.hex.toUpperCase()),
      );
      row.addEventListener("click", () => void applySource(source));
      list.appendChild(row);
    }

    async function applySource(source: SyncSource): Promise<void> {
      try {
        if (source.target === "gpu") {
          const gpu = ctx.snapshot()?.gpu_led;
          if (!gpu || gpu.devices.length === 0) {
            ctx.toast("No GPU RGB controller detected", "danger");
            return;
          }
          let last: Awaited<ReturnType<typeof api.setGpuZone>> | null = null;
          for (const device of gpu.devices) {
            for (const zone of device.zones) {
              last = await api.setGpuZone(device.id, zone.id, {
                ...zone,
                color: source.color,
                enabled: true,
              }, true);
            }
          }
          if (last) {
            ctx.applySnapshot(last.snapshot);
            ctx.toast(last.message, "ok");
          }
        } else {
          const result = await api.setAllChannels(source.color, source.brightness, true);
          ctx.applySnapshot(result.snapshot);
          ctx.toast(result.message, "ok");
        }
        close();
      } catch (error) {
        ctx.toast(`Sync failed: ${String(error)}`, "danger");
      }
    }

    document.body.appendChild(backdrop);
  };

  const makeGroup = (
    title: string,
    subtitle: string,
    apply: (color: [number, number, number]) => void,
    onSync: () => void,
  ) => {
    const input = el("input", {
      class: "color-input",
      type: "color",
      value: "#22d3ee",
    }) as HTMLInputElement;
    const dot = el("span", { class: "group-dot" });
    dot.style.setProperty("--dot", input.value);
    let touched = false;
    input.addEventListener("input", () => {
      touched = true;
      dot.style.setProperty("--dot", input.value);
    });
    const subtitleP = el("p", {}, subtitle);
    const summary = el(
      "summary",
      { class: "group-summary" },
      dot,
      el(
        "div",
        { class: "group-titles" },
        el("h2", {}, title),
        subtitleP,
      ),
      el("label", { class: "color-field" }, input),
      button("Sync", {
        variant: "ghost",
        size: "sm",
        onClick: onSync,
      }),
      button("Apply", {
        variant: "primary",
        size: "sm",
        onClick: () => apply(fromHex(input.value)),
      }),
      icon("chevron-down", "group-chevron"),
    );
    const details = el("details", { class: "led-group" }, summary);
    return {
      details,
      setColor: (hex: string) => {
        if (touched) return;
        input.value = hex;
        dot.style.setProperty("--dot", hex);
      },
      isTouched: () => touched,
      setSubtitle: (text: string) => {
        subtitleP.textContent = text;
      },
    };
  };

  const boardGroup = makeGroup(
    "Motherboard",
    "Six channels · 200 LEDs · onboard controller",
    (color) => {
      void (async () => {
        try {
          const result = await api.setAllChannels(color, globalBright.value(), true);
          ctx.applySnapshot(result.snapshot);
          ctx.toast(result.message, "ok");
        } catch (error) {
          ctx.toast(`Board write failed: ${String(error)}`, "danger");
        }
      })();
    },
    openSyncModal,
  );

  const gpuGroup = makeGroup(
    "GPU",
    "Per-card profiles · AMD / NVIDIA / Intel",
    (color) => {
      void (async () => {
        const gpu = ctx.snapshot()?.gpu_led;
        if (!gpu || gpu.devices.length === 0) return;
        try {
          let last: Awaited<ReturnType<typeof api.setGpuZone>> | null = null;
          for (const device of gpu.devices) {
            for (const zone of device.zones) {
              last = await api.setGpuZone(device.id, zone.id, {
                ...zone,
                color,
                enabled: true,
              }, true);
            }
          }
          if (last) {
            ctx.applySnapshot(last.snapshot);
            ctx.toast(last.message, "ok");
          }
        } catch (error) {
          ctx.toast(`GPU write failed: ${String(error)}`, "danger");
        }
      })();
    },
    openSyncModal,
  );

  boardGroup.details.appendChild(
    el(
      "div",
      { class: "group-body" },
      panel({
        title: "Global colour",
        subtitle: "Every channel, one colour",
        body: [
          el(
            "div",
            { class: "global-colour" },
            el("label", { class: "color-field color-field--lg" }, globalColor, globalHex),
            el(
              "div",
              { class: "global-colour-controls" },
              globalBright.el,
              el("div", { class: "card-actions" }, applyAll, dim, allOff),
            ),
          ),
          presetRow,
        ],
      }),
      panel({
        title: "Frame preview",
        subtitle: "What the controller receives — 10 packets of 20 LEDs",
        body: [strip.el],
      }),
      grid,
      ledState.el,
    ),
  );
  gpuGroup.details.appendChild(el("div", { class: "group-body" }, gpuPanel));

  const root = el(
    "div",
    { class: "screen" },
    el(
      "header",
      { class: "screen-head" },
      el(
        "div",
        {},
        el("h1", { class: "screen-title" }, "Lighting"),
      ),
      el("div", { class: "screen-actions" }, presetsButton(ctx, { kind: "lighting" }).el),
    ),
    boardGroup.details,
    gpuGroup.details,
  );

  const syncCard = (channel: ChannelStatus) => {
    const card = cards.get(channel.id);
    if (!card) return;

    const signature = `${channel.hex}|${channel.brightness}|${channel.enabled}`;
    if (signature === card.lastApplied) return;
    card.lastApplied = signature;

    if (document.activeElement !== card.colorInput) {
      card.colorInput.value = channel.hex;
      card.hexLabel.textContent = channel.hex.toUpperCase();
    }
    card.colorWrap.classList.toggle("is-off", !channel.enabled);
    if (document.activeElement !== card.bright.el.querySelector("input")) {
      card.bright.setValue(channel.enabled ? channel.brightness : 0);
    }
    card.bright.setDisabled(!channel.enabled);
    card.toggle.checked = channel.enabled;
    card.stateChip.textContent = channel.enabled ? `${channel.brightness}%` : "off";
    card.stateChip.className = `mode-chip${channel.enabled ? " is-curve" : ""}`;
  };

  const update = (snapshot: Snapshot) => {
    syncGpuPanel(snapshot);
    const device = snapshot.led_device;
    if (!device.available || snapshot.channels.length === 0) {
      grid.hidden = true;
      ledState.show(
        device.available
          ? {
              kind: "empty",
              icon: "leds",
              title: "No lighting channels",
              description:
                "The controller reported no addressable channels. On this board six are expected; check that the RGB header is connected.",
            }
          : {
              kind: "error",
              icon: "plug-off",
              title: "Lighting controller not reachable",
              description:
                "The USB HID controller did not answer, so colours and brightness cannot be written. Replug the header, then try again.",
              detail: device.error ?? device.detail,
              actions: [{ label: "Try again", onClick: () => void ctx.refresh() }],
            },
      );
      return;
    }
    grid.hidden = false;
    ledState.clear();

    strip.update(snapshot.channels);

    // Until the user picks a colour, the global field shows what the controller
    // is actually running.
    if (!pickerTouched) {
      const source = snapshot.channels.find((channel) => channel.enabled) ?? snapshot.channels[0];
      if (source) {
        globalColor.value = source.hex.toLowerCase();
        globalHex.textContent = source.hex.toUpperCase();
        boardGroup.setColor(source.hex);
        if (document.activeElement !== globalBright.el.querySelector("input")) {
          globalBright.setValue(source.brightness);
        }
      }
    }

    for (const channel of snapshot.channels) {
      if (!cards.has(channel.id)) {
        const card = makeCard(channel.id, channel.label, channel.name, channel.description, channel.leds);
        cards.set(channel.id, card);
        grid.appendChild(card.root);
      }
      syncCard(channel);
    }
    const litCount = snapshot.channels.filter((c) => c.enabled).length;
    allOff.toggleAttribute("disabled", litCount === 0);
  };

  const initial = ctx.snapshot();
  if (initial) update(initial);

  return { el: root, update };
}
