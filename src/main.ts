/** Application shell: state, routing, telemetry loop, toasts. */

import "./styles/app.css";

import { api, onTelemetry } from "./api";
import { createSidebar } from "./components/sidebar";
import { el, qs } from "./dom";
import type { ScreenId, Settings, SettingsPatch, Snapshot } from "./types";
import type { Tone } from "./components/ui";
import { maybeOfferUpdate } from "./update";
import type { View, ViewContext, ViewFactory } from "./view";
import { createAbout } from "./views/about";
import { createDashboard } from "./views/dashboard";
import { createFans } from "./views/fans";
import { createLeds } from "./views/leds";
import { createSettings } from "./views/settings";

const VIEWS: Record<ScreenId, ViewFactory> = {
  dashboard: createDashboard,
  fans: createFans,
  leds: createLeds,
  settings: createSettings,
  about: createAbout,
};

const SCREEN_ORDER: ScreenId[] = ["dashboard", "fans", "leds", "settings", "about"];

const state: {
  snapshot: Snapshot | null;
  settings: Settings | null;
  screen: ScreenId;
  view: View | null;
  version: string;
} = { snapshot: null, settings: null, screen: "dashboard", view: null, version: "" };

const toastStack = qs<HTMLElement>("#toasts");
const appRoot = qs<HTMLElement>("#app");

function toast(message: string, tone: Tone = "muted"): void {
  const node = el(
    "div",
    { class: `toast toast--${tone}` },
    el("span", { class: "toast-mark" }, tone === "danger" ? "×" : tone === "ok" ? "✓" : "·"),
    el("span", { class: "toast-message" }, message),
  );
  toastStack.appendChild(node);
  window.setTimeout(() => {
    node.classList.add("is-leaving");
    window.setTimeout(() => node.remove(), 220);
  }, 4200);
  while (toastStack.children.length > 4) toastStack.firstElementChild?.remove();
}

function applyAccent(accent: string): void {
  if (/^#[0-9a-fA-F]{6}$/.test(accent)) {
    document.documentElement.style.setProperty("--accent", accent);
  }
}


const mainColumn = el(
  "div",
  { class: "main-column" },
  el("main", { class: "main", id: "main" }),
);

const sidebar = createSidebar({
  onNavigate: (screen) => navigate(screen),
});

appRoot.append(sidebar.el, mainColumn);


function parseHash(): ScreenId | null {
  const raw = window.location.hash.replace(/^#\/?/, "").trim();
  return (SCREEN_ORDER as string[]).includes(raw) ? (raw as ScreenId) : null;
}

function navigate(screen: ScreenId): void {
  if (state.view && state.screen === screen) return;
  state.screen = screen;
  sidebar.setActive(screen);

  const main = qs<HTMLElement>("#main");
  main.replaceChildren();

  const factory = VIEWS[screen];
  const view = factory(context);
  state.view = view;
  main.appendChild(view.el);
  main.scrollTop = 0;

  if (state.snapshot) view.update(state.snapshot);

  if (window.location.hash !== `#/${screen}`) {
    window.history.replaceState(null, "", `#/${screen}`);
  }
  if (state.settings && state.settings.last_screen !== screen) {
    void api.saveSettings({ last_screen: screen }).then((settings) => {
      if (state.settings) state.settings.last_screen = settings.last_screen;
    });
  }
}

const context: ViewContext = {
  settings: () => state.settings ?? fallbackSettings,
  version: () => state.version,
  snapshot: () => state.snapshot,
  toast,
  confirmWrite: (message) => !state.settings?.confirm_writes || window.confirm(message),
  navigate,
  saveSettings: async (patch: SettingsPatch) => {
    const settings = await api.saveSettings(patch);
    state.settings = settings;
    applyAccent(settings.accent);
    updateStatusbar();
  },
  applySnapshot: (snapshot) => {
    state.snapshot = snapshot;
    state.view?.update(snapshot);
    updateStatusbar();
  },
  refresh: async () => {
    const [settings, snapshot] = await Promise.all([api.settings(), api.snapshot()]);
    state.settings = settings;
    state.snapshot = snapshot;
    applyAccent(settings.accent);
    state.view?.update(snapshot);
    updateStatusbar();
  },
};

// Used only before the first settings load completes.
const fallbackSettings: Settings = {
  backend: "auto",
  poll_interval_ms: 1000,
  temp_unit: "celsius",
  smoothing: true,
  reapply_led_on_launch: false,
  confirm_writes: true,
  accent: "#4ade80",
  state_file: null,
  last_screen: "dashboard",
  low_duty_warning: 20,
};

function updateStatusbar(): void {
  const snapshot = state.snapshot;
  if (!snapshot) return;
  sidebar.update(snapshot);
}


async function boot(): Promise<void> {
  try {
    const [settings, snapshot, about] = await Promise.all([api.settings(), api.snapshot(), api.about()]);
    state.settings = settings;
    state.snapshot = snapshot;
    applyAccent(settings.accent);
    state.version = about.version;
    sidebar.setVersion(about.version);
    sidebar.setLicence(about.license.replace(/-only$/, ""));
    updateStatusbar();

    const target = parseHash() ?? (settings.last_screen as ScreenId);
    navigate((SCREEN_ORDER as string[]).includes(target) ? target : "dashboard");

    await onTelemetry((next) => {
      state.snapshot = next;
      state.view?.update(next);
      updateStatusbar();
    });

    window.addEventListener("hashchange", () => {
      const screen = parseHash();
      if (screen && screen !== state.screen) navigate(screen);
    });

    window.addEventListener("keydown", (event) => {
      if (!(event.ctrlKey || event.metaKey)) return;
      const index = Number(event.key);
      if (Number.isInteger(index) && index >= 1 && index <= SCREEN_ORDER.length) {
        event.preventDefault();
        navigate(SCREEN_ORDER[index - 1]);
      }
    });
  } catch (error) {
    appRoot.replaceChildren(
      el(
        "div",
        { class: "fatal" },
        el("h1", {}, "opencolorhub could not start"),
        el("p", {}, "The desktop bridge did not answer. Check the app logs, or run the UI in a browser for the demo mode."),
        el("pre", { class: "fatal-detail" }, String(error)),
      ),
    );
  }
}

void boot();

// Offer a newer release once the interface has painted.
setTimeout(() => void maybeOfferUpdate(), 2500);

// Surface unexpected failures instead of leaving a blank window.
window.addEventListener("error", (event) => {
  if (!appRoot.children.length) {
    appRoot.replaceChildren(
      el(
        "div",
        { class: "fatal" },
        el("h1", {}, "opencolorhub hit an unexpected error"),
        el("p", {}, "The interface stopped while rendering. The message below is the original error."),
        el("pre", { class: "fatal-detail" }, String(event.error ?? event.message)),
      ),
    );
  } else {
    toast(String(event.error ?? event.message), "danger");
  }
});

window.addEventListener("unhandledrejection", (event) => {
  toast(`Unhandled: ${String(event.reason)}`, "danger");
});

