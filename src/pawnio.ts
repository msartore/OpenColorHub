/**
 * PawnIO prompt: fan control on Windows needs the driver, and it is not
 * something this project ships inside its own installer.
 *
 * The dialog is explicit about what gets downloaded (the vendor's own signed
 * installer, from its own release page, checked against a pinned SHA-256) and
 * that its UI — including its elevation prompt — is what runs, not a silent
 * driver install.
 */

import { api, onPawnioProgress } from "./api";
import { el } from "./dom";
import { button, inlineNote } from "./components/ui";
import type { PawnioStatus } from "./types";
import type { ViewContext } from "./view";

/** Only one dialog per launch, however many times it is called. */
let offered = false;

const PIN_NOTE =
  "PawnIO 2.2.0 · downloaded from the project's GitHub releases and checked against a pinned SHA-256 before it runs.";

export function showPawnioDialog(ctx: ViewContext, status: PawnioStatus): void {
  const progress = el(
    "div",
    { class: "bar bar--thin pawnio-progress", style: "display: none" },
    el("div", { class: "bar-fill" }),
  );
  const statusLine = el("p", { class: "update-status" }, "");

  const later = button("Not now", { variant: "quiet", onClick: () => close() });

  // "Installed but not elevated" and "not installed" need different actions: an
  // install button makes no sense once the driver is there.
  const denied = status.state === "denied";
  const install = button(denied ? "Restart as administrator" : "Install PawnIO", {
    variant: "primary",
    onClick: () => void (denied ? elevate() : run()),
  });
  const site = button("pawnio.eu", {
    variant: "ghost",
    onClick: () => void api.openExternal("https://pawnio.eu"),
  });

  const card = el(
    "div",
    { class: "update-card" },
    el("h2", { class: "update-title" }, "Fan control needs PawnIO"),
    el(
      "p",
      { class: "update-lead" },
      "PawnIO is a signed, sandboxed I/O driver. OpenColorHub uses it to read fan speeds and temperatures on Windows; without it those readings are simulated.",
    ),
    el(
      "ul",
      { class: "bullet-list bullet-list--tight" },
      el("li", {}, "Only Super I/O ports are reachable through it — no physical memory, no MSRs."),
      el("li", {}, PIN_NOTE),
      el("li", {}, "The installer runs with its own window and its own administrator prompt."),
    ),
    inlineNote(
      status.state === "denied"
        ? "PawnIO is installed, but this process is not elevated. Restart OpenColorHub as administrator."
        : "This is a separate project (GPL-2.0) by namazso, not bundled with OpenColorHub.",
      status.state === "denied" ? "warn" : "muted",
    ),
    statusLine,
    progress,
    el("div", { class: "update-actions" }, site, later, install),
  );

  const backdrop = el("div", { class: "update-backdrop", role: "dialog", "aria-modal": "true" }, card);

  function close(): void {
    backdrop.remove();
  }

  async function elevate(): Promise<void> {
    install.disabled = true;
    later.disabled = true;
    statusLine.textContent = "Restarting with administrator rights…";
    try {
      await api.requestElevation();
    } catch (error) {
      statusLine.textContent = `Could not elevate: ${String(error)}`;
      install.disabled = false;
      later.disabled = false;
    }
  }

  async function run(): Promise<void> {
    install.disabled = true;
    later.disabled = true;
    statusLine.textContent = "Downloading the installer…";
    progress.style.display = "";

    try {
      const next = await api.installPawnio();
      if (next.state === "ready") {
        statusLine.textContent = `PawnIO ${next.version} is installed — re-reading the hardware…`;
        await ctx.refresh();
        ctx.toast("PawnIO installed — fan readings are live", "ok");
        close();
      } else if (next.state === "denied") {
        statusLine.textContent = "Installed, but this process is not elevated. Restart as administrator.";
        install.disabled = false;
        later.disabled = false;
      } else {
        statusLine.textContent =
          next.state === "failed" ? `Still unavailable: ${next.detail}` : "The installer finished, but the driver is not answering.";
        install.disabled = false;
        later.disabled = false;
      }
    } catch (error) {
      statusLine.textContent = `Could not install: ${String(error)}`;
      install.disabled = false;
      later.disabled = false;
      progress.style.display = "none";
    }
  }

  backdrop.addEventListener("click", (event) => {
    if (event.target === backdrop) close();
  });
  document.body.appendChild(backdrop);
  install.focus();

  void onPawnioProgress((p) => {
    const pct = p.total ? Math.round((p.received / p.total) * 100) : null;
    const fill = progress.querySelector<HTMLElement>(".bar-fill");
    if (fill) fill.style.width = pct === null ? "35%" : `${pct}%`;
    if (pct !== null) statusLine.textContent = `Downloading the installer… ${pct}%`;
  }).catch(() => {
    /* progress is cosmetic */
  });
}

/** Offers the install once per launch, and only when it would help. */
export async function maybeOfferPawnio(ctx: ViewContext): Promise<void> {
  if (offered) return;
  offered = true;
  try {
    const status = await api.pawnioStatus();
    if (status.state === "not_installed" || status.state === "denied") {
      showPawnioDialog(ctx, status);
    }
  } catch {
    /* an unavailable probe must not interrupt startup */
  }
}
