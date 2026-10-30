/** Fan control: per-header manual duty, SmartFan curve editing, presets. */

import { api } from "../api";
import { el } from "../dom";
import { PROFILES, curveFor, sameCurve } from "../curves";
import { dutyColor, formatRpm } from "../format";
import type { CurvePoint, FanHeaderId, FanStatus, Snapshot } from "../types";
import type { View, ViewContext } from "../view";
import { curveEditor, type CurveEditor } from "../components/charts";
import { button, cardHead, panel, slider, type Slider } from "../components/ui";

const HEADERS: { id: FanHeaderId; label: string; description: string; floor: number }[] = [
  { id: "cpu", label: "CPU_FAN", description: "CPU cooler / AIO radiator", floor: 0 },
  { id: "cha_fan1", label: "CHA_FAN1", description: "Chassis intake", floor: 0 },
  { id: "cha_fan2", label: "CHA_FAN2", description: "Chassis exhaust", floor: 0 },
  { id: "cha_fan3", label: "CHA_FAN3", description: "Chassis auxiliary", floor: 0 },
  { id: "pump", label: "AIO_PUMP", description: "AIO pump — never below 60%", floor: 60 },
];

const DEFAULT_CURVE: CurvePoint[] = [
  { temp_c: 20, percent: 20 },
  { temp_c: 40, percent: 40 },
  { temp_c: 60, percent: 60 },
  { temp_c: 80, percent: 100 },
];

interface FanCard {
  header: FanHeaderId;
  root: HTMLElement;
  rpm: HTMLElement;
  pct: HTMLElement;
  mode: HTMLElement;
  fill: HTMLElement;
  slider: Slider;
  editor: CurveEditor;
  dirty: HTMLElement;
  appliedCurve: CurvePoint[];
  lastSignature: string;
}

