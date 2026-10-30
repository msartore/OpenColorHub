/** Left navigation rail: brand, screens, backend health. */

import { el, svg } from "../dom";
import logoUrl from "../assets/logo.png";
import { isDemo } from "../api";
import type { ScreenId, Snapshot } from "../types";
import { pill } from "./ui";

export interface Sidebar {
  el: HTMLElement;
  setActive(screen: ScreenId): void;
  update(snapshot: Snapshot): void;
  setVersion(version: string): void;
  setLicence(license: string): void;
}

const ICONS: Record<ScreenId, string> = {
  dashboard: "M4 13h6V4H4v9Zm0 7h6v-5H4v5Zm10 0h6v-9h-6v9Zm0-16v5h6V4h-6Z",
  fans: "M12 13a2 2 0 1 0 0-4 2 2 0 0 0 0 4Zm0-2V3m0 8 7 4M12 11 5 15",
  leds: "M12 3v2m0 14v2M5 12H3m18 0h-2M6.3 6.3 4.9 4.9m14.2 14.2-1.4-1.4M6.3 17.7l-1.4 1.4M19.1 4.9l-1.4 1.4M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8Z",
  settings: "M4 7h10m4 0h2M4 17h4m4 0h8M8 4v6m8 6v6",
  about: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18Zm0-13h.01M11 12h1v5h1",
};

const NAV: { id: ScreenId; label: string; hint: string }[] = [
  { id: "dashboard", label: "Dashboard", hint: "Overview" },
  { id: "fans", label: "Fan control", hint: "Headers & curves" },
  { id: "leds", label: "Lighting", hint: "Channels & colour" },
  { id: "settings", label: "Settings", hint: "Backend & display" },
  { id: "about", label: "About", hint: "Board detection" },
];

function icon(path: string): SVGSVGElement {
  const node = svg("svg", {
    class: "nav-icon",
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    "stroke-width": 1.4,
    "stroke-linecap": "round",
    "stroke-linejoin": "round",
  });
  node.appendChild(svg("path", { d: path }));
  return node;
}

function brandMark(): HTMLImageElement {
  return el("img", {
    class: "brand-mark",
    src: logoUrl,
    alt: "",
    "aria-hidden": "true",
    draggable: false,
  });
}

export function createSidebar(options: { onNavigate: (screen: ScreenId) => void }): Sidebar {
  const buttons = new Map<ScreenId, HTMLButtonElement>();

  const nav = el(
    "nav",
    { class: "nav", "aria-label": "Screens" },
    ...NAV.map((item) => {
      const node = el(
        "button",
        {
          type: "button",
          class: "nav-item",
          "data-screen": item.id,
          onclick: () => options.onNavigate(item.id),
        },
        icon(ICONS[item.id]),
        el(
          "span",
          { class: "nav-copy" },
          el("span", { class: "nav-label" }, item.label),
          el("span", { class: "nav-hint" }, item.hint),
        ),
      );
      buttons.set(item.id, node);
      return node;
    }),
  );

  const fanStatus = el("div", { class: "rail-status-row" });
  const ledStatus = el("div", { class: "rail-status-row" });
  const versionNode = el("span", { class: "rail-version" }, "v0.2.0");
  const licenceNode = el("span", { class: "rail-build" }, "GPL-3.0");

  const root = el(
    "aside",
    { class: "sidebar" },
    el(
      "div",
      { class: "brand" },
      brandMark(),
      el(
        "div",
        { class: "brand-copy" },
        el("span", { class: "brand-name" }, "OpenColorHub"),
        el("span", { class: "brand-sub" }, "fan & lighting"),
      ),
    ),
    nav,
    el(
      "div",
      { class: "rail-foot" },
      isDemo ? el("div", { class: "rail-demo" }, "browser demo · simulated data") : null,
      el("div", { class: "rail-status" }, fanStatus, ledStatus),
      el("div", { class: "rail-meta" }, versionNode, el("span", { class: "rail-sep" }, "/"), licenceNode),
    ),
  );

  return {
    el: root,
    setActive(screen) {
      for (const [id, node] of buttons) node.classList.toggle("is-active", id === screen);
    },
    update(snapshot) {
      const fanPill = snapshot.fan_device.simulated
        ? pill("simulated fan", "warn")
        : !snapshot.fan_device.available
          ? pill("fan · offline", "danger")
          : snapshot.capabilities.fan_control
            ? pill("fan · ok", "ok")
            : pill("fan · read-only", "warn");
      const ledPill = snapshot.led_device.simulated
        ? pill("simulated led", "warn")
        : !snapshot.led_device.available
          ? pill("led · offline", "danger")
          : snapshot.capabilities.led_control
            ? pill("led · ok", "ok")
            : pill("led · read-only", "warn");
      fanStatus.replaceChildren(el("span", { class: "rail-status-label" }, "fans"), fanPill);
      ledStatus.replaceChildren(el("span", { class: "rail-status-label" }, "lighting"), ledPill);
    },
    setVersion(version) {
      versionNode.textContent = `v${version}`;
    },
    setLicence(license) {
      licenceNode.textContent = license;
    },
  };
}
