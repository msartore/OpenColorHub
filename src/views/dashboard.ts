/**
 * Overview screen: thermal gauge with the session range, six headline numbers,
 * fan and lighting state, temperature history, system facts and recent writes.
 */

import { api } from "../api";
import { el } from "../dom";
import { dutyColor, formatClock, formatRpm, formatTemp } from "../format";
import type { ElevationState, FanHeaderId, Snapshot } from "../types";
import type { View, ViewContext } from "../view";
import { ledStrip, sparkline, temperatureGauge } from "../components/charts";
import { button, panel, pill, statTile } from "../components/ui";

/** Samples kept for the history chart; at the default 1 s poll that is two minutes. */
const HISTORY = 120;

export function createDashboard(ctx: ViewContext): View {
  const gauge = temperatureGauge();
  const spark = sparkline({ capacity: HISTORY });

  /** Every temperature seen this session; the gauge shows the instantaneous value. */
  const history: number[] = [];

  const dutyTile = statTile({ label: "Average duty", value: "—", unit: "%", hint: "across every header" });
  const rpmTile = statTile({ label: "Total rpm", value: "—", unit: "rpm", hint: "all headers" });
  const idleTile = statTile({ label: "Idle headers", value: "—", hint: "reporting 0 rpm" });
  const litTile = statTile({ label: "Channels lit", value: "—", hint: "of 6 lighting channels" });
  const curveTile = statTile({ label: "Curve driven", value: "—", hint: "headers on SmartFan" });
  const ledTile = statTile({ label: "LEDs lit", value: "—", hint: "of 200 addressable" });

  const gaugeRange = el("p", { class: "gauge-range mono" }, "session —");
  const fastest = el("p", { class: "panel-foot mono" }, "fastest —");

  const fanList = el("ul", { class: "fan-mini" });
  const fanRows = new Map<
    FanHeaderId,
    { root: HTMLElement; rpm: HTMLElement; pct: HTMLElement; fill: HTMLElement; mode: HTMLElement }
  >();

  const forEachHeader = () => {
    fanList.replaceChildren();
    fanRows.clear();
    for (const header of ["cpu", "cha_fan1", "cha_fan2", "cha_fan3", "pump"] as FanHeaderId[]) {
      const rpm = el("span", { class: "fan-mini-rpm mono" }, "0");
      const pct = el("span", { class: "fan-mini-pct mono" }, "0%");
      const mode = el("span", { class: "fan-mini-mode" }, "—");
      const fill = el("span", { class: "bar-fill" });
      const row = el(
        "li",
        { class: "fan-mini-row" },
        el(
          "div",
          { class: "fan-mini-head" },
          el("span", { class: "fan-mini-label" }, header.toUpperCase().replace("CHA_", "CHA ")),
          mode,
          el("span", { class: "fan-mini-spacer" }),
          rpm,
          el("span", { class: "fan-mini-unit" }, "rpm"),
          pct,
        ),
        el("div", { class: "bar bar--thin" }, fill),
      );
      fanList.appendChild(row);
      fanRows.set(header, { root: row, rpm, pct, fill, mode });
    }
  };
  forEachHeader();

  const strip = ledStrip({ legend: false });
  const lightNote = el("p", { class: "note" }, "");
  const channelBody = el("tbody");
  const channelTable = el(
    "table",
    { class: "data-table dash-led-table" },
    el(
      "thead",
      {},
      el("tr", {}, el("th", {}, "channel"), el("th", {}, "leds"), el("th", {}, "colour"), el("th", {}, "level")),
    ),
    channelBody,
  );

  const sparkLegend = el("div", { class: "spark-legend" });
  const sysList = el("div", { class: "kv-list sys-list" });
  const activity = el("ul", { class: "activity" });
  let lastActivityFetch = 0;

  const notes = el("div", { class: "notice-list" });
  const boardPill = pill("detecting board…", "muted");

  /** Privileges are fixed for the life of the process, so this is read once. */
  let elevation: ElevationState | null = null;
  void api
    .elevation()
    .then((state) => {
      elevation = state;
    })
    .catch(() => {
      elevation = null;
    });

  const root = el(
    "div",
    { class: "screen" },
    el(
      "header",
      { class: "screen-head" },
      el("div", {}, el("h1", { class: "screen-title" }, "Dashboard")),
      el(
        "div",
        { class: "screen-actions" },
        boardPill,
        button("Refresh", { variant: "ghost", size: "sm", onClick: () => void ctx.refresh() }),
      ),
    ),
    notes,
    el(
      "div",
      { class: "dash" },
      panel({
        className: "area-gauge",
        title: "Motherboard temperature",
        body: [gauge.el, gaugeRange, spark.el, sparkLegend],
      }),
      el(
        "div",
        { class: "stat-grid area-stats" },
        dutyTile.el,
        rpmTile.el,
        idleTile.el,
        litTile.el,
        curveTile.el,
        ledTile.el,
      ),
      panel({
        className: "area-fans",
        title: "Fan headers",
        actions: [
          button("Open fan control", { variant: "quiet", size: "sm", onClick: () => ctx.navigate("fans") }),
        ],
        body: [fanList, fastest],
      }),
      panel({
        className: "area-light",
        title: "Lighting",
        actions: [button("Edit lighting", { variant: "quiet", size: "sm", onClick: () => ctx.navigate("leds") })],
        body: [strip.el, lightNote, channelTable],
      }),
      panel({
        className: "area-system",
        title: "System",
        subtitle: "What this build is talking to",
        body: [sysList],
      }),
      panel({
        className: "area-activity",
        title: "Activity",
        body: [activity],
      }),
    ),
  );

  const tempText = (celsius: number): string => formatTemp(celsius, ctx.settings().temp_unit, 0);

  const renderActivity = async () => {
    try {
      const entries = await api.activity();
      if (!entries.length) {
        activity.replaceChildren(
          el("li", { class: "activity-empty" }, "No writes yet — changes you apply show up here."),
        );
        return;
      }
      activity.replaceChildren(
        ...entries.slice(0, 6).map((entry) =>
          el(
            "li",
            { class: "activity-row" },
            el("span", { class: `activity-kind activity-kind--${entry.kind}` }, entry.kind),
            el("span", { class: "activity-msg" }, entry.message),
            el("span", { class: "activity-time mono" }, formatClock(entry.at_ms)),
          ),
        ),
      );
    } catch {
      activity.replaceChildren(el("li", { class: "activity-empty" }, "Activity unavailable."));
    }
  };

  const renderSystem = (snapshot: Snapshot) => {
    const board = snapshot.board;
    const name = board.identity?.name?.trim() || board.identity?.vendor?.trim() || "unknown board";
    const row = (key: string, value: string, tone?: string) =>
      el(
        "div",
        { class: "kv" },
        el("span", { class: "kv-key" }, key),
        el("span", { class: `kv-value mono${tone ? ` kv-value--${tone}` : ""}`, title: value }, value),
      );

    const backend = (device: Snapshot["fan_device"]) =>
      device.available ? `${device.name}${device.simulated ? " · simulated" : ""}` : "unavailable";

    const privileges = elevation
      ? elevation.elevated
        ? "administrator"
        : elevation.can_request
          ? "standard · can elevate"
          : "standard"
      : "—";

    sysList.replaceChildren(
      row("board", name),
      row(
        "support",
        board.confidence,
        board.confidence === "verified" ? "ok" : board.confidence === "unsupported" ? "danger" : "warn",
      ),
      row("fan backend", backend(snapshot.fan_device)),
      row("lighting backend", backend(snapshot.led_device)),
      row("bios", board.identity?.bios_version?.trim() || "—"),
      row("polling", `${ctx.settings().poll_interval_ms} ms`),
      row("privileges", privileges, elevation?.elevated ? "ok" : undefined),
      row("last sample", formatClock(snapshot.timestamp_ms)),
    );
  };

  const update = (snapshot: Snapshot) => {
    gauge.update(snapshot.temperature_c, ctx.settings().temp_unit);

    if (snapshot.temperature_c !== null) {
      history.push(snapshot.temperature_c);
      if (history.length > HISTORY) history.shift();
    }
    if (history.length) {
      const low = Math.min(...history);
      const high = Math.max(...history);
      const mean = history.reduce((sum, value) => sum + value, 0) / history.length;
      gaugeRange.textContent = `session ${tempText(low)} · ${tempText(mean)} · ${tempText(high)}`;
      spark.update(history);
      sparkLegend.replaceChildren(
        el("span", { class: "spark-now mono" }, `now ${tempText(history[history.length - 1])}`),
        el("span", {}, `${history.length} samples`),
        el("span", {}, `range ${tempText(low)} – ${tempText(high)}`),
      );
    } else {
      gaugeRange.textContent = "session —";
      sparkLegend.replaceChildren(el("span", {}, "Waiting for the first temperature reading"));
    }

    const idle = snapshot.fans.filter((fan) => fan.rpm === 0).length;
    const totalRpm = snapshot.fans.reduce((sum, fan) => sum + fan.rpm, 0);
    dutyTile.set(
      String(snapshot.avg_percent),
      `across ${snapshot.fans.length} headers`,
      snapshot.avg_percent > 85 ? "warn" : "accent",
    );
    rpmTile.set(formatRpm(totalRpm), `${idle} idle`, "muted");
    idleTile.set(String(idle), idle ? "not spinning" : "all spinning", idle ? "warn" : "ok");

    const lit = snapshot.channels.filter((channel) => channel.enabled);
    litTile.set(
      String(lit.length),
      `${lit.length} of ${snapshot.channels.length} on`,
      lit.length ? "accent" : "muted",
    );
    const totalLeds = snapshot.channels.reduce((sum, channel) => sum + channel.leds, 0);
    const leds = lit.reduce((sum, channel) => sum + channel.leds, 0);
    ledTile.set(String(leds), `of ${totalLeds}`, leds ? "accent" : "muted");

    const curved = snapshot.fans.filter((fan) => fan.mode === "smart_fan").length;
    curveTile.set(String(curved), curved ? "SmartFan curves active" : "all manual", curved ? "ok" : "muted");

    for (const fan of snapshot.fans) {
      const row = fanRows.get(fan.header);
      if (!row) continue;
      row.rpm.textContent = formatRpm(fan.rpm);
      row.pct.textContent = `${fan.percent}%`;
      row.fill.style.width = `${fan.percent}%`;
      row.fill.style.background = dutyColor(fan.percent);
      row.mode.textContent =
        fan.mode === "smart_fan"
          ? `curve · ${fan.curve_percent ?? "—"}%`
          : fan.mode === "manual"
            ? "manual"
            : "—";
      row.mode.className = `fan-mini-mode${fan.mode === "smart_fan" ? " is-curve" : ""}`;
      row.root.classList.toggle("is-idle", fan.rpm === 0);
    }

    const spinning = snapshot.fans.filter((fan) => fan.rpm > 0);
    fastest.textContent = spinning.length
      ? `fastest ${spinning.reduce((best, fan) => (fan.rpm > best.rpm ? fan : best)).label} · ${formatRpm(
          Math.max(...spinning.map((fan) => fan.rpm)),
        )} rpm`
      : "no header is spinning";

    strip.update(snapshot.channels);
    lightNote.textContent = lit.length
      ? `${lit.length} of ${snapshot.channels.length} channels lit · ${leds} LEDs`
      : "All channels are off — open Lighting to set a colour.";

    channelBody.replaceChildren(
      ...snapshot.channels.map((channel) =>
        el(
          "tr",
          { class: channel.enabled ? "" : "is-off", title: channel.name },
          el("td", {}, channel.label),
          el("td", { class: "mono" }, String(channel.leds)),
          el(
            "td",
            {},
            el(
              "span",
              { class: "dash-swatch" },
              el("i", {
                class: "dash-swatch-dot",
                style: { background: channel.enabled ? channel.hex : "transparent" },
              }),
              channel.enabled ? channel.hex.toUpperCase() : "off",
            ),
          ),
          el("td", { class: "mono" }, channel.enabled ? `${channel.brightness}%` : "—"),
        ),
      ),
    );

    const board = snapshot.board;
    const boardName = board.identity?.name?.trim() || board.identity?.vendor?.trim() || "unknown board";
    const tone =
      board.confidence === "verified"
        ? "ok"
        : board.confidence === "unsupported"
          ? "danger"
          : board.confidence === "compatible"
            ? "accent"
            : "warn";
    boardPill.className = `pill pill--${tone}`;
    boardPill.replaceChildren(
      el("span", { class: "pill-dot" }),
      el("span", { class: "board-pill-name" }, boardName),
      el("span", { class: "board-pill-state" }, board.confidence),
    );
    boardPill.title = `${board.identity?.vendor ?? ""} ${boardName} · detected via ${board.source} · ${board.catalog_size} boards in the catalogue`;

    const allNotes = [...snapshot.capabilities.notes];
    if (allNotes.length) {
      const headline = !snapshot.capabilities.fan_control
        ? "Fan control unavailable"
        : !snapshot.capabilities.led_control
          ? "Lighting unavailable"
          : "Hardware notes";
      notes.replaceChildren(
        el(
          "details",
          { class: "notice notice--fold" },
          el(
            "summary",
            {},
            el("span", { class: "notice-icon" }, "!"),
            el("span", { class: "notice-headline" }, headline),
            el("span", { class: "notice-more" }, "details"),
          ),
          el("div", { class: "notice-copy" }, ...allNotes.map((n) => el("p", {}, n))),
        ),
      );
    } else {
      notes.replaceChildren();
    }

    renderSystem(snapshot);

    if (Date.now() - lastActivityFetch > 2000) {
      lastActivityFetch = Date.now();
      void renderActivity();
    }
  };

  const initial = ctx.snapshot();
  if (initial) update(initial);
  void renderActivity();

  return { el: root, update };
}
