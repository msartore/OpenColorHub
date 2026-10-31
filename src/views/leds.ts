/** Lighting: per-channel colour, brightness, and the 200-LED frame preview. */

import { api } from "../api";
import { el, fromHex } from "../dom";
import type { ChannelStatus, Snapshot } from "../types";
import type { View, ViewContext } from "../view";
import { ledStrip } from "../components/charts";
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

  const makeCard = (id: string, label: string, name: string, description: string, leds: number): ChannelCard => {
    const colorInput = el("input", { class: "color-input", type: "color", value: "#22d3ee" });
    const hexLabel = el("span", { class: "color-hex" }, "#22D3EE");
    const colorWrap = el("label", { class: "channel-colour" }, colorInput);
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
          colorWrap,
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
    ),
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
    strip.update(snapshot.channels);

    // Until the user picks a colour, the global field shows what the controller
    // is actually running. Leaving it on a hard-coded cyan made the screen
    // disagree with every channel card after navigating back to it.
    if (!pickerTouched) {
      const source = snapshot.channels.find((channel) => channel.enabled) ?? snapshot.channels[0];
      if (source) {
        globalColor.value = source.hex.toLowerCase();
        globalHex.textContent = source.hex.toUpperCase();
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
