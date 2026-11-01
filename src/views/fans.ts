/** Fan control: per-header manual duty, SmartFan curve editing, presets. */

import { api } from "../api";
import { el } from "../dom";
import { PROFILES, curveFor, sameCurve } from "../curves";
import { dutyColor, formatRpm } from "../format";
import type { CurvePoint, FanHeaderId, FanStatus, Snapshot } from "../types";
import type { View, ViewContext } from "../view";
import { curveEditor, type CurveEditor } from "../components/charts";
import { button, cardHead, panel, slider, type Slider } from "../components/ui";
import { showPawnioDialog } from "../pawnio";
import { presetsButton } from "../components/setups-panel";
import { stateSlot } from "../components/state";

const HEADERS: { id: FanHeaderId; label: string; description: string }[] = [
  { id: "sys", label: "SYS_FAN", description: "System fan / rear exhaust" },
  { id: "cpu", label: "CPU_FAN", description: "CPU cooler or AIO radiator" },
  { id: "aux0", label: "AUX0", description: "Auxiliary header" },
  { id: "aux1", label: "AUX1", description: "Auxiliary header" },
  { id: "aux2", label: "AUX2", description: "Auxiliary header" },
  { id: "aux3", label: "AUX3", description: "Auxiliary header" },
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
  stallChip: HTMLElement;
  rpm: HTMLElement;
  pct: HTMLElement;
  mode: HTMLElement;
  fill: HTMLElement;
  slider: Slider;
  editor: CurveEditor;
  hint: HTMLElement;
  dirty: HTMLElement;
  appliedCurve: CurvePoint[];
  lastSignature: string;
}

export function createFans(ctx: ViewContext): View {
  const cards = new Map<FanHeaderId, FanCard>();
  const warningSlot = el("div", { class: "fans-warning" });
  // Kept apart from `warningSlot`: update() rewrites that one on every tick.
  const pawnioSlot = el("div", { class: "fans-pawnio" });
  const grid = el("div", { class: "fan-grid" });
  // Headers nothing is plugged into are hidden rather than shown as five empty
  // cards; this line accounts for them.
  const fanNote = el("p", { class: "fans-note" });
  const fanState = stateSlot();

  const makeCard = (meta: (typeof HEADERS)[number]): FanCard => {
    const rpm = el("span", { class: "readout-value mono" }, "0");
    const pct = el("span", { class: "readout-sub mono" }, "0%");
    const mode = el("span", { class: "mode-chip" }, "manual");
    const stallChip = el("span", { class: "mode-chip mode-chip--danger" }, "stalled");
    stallChip.hidden = true;
    const fill = el("span", { class: "bar-fill" });
    const dirty = el("span", { class: "dirty-flag" }, "unsaved");
    const hint = el("span", { class: "curve-hint" }, "drag points");

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
        right: [stallChip, mode],
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
            hint,
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
      stallChip,
      slider: manual,
      editor,
      hint,
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
        if (!ctx.confirmWrite(`Apply the ${profile.label} profile to all six channels?`)) return;
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
        fanNote,
      ),
      el("div", { class: "screen-actions" }, presetsButton(ctx, { kind: "fans" }).el),
    ),
    warningSlot,
    pawnioSlot,
    panel({
      title: "Curve presets",
      subtitle: "Quiet, standard or full tilt",
      body: [presetRow],
    }),
    grid,
    fanState.el,
  );

  const applyFan = (fan: FanStatus, snapshot: Snapshot) => {
    const card = cards.get(fan.header);
    if (!card) return;

    card.root.classList.toggle("is-undetected", !fan.detected);
    card.root.classList.toggle("is-pump", fan.pump);
    card.hint.textContent = fan.pump ? "min 60% · pump" : "drag points";
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
    card.stallChip.hidden = !fan.stalled;

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
    const present = snapshot.fans.filter((fan) => fan.detected);
    const absent = snapshot.fans.length - present.length;

    if (present.length === 0) {
      fanNote.textContent = "";
      grid.hidden = true;
      fanState.show({
        kind: snapshot.read_error ? "error" : "empty",
        icon: snapshot.read_error ? "alert" : "fans",
        title: snapshot.read_error ? "Fan readings failed" : "No fans detected",
        description: snapshot.read_error
          ? "The board's Super I/O did not answer, so speeds, duty and curves cannot be read or written."
          : "No header is reporting a speed. With PawnIO installed this board normally exposes five; check that the fans are seated on the headers.",
        detail: snapshot.read_error,
        actions: snapshot.read_error
          ? [{ label: "Try again", onClick: () => void ctx.refresh() }]
          : [],
      });
    } else if (absent > 0) {
      grid.hidden = false;
      fanState.clear();
      fanNote.textContent = `No more fans detected — ${present.length} of ${snapshot.fans.length} headers are in use.`;
    } else {
      fanNote.textContent = "";
      grid.hidden = false;
      fanState.clear();
    }

    const stalled = present.filter((fan) => fan.stalled);
    const lowDuty = present.filter(
      (fan) => fan.header !== "aux3" && fan.percent < settings.low_duty_warning,
    );

    if (stalled.length) {
      warningSlot.replaceChildren(
        el(
          "div",
          { class: "notice notice--danger" },
          el("span", { class: "notice-icon" }, "!"),
          el(
            "div",
            { class: "notice-copy" },
            el(
              "p",
              {},
              `${stalled.map((f) => `${f.label} (${f.percent}%)`).join(", ")} reporting no rotation while driven — check the connection or lower the duty that stalled it.`,
            ),
            el("p", { class: "notice-meta" }, "A fan can also be stopped by a curve that drops below its start duty."),
          ),
        ),
      );
    } else if (lowDuty.length) {
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
      // Only the headers that exist decide whether a profile is "running".
      const active =
        present.length > 0 &&
        present.every((fan) => sameCurve(fan.curve, curveFor(profile.id, fan.header === "aux3")));
      presetButtons[index].classList.toggle("is-active", active);
    });

    for (const fan of snapshot.fans) applyFan(fan, snapshot);
  };

  // PawnIO missing (or blocked by UAC) is the one reason fan control reads
  // simulated values on a supported board, so offer the fix where it matters.
  void api
    .pawnioStatus()
    .then((status) => {
      if (status.state !== "not_installed" && status.state !== "denied") return;
      const action = button(status.state === "denied" ? "Restart as administrator" : "Install PawnIO", {
        variant: "primary",
        size: "sm",
        onClick: () => showPawnioDialog(ctx, status),
      });
      pawnioSlot.append(
        el(
          "div",
          { class: "note note--warn pawnio-callout" },
          el(
            "span",
            {},
            status.state === "denied"
              ? "Fan readings need administrator rights — PawnIO is installed but this process cannot open it."
              : "Fan readings are simulated: PawnIO is not installed.",
          ),
          action,
        ),
      );
    })
    .catch(() => {
      /* the probe must not break the screen */
    });

  const initial = ctx.snapshot();
  if (initial) update(initial);

  return { el: root, update };
}
