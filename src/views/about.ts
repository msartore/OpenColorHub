/** About: build identity, supported hardware, paths, credits and licences. */

import { api } from "../api";
import { el } from "../dom";
import logoUrl from "../assets/logo.png";
import type { AboutInfo, Snapshot } from "../types";
import type { View, ViewContext } from "../view";
import { badge, kv, panel } from "../components/ui";
import { stateView } from "../components/state";

function boardBadge(board: AboutInfo["board"]): HTMLElement {
  const tone = board.confidence === "verified"
    ? "ok"
    : board.confidence === "unsupported"
      ? "danger"
      : "warn";
  return badge(`${board.confidence} board`, tone);
}

function mark(): HTMLImageElement {
  return el("img", {
    class: "about-mark",
    src: logoUrl,
    alt: "",
    "aria-hidden": "true",
    draggable: false,
  });
}

export function createAbout(_ctx: ViewContext): View {
  const liveDevice = el("p", { class: "screen-sub" }, "Reading controller identity…");
  const detail = el("div", { class: "about-detail" });

  const hero = panel({
    className: "about-hero",
    body: [
      el(
        "div",
        { class: "hero-main" },
        mark(),
        el(
          "div",
          { class: "hero-copy" },
          el("h2", { class: "hero-title" }, "OpenColorHub"),
          el("p", { class: "hero-tagline" }, "Fan and lighting control for Colorful AMD motherboards."),
          el(
            "div",
            { class: "hero-chips" },
            el("span", { class: "chip mono" }, `v${"0.2.0"}`),
            el("span", { class: "chip" }, "GPL-3.0"),
          ),
        ),
      ),
      liveDevice,
    ],
  });

  const credit = el("p", { class: "about-credit" });

  const root = el(
    "div",
    { class: "screen" },
    el(
      "header",
      { class: "screen-head" },
      el(
        "div",
        {},
        el("h1", { class: "screen-title" }, "About"),
      ),
    ),
    hero,
    detail,
    credit,
  );

  const render = (about: AboutInfo) => {
    detail.replaceChildren(
      el(
        "div",
        { class: "about-grid" },
        panel({
          className: "area-detected",
          title: "Detected motherboard",
          subtitle: "Read from SMBIOS at startup — this is what the hardware guard checks",
          actions: [boardBadge(about.board)],
          body: about.board.identity
            ? [
                el(
                  "div",
                  { class: "detected-grid" },
                  el(
                    "div",
                    { class: "kv-list" },
                    kv("vendor", about.board.identity.vendor),
                    kv("model", about.board.identity.name || "—"),
                    kv("revision", about.board.identity.version || "—"),
                    kv("system", about.board.identity.system_product || "—"),
                  ),
                  el(
                    "div",
                    { class: "kv-list" },
                    kv(
                      "bios",
                      `${about.board.identity.bios_vendor ?? ""} ${about.board.identity.bios_version ?? ""}`.trim() || "—",
                    ),
                    kv("detected via", about.board.source),
                    kv("series", about.board.series ?? about.board.family ?? "—"),
                    kv("catalogue", about.board.matched ? `${about.board.matched}` : `${about.board.catalog_size} verified boards`),
                  ),
                ),
                el("ul", { class: "bullet-list" }, ...about.board.notes.map((note) => el("li", {}, note))),
              ]
            : [
                stateView({
                  kind: "empty",
                  icon: "about",
                  title: "Board not identified",
                  description:
                    "SMBIOS did not report a motherboard model, so the hardware guard cannot match this machine to the verified catalogue. RGB and fans may still work; the app just cannot vouch for it.",
                  detail: `detected via ${about.board.source}`,
                  compact: true,
                }),
              ],
        }),
      ),
    );
  };

  void api
    .about()
    .then((about) => {
      render(about);
      credit.replaceChildren(
        el(
          "span",
          {},
          `Register programming and state format derived from ${about.upstream.name} by ${about.upstream.author}, ${about.upstream.license} licensed.`,
        ),
      );
      const version = root.querySelector(".chip.mono");
      if (version) version.textContent = `v${about.version}`;
    })
    .catch((error: unknown) => {
      detail.replaceChildren(
        stateView({
          kind: "error",
          icon: "alert",
          title: "About this build is unavailable",
          description:
            "The build metadata could not be read, so the version, paths and licence are unknown for this install.",
          detail: String(error),
          actions: [{ label: "Try again", onClick: () => location.reload() }],
        }),
      );
    });

  return {
    el: root,
    update: (snapshot: Snapshot) => {
      const fan = snapshot.fan_device;
      const led = snapshot.led_device;
      liveDevice.replaceChildren(
        el("span", { class: "mono" }, `${fan.name} · ${led.name}`),
        el("span", { class: "hero-device-state" }, fan.simulated || led.simulated ? "simulated readings" : "hardware attached"),
      );
    },
  };
}
