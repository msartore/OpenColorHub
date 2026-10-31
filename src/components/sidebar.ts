/** Left navigation rail: brand, screens, backend health. */

import { el } from "../dom";
import logoUrl from "../assets/logo.png";
import { isDemo } from "../api";
import type { ScreenId, Snapshot } from "../types";
import { icon } from "./icons";
import { pill } from "./ui";

export interface Sidebar {
  el: HTMLElement;
  setActive(screen: ScreenId): void;
  update(snapshot: Snapshot): void;
  setVersion(version: string): void;
  setLicence(license: string): void;
}

const NAV: { id: ScreenId; label: string; hint: string }[] = [
  { id: "dashboard", label: "Dashboard", hint: "Overview" },
  { id: "fans", label: "Fan control", hint: "Headers & curves" },
  { id: "leds", label: "Lighting", hint: "Channels & colour" },
  { id: "drivers", label: "Drivers", hint: "Board downloads" },
  { id: "setups", label: "Setups", hint: "Saved fan & light" },
  { id: "settings", label: "Settings", hint: "Backend & display" },
  { id: "about", label: "About", hint: "Board detection" },
];

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
        icon(item.id),
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
