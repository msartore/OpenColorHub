/** Overview screen: thermal gauge, headline numbers, fan snapshot, lighting, activity. */

import { api } from "../api";
import { el } from "../dom";
import { dutyColor, formatClock, formatRpm } from "../format";
import type { FanHeaderId, Snapshot } from "../types";
import type { View, ViewContext } from "../view";
import { ledStrip, temperatureGauge } from "../components/charts";
import { button, panel, pill, statTile } from "../components/ui";

export function createDashboard(ctx: ViewContext): View {
  const gauge = temperatureGauge();

  const dutyTile = statTile({ label: "Average duty", value: "—", unit: "%", hint: "across 5 headers" });
  const rpmTile = statTile({ label: "Total rpm", value: "—", unit: "rpm", hint: "all headers" });
  const litTile = statTile({ label: "Channels lit", value: "—", hint: "of 6 lighting channels" });
  const curveTile = statTile({ label: "Curve driven", value: "—", hint: "headers on SmartFan" });

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
  const channelList = el("div", { class: "channel-chips" });
  const lightNote = el("p", { class: "note" }, "");

  const activity = el("ul", { class: "activity" });
  let lastActivityFetch = 0;

  const notes = el("div", { class: "notice-list" });
  const boardPill = pill("detecting board…", "muted");

  const root = el(
    "div",
    { class: "screen" },
    el(
      "header",
      { class: "screen-head" },
      el(
        "div",
        {},
        el("h1", { class: "screen-title" }, "Dashboard"),
      ),
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
        body: [
          gauge.el,
        ],
      }),
      el("div", { class: "stat-grid area-stats" }, dutyTile.el, rpmTile.el, litTile.el, curveTile.el),
      panel({
        className: "area-fans",
        title: "Fan headers",
        actions: [button("Open fan control", { variant: "quiet", size: "sm", onClick: () => ctx.navigate("fans") })],
        body: [fanList],
      }),
      panel({
        className: "area-light",
        title: "Lighting",
        actions: [button("Edit lighting", { variant: "quiet", size: "sm", onClick: () => ctx.navigate("leds") })],
        body: [strip.el, lightNote, channelList],
      }),
      panel({
        className: "area-activity",
        title: "Activity",
        body: [activity],
      }),
    ),
  );

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

  const update = (snapshot: Snapshot) => {
    gauge.update(snapshot.temperature_c, ctx.settings().temp_unit);

    dutyTile.set(String(snapshot.avg_percent), `across ${snapshot.fans.length} headers`, snapshot.avg_percent > 85 ? "warn" : "accent");
    const totalRpm = snapshot.fans.reduce((sum, fan) => sum + fan.rpm, 0);
    rpmTile.set(formatRpm(totalRpm), `${snapshot.fans.filter((f) => f.rpm === 0).length} idle`, "muted");
    const lit = snapshot.channels.filter((c) => c.enabled).length;
    litTile.set(String(lit), lit ? `${snapshot.channels.filter((c) => c.enabled).map((c) => c.label).slice(0, 2).join(", ")}${lit > 2 ? "…" : ""}` : "all off", lit ? "accent" : "muted");
    const curved = snapshot.fans.filter((f) => f.mode === "smart_fan").length;
    curveTile.set(String(curved), curved ? "SmartFan curves active" : "all manual", curved ? "ok" : "muted");

    for (const fan of snapshot.fans) {
      const row = fanRows.get(fan.header);
      if (!row) continue;
      row.rpm.textContent = formatRpm(fan.rpm);
      row.pct.textContent = `${fan.percent}%`;
      row.fill.style.width = `${fan.percent}%`;
      row.fill.style.background = dutyColor(fan.percent);
      row.mode.textContent =
        fan.mode === "smart_fan" ? `curve · ${fan.curve_percent ?? "—"}%` : fan.mode === "manual" ? "manual" : "—";
      row.mode.className = `fan-mini-mode${fan.mode === "smart_fan" ? " is-curve" : ""}`;
      row.root.classList.toggle("is-idle", fan.rpm === 0);
    }

    strip.update(snapshot.channels);
    lightNote.textContent = snapshot.channels.some((channel) => channel.enabled)
      ? `${snapshot.channels.filter((c) => c.enabled).length} of 6 channels lit`
      : "All channels are off — open Lighting to set a colour.";

    channelList.replaceChildren(
      ...snapshot.channels.map((channel) =>
        el(
          "span",
          { class: `channel-chip${channel.enabled ? "" : " is-off"}`, title: channel.name },
          el("span", { class: "channel-chip-swatch", style: { background: channel.enabled ? channel.hex : "transparent" } }),
          el("span", { class: "channel-chip-label" }, channel.label),
          el("span", { class: "channel-chip-value mono" }, channel.enabled ? `${channel.brightness}%` : "off"),
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