export function createFans(ctx: ViewContext): View {
  const cards = new Map<FanHeaderId, FanCard>();
  const warningSlot = el("div", { class: "fans-warning" });
  const grid = el("div", { class: "fan-grid" });

  const makeCard = (meta: (typeof HEADERS)[number]): FanCard => {
    const rpm = el("span", { class: "readout-value mono" }, "0");
    const pct = el("span", { class: "readout-sub mono" }, "0%");
    const mode = el("span", { class: "mode-chip" }, "manual");
    const fill = el("span", { class: "bar-fill" });
    const dirty = el("span", { class: "dirty-flag" }, "unsaved");

    const ref: { card?: FanCard } = {};

    const manual = slider({
      min: 0,
      max: 100,
      step: 1,
      value: 50,
      suffix: "%",
      label: "Manual duty",
      onCommit: async (value) => {
        if (!ctx.confirmWrite(`Set ${meta.label} to ${value}%?`)) {
          ref.card?.slider.setValue(ref.card.slider.value());
          return;
        }
        try {
          const result = await api.setFanPercent(meta.id, value);
          ctx.applySnapshot(result.snapshot);
          ctx.toast(result.message, "ok");
        } catch (error) {
          ctx.toast(`Set failed: ${String(error)}`, "danger");
        }
      },
    });

    const editor = curveEditor({
      curve: DEFAULT_CURVE,
      temperature: null,
      onChange: () => dirty.classList.add("is-visible"),
    });

    const applyBtn = button("Apply curve", {
      variant: "primary",
      size: "sm",
      onClick: async () => {
        const points = editor.curve();
        try {
          const result = await api.setFanCurve(meta.id, points);
          ctx.applySnapshot(result.snapshot);
          ctx.toast(result.message, "ok");
          dirty.classList.remove("is-visible");
        } catch (error) {
          ctx.toast(`Curve rejected: ${String(error)}`, "danger");
        }
      },
    });

    const revertBtn = button("Revert", {
      variant: "quiet",
      size: "sm",
      onClick: () => {
        const applied = ref.card?.appliedCurve;
        if (applied) editor.setCurve(applied);
        dirty.classList.remove("is-visible");
      },
    });

    const root = el(
      "section",
      { class: "panel fan-card", "data-header": meta.id },
      cardHead({
        title: meta.label,
        subtitle: meta.description,
        right: [mode],
      }),
      el(
        "div",
        { class: "fan-card-body" },
        el(
          "div",
          { class: "readout" },
          el("div", { class: "readout-main" }, rpm, el("span", { class: "readout-unit" }, "rpm")),
          el("div", { class: "readout-side" }, pct, el("span", { class: "readout-caption" }, "duty")),
        ),
        el("div", { class: "bar" }, fill),
        manual.el,
        el(
          "div",
          { class: "curve-block" },
          el(
            "div",
            { class: "curve-block-head" },
            el("span", { class: "curve-block-title" }, "SmartFan curve"),
            dirty,
          ),
          editor.el,
          el(
            "div",
            { class: "curve-actions" },
            applyBtn,
            revertBtn,
            el("span", { class: "curve-hint" }, `min ${meta.floor}% · drag points`),
          ),
        ),
      ),
    );

    const card: FanCard = {
      header: meta.id,
      root,
      rpm,
      pct,
      mode,
      fill,
      slider: manual,
      editor,
      dirty,
      appliedCurve: DEFAULT_CURVE.map((p) => ({ ...p })),
      lastSignature: "",
    };
    ref.card = card;
    return card;
  };

  for (const meta of HEADERS) {
    const card = makeCard(meta);
    cards.set(meta.id, card);
    grid.appendChild(card.root);
  }

  const presetButtons = PROFILES.map((profile) =>
    button(profile.label, {
      variant: "ghost",
      onClick: async () => {
        if (!ctx.confirmWrite(`Apply the ${profile.label} profile to all five headers?`)) return;
        try {
          const result = await api.applyProfile(HEADERS.map((h) => h.id), profile.id);
          ctx.applySnapshot(result.snapshot);
          ctx.toast(result.message, "ok");
        } catch (error) {
          ctx.toast(`Profile failed: ${String(error)}`, "danger");
        }
      },
    }),
  );

  const presetRow = el(
    "div",
    { class: "preset-row" },
    ...PROFILES.map((profile, index) =>
      el("div", { class: "preset" }, presetButtons[index], el("span", { class: "preset-blurb" }, profile.blurb)),
    ),
  );

  const root = el(
    "div",
    { class: "screen" },
    el(
      "header",
      { class: "screen-head" },
      el(
        "div",
        {},
        el("h1", { class: "screen-title" }, "Fan control"),
      ),
    ),
    warningSlot,
    panel({
      title: "Curve presets",
      subtitle: "Quiet, standard or full tilt",
      body: [presetRow],
    }),
    grid,
  );

  const applyFan = (fan: FanStatus, snapshot: Snapshot) => {
    const card = cards.get(fan.header);
    if (!card) return;

    card.rpm.textContent = formatRpm(fan.rpm);
    card.pct.textContent = `${fan.percent}%`;
    card.fill.style.width = `${fan.percent}%`;
    card.fill.style.background = dutyColor(fan.percent);
    card.mode.textContent =
      fan.mode === "smart_fan"
        ? `curve → ${fan.curve_percent ?? "—"}%`
        : fan.mode === "manual"
          ? "manual"
          : "unknown";
    card.mode.className = `mode-chip${fan.mode === "smart_fan" ? " is-curve" : ""}`;

    const input = card.slider.el.querySelector("input");
    if (document.activeElement !== input) card.slider.setValue(fan.percent);

    const signature = fan.curve ? fan.curve.map((p) => `${p.temp_c}:${p.percent}`).join(",") : "";
    if (signature && signature !== card.lastSignature) {
      card.lastSignature = signature;
      card.appliedCurve = fan.curve!.map((p) => ({ ...p }));
      card.editor.setCurve(fan.curve!);
      card.dirty.classList.remove("is-visible");
    }
    card.editor.setTemperature(snapshot.temperature_c);
  };

  const update = (snapshot: Snapshot) => {
    const settings = ctx.settings();
    const lowDuty = snapshot.fans.filter(
      (fan) => fan.header !== "pump" && fan.percent < settings.low_duty_warning,
    );

    if (lowDuty.length) {
      warningSlot.replaceChildren(
        el(
          "div",
          { class: "notice notice--warn" },
          el("span", { class: "notice-icon" }, "!"),
          el(
            "div",
            { class: "notice-copy" },
            el(
              "p",
              {},
              `${lowDuty.map((f) => f.label).join(", ")} below ${settings.low_duty_warning}% — verify the fans keep spinning under load.`,
            ),
            el("p", { class: "notice-meta" }, "Threshold lives in Settings → Safety."),
          ),
        ),
      );
    } else {
      warningSlot.replaceChildren();
    }

    // Highlight the preset that matches every header, so the user can see at a
    // glance which profile the board is currently running.
    PROFILES.forEach((profile, index) => {
      const active = snapshot.fans.every((fan) =>
        sameCurve(fan.curve, curveFor(profile.id, fan.header === "pump")),
      );
      presetButtons[index].classList.toggle("is-active", active);
    });

    for (const fan of snapshot.fans) applyFan(fan, snapshot);
  };

  const initial = ctx.snapshot();
  if (initial) update(initial);

  return { el: root, update };
}
