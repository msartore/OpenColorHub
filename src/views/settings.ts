/** Settings: backend selection, polling, display, safety, paths. */

import { api, isDemo } from "../api";
import { el } from "../dom";
import type { AutostartState, BackendMode, ElevationState, Settings, SettingsPatch, Snapshot, TempUnit } from "../types";
import type { View, ViewContext } from "../view";
import { checkForUpdate, showUpdateDialog } from "../update";
import {
  badge,
  button,
  cardHead,
  inlineNote,
  kv,
  panel,
  pill,
  segmented,
  slider,
  toggle,
  type Slider,
} from "../components/ui";

const BACKEND_COPY: Record<BackendMode, { label: string; blurb: string }> = {
  auto: {
    label: "Auto detect",
    blurb: "Probe the Super I/O and HID controller, fall back to the simulator with a warning.",
  },
  hardware: {
    label: "Hardware only",
    blurb: "Require real hardware; the UI shows an error banner instead of pretending.",
  },
  simulated: {
    label: "Simulated",
    blurb: "Never touch hardware — safe for demos, screenshots and development.",
  },
};

export function createSettings(ctx: ViewContext): View {
  let draft: Settings = { ...ctx.settings() };

  const persist = async (patch: SettingsPatch) => {
    draft = { ...draft, ...patch } as Settings;
    await ctx.saveSettings(patch);
  };

  // ----------------------------------------------------------- privileges
  const elevationBody = el("div", { class: "elevation-body" });

  const renderElevation = (state: ElevationState) => {
    const rows: HTMLElement[] = [
      el(
        "div",
        { class: "card-actions" },
        state.elevated ? pill("elevated", "ok") : pill("standard user", "muted"),
      ),
    ];

    if (!state.elevated && state.can_request && !isDemo) {
      const elevate = button("Restart as administrator", {
        variant: "primary",
        onClick: async () => {
          elevate.disabled = true;
          elevate.textContent = "Waiting for consent…";
          try {
            const outcome = await api.requestElevation();
            // "launched" means the elevated instance takes over and this one exits.
            if (outcome === "declined") {
              elevate.disabled = false;
              elevate.textContent = "Restart as administrator";
              elevationBody.replaceChildren(
                ...rows,
                inlineNote("Elevation cancelled — still running as a standard user."),
              );
            }
          } catch (err) {
            elevate.disabled = false;
            elevate.textContent = "Restart as administrator";
            elevationBody.replaceChildren(...rows, inlineNote(String(err), "danger"));
          }
        },
      });
      rows.push(el("div", { class: "card-actions" }, elevate));
    }

    rows.push(inlineNote(state.detail));
    elevationBody.replaceChildren(...rows);
  };

  void api
    .elevation()
    .then(renderElevation)
    .catch((err) => elevationBody.replaceChildren(inlineNote(String(err), "danger")));

  // ------------------------------------------------------------- backend
  const backendBlurb = el("p", { class: "field-help" }, BACKEND_COPY[draft.backend].blurb);
  const backendState = el("div", { class: "backend-state" });

  const backendSwitch = segmented<BackendMode>({
    value: draft.backend,
    items: (["auto", "hardware", "simulated"] as BackendMode[]).map((id) => ({
      value: id,
      label: BACKEND_COPY[id].label,
    })),
    onChange: async (value) => {
      backendBlurb.textContent = BACKEND_COPY[value].blurb;
      await persist({ backend: value });
      await ctx.refresh();
      ctx.toast(`Backend set to ${BACKEND_COPY[value].label.toLowerCase()}`, "ok");
    },
  });

  // ------------------------------------------------------------- polling
  const pollSlider: Slider = slider({
    min: 250,
    max: 10000,
    step: 250,
    value: draft.poll_interval_ms,
    suffix: " ms",
    label: "Telemetry interval",
    onCommit: async (value) => {
      await persist({ poll_interval_ms: value });
      ctx.toast(`Polling every ${value} ms`, "ok");
    },
  });

  const smoothing = toggle({
    label: "Smooth readings",
    description: "Exponential moving average on temperature and RPM (4 lines of maths, less jitter).",
    checked: draft.smoothing,
    onChange: (checked) => void persist({ smoothing: checked }),
  });

  // ------------------------------------------------------------- display
  const unitSwitch = segmented<TempUnit>({
    value: draft.temp_unit,
    items: [
      { value: "celsius", label: "Celsius" },
      { value: "fahrenheit", label: "Fahrenheit" },
    ],
    onChange: (value) => void persist({ temp_unit: value }),
  });

  const accentInput = el("input", { class: "color-input", type: "color", value: draft.accent });
  const accentHex = el("span", { class: "color-hex" }, draft.accent.toUpperCase());
  accentInput.addEventListener("input", () => {
    accentHex.textContent = accentInput.value.toUpperCase();
    document.documentElement.style.setProperty("--accent", accentInput.value);
  });
  accentInput.addEventListener("change", () => void persist({ accent: accentInput.value }));

  const ACCENTS = ["#4ade80", "#22d3ee", "#e879f9", "#fbbf24", "#f87171", "#a3e635"];
  const accentPresets = el(
    "div",
    { class: "swatch-row" },
    ...ACCENTS.map((hex) =>
      el(
        "button",
        {
          type: "button",
          class: "swatch",
          style: { "--swatch": hex },
          onclick: () => {
            accentInput.value = hex;
            accentHex.textContent = hex.toUpperCase();
            document.documentElement.style.setProperty("--accent", hex);
            void persist({ accent: hex });
          },
        },
        el("span", { class: "swatch-dot" }),
        hex.replace("#", ""),
      ),
    ),
  );

  // ------------------------------------------------------------- safety
  const confirmToggle = toggle({
    label: "Confirm hardware writes",
    description: "Show a confirmation before every fan or lighting change.",
    checked: draft.confirm_writes,
    onChange: (checked) => void persist({ confirm_writes: checked }),
  });

  const reapplyToggle = toggle({
    label: "Re-apply lighting at launch",
    description: "Push the stored CCTL state to the controller when the app starts.",
    checked: draft.reapply_led_on_launch,
    onChange: (checked) => void persist({ reapply_led_on_launch: checked }),
  });

  const lowDuty = slider({
    min: 0,
    max: 60,
    step: 5,
    value: draft.low_duty_warning,
    suffix: "%",
    label: "Warn below this duty",
    onCommit: (value) => void persist({ low_duty_warning: value }),
  });

  // ------------------------------------------------------------- paths
  const pathsBody = el("div", { class: "kv-list" });
  const copyButton = (value: string) =>
    button("copy", {
      variant: "quiet",
      size: "sm",
      onClick: async () => {
        try {
          await navigator.clipboard.writeText(value);
          ctx.toast("Copied to clipboard", "ok");
        } catch {
          ctx.toast("Clipboard unavailable", "warn");
        }
      },
    });

  // ------------------------------------------------------------- danger
  const resetButton = button("Reset to defaults", {
    variant: "danger",
    onClick: async () => {
      if (!window.confirm("Reset all settings to their defaults?")) return;
      await persist({
        backend: "auto",
        poll_interval_ms: 1000,
        temp_unit: "celsius",
        smoothing: true,
        confirm_writes: true,
        reapply_led_on_launch: false,
        accent: "#4ade80",
        low_duty_warning: 20,
      });
      await ctx.refresh();
      ctx.toast("Settings reset", "ok");
    },
  });

  const reloadState = button("Reload state file", {
    variant: "ghost",
    onClick: async () => {
      try {
        await api.snapshot();
        await ctx.refresh();
        ctx.toast("Snapshot reloaded", "ok");
      } catch (error) {
        ctx.toast(`Reload failed: ${String(error)}`, "danger");
      }
    },
  });

  const checkButton = button("Check now", {
    variant: "ghost",
    size: "sm",
    onClick: async () => {
      checkButton.disabled = true;
      checkButton.textContent = "Checking…";
      const status = await checkForUpdate();
      checkButton.disabled = false;
      checkButton.textContent = "Check now";
      if (!status) {
        ctx.toast("Update checks need the desktop build", "warn");
        return;
      }
      if (status.error) {
        ctx.toast(status.error, "warn");
        return;
      }
      if (status.available) {
        void showUpdateDialog(status);
        return;
      }
      ctx.toast(`Up to date — ${status.current} is the newest release`, "ok");
    },
  });

  // ----------------------------------------------------------- startup
  const platformName = navigator.userAgent.includes("Windows") ? "Windows" : "Linux";

  const autostartNote = inlineNote("Checking the logon entry…");
  const autostartToggle = toggle({
    label: `Start with ${platformName}`,
    description: "Launch when you sign in, then push the lighting you saved.",
    checked: false,
    onChange: (checked) => {
      void (async () => {
        try {
          const next = await api.setAutostart(checked);
          renderAutostart(next);
          if (checked) {
            // Starting at sign-in is pointless if nothing is applied, so make
            // sure the saved state is pushed on launch.
            await ctx.saveSettings({ reapply_led_on_launch: true });
            reapplyToggle.setChecked(true);
            ctx.toast("Saved lighting will be applied at sign-in", "ok");
          } else {
            ctx.toast("No longer starting with the OS", "muted");
          }
        } catch (err) {
          autostartToggle.setChecked(!checked);
          ctx.toast(String(err), "danger");
        }
      })();
    },
  });

  const renderAutostart = (state: AutostartState) => {
    autostartToggle.setChecked(state.enabled);
    autostartToggle.el.style.display = state.supported ? "" : "none";
    autostartNote.textContent = state.detail;
    autostartNote.className = state.supported ? "note note--muted" : "note note--warn";
  };

  void api
    .autostart()
    .then(renderAutostart)
    .catch((err) => {
      autostartNote.textContent = String(err);
      autostartNote.className = "note note--warn";
    });

  const root = el(
    "div",
    { class: "screen" },
    el(
      "header",
      { class: "screen-head" },
      el(
        "div",
        {},
        el("h1", { class: "screen-title" }, "Settings"),
      ),
      el("div", { class: "screen-actions" }, isDemo ? badge("browser demo", "warn") : null),
    ),
    el(
      "div",
      { class: "settings-grid" },
      panel({
        className: "area-updates",
        title: "Updates",
        body: [
          el(
            "div",
            { class: "kv-list" },
            kv("version", ctx.version() || "—"),
            kv("source", "github.com/msartore/OpenColorHub"),
          ),
          el("div", { class: "card-actions" }, checkButton),
        ],
      }),
      panel({
        className: "area-backend",
        title: "Backend",
        subtitle: "Where readings come from",
        body: [
          backendSwitch.el,
          backendBlurb,
          backendState,
        ],
      }),
      panel({
        className: "area-elevation",
        title: "Privileges",
        subtitle: "Runs elevated for tasks that need it",
        body: [elevationBody],
      }),
      panel({
        className: "area-poll",
        title: "Telemetry",
        subtitle: "How often the controller is polled",
        body: [pollSlider.el, smoothing.el],
      }),
      panel({
        className: "area-startup",
        title: "Startup",
        body: [autostartToggle.el, autostartNote],
      }),
      panel({
        className: "area-display",
        title: "Display",
        subtitle: "Units and accent colour",
        body: [
          el("div", { class: "field" }, el("span", { class: "field-label" }, "Temperature unit"), unitSwitch.el),
          el(
            "label",
            { class: "color-field" },
            accentInput,
            accentHex,
          ),
          accentPresets,
        ],
      }),
      panel({
        className: "area-safety",
        title: "Safety",
        subtitle: "Guard rails for real hardware",
        body: [confirmToggle.el, reapplyToggle.el, lowDuty.el],
      }),
      panel({
        className: "area-paths",
        title: "Files & paths",
        subtitle: "Where state and settings live",
        body: [pathsBody],
      }),
      panel({
        className: "area-danger",
        title: "Maintenance",
        subtitle: "Recovery actions",
        body: [
          el("div", { class: "card-actions" }, reloadState, resetButton),
          inlineNote("Resetting only touches preferences — fan curves and colours are left alone."),
        ],
      }),
    ),
  );

  const update = (snapshot: Snapshot) => {
    const fan = snapshot.fan_device;
    backendState.replaceChildren(
      el(
        "div",
        { class: "backend-card" },
        cardHead({
          title: fan.name,
          subtitle: fan.detail,
          right: [fan.simulated ? badge("simulated", "warn") : badge("hardware", "ok")],
        }),
        el(
          "div",
          { class: "kv-list" },
          kv("board", snapshot.board.identity?.name || "unknown"),
          kv("board vendor", snapshot.board.identity?.vendor || "—"),
          kv("board support", `${snapshot.board.confidence} · ${snapshot.board.source}`),
          kv("bios", snapshot.board.identity?.bios_version || "—"),
          kv("led device", snapshot.led_device.name),
          kv("temperature", snapshot.temperature_c === null ? "—" : `${snapshot.temperature_c.toFixed(1)} °C`),
        ),
      ),
    );

    const paths = apiPaths.value;
    pathsBody.replaceChildren(
      el("div", { class: "kv-row" }, kv("data dir", paths.dataDir), copyButton(paths.dataDir)),
      el("div", { class: "kv-row" }, kv("settings", paths.settings), copyButton(paths.settings)),
      el("div", { class: "kv-row" }, kv("lighting state", paths.state), copyButton(paths.state)),
    );
  };

  // Paths are static per process; the about payload is the cheapest source.
  const apiPaths: { value: { dataDir: string; settings: string; state: string; mode: string } } = {
    value: { dataDir: "—", settings: "—", state: "—", mode: "—" },
  };
  void api
    .about()
    .then((about) => {
      apiPaths.value = {
        dataDir: about.paths.data_dir,
        settings: about.paths.settings,
        state: about.paths.state,
        mode: about.build.profile,
      };
      const snap = ctx.snapshot();
      if (snap) update(snap);
    })
    .catch(() => undefined);

  const initial = ctx.snapshot();
  if (initial) update(initial);

  // Keep controls in sync when settings change elsewhere (e.g. reset).
  const syncFromSettings = () => {
    const settings = ctx.settings();
    pollSlider.setValue(settings.poll_interval_ms);
    smoothing.setChecked(settings.smoothing);
    confirmToggle.setChecked(settings.confirm_writes);
    reapplyToggle.setChecked(settings.reapply_led_on_launch);
    lowDuty.setValue(settings.low_duty_warning);
    unitSwitch.setValue(settings.temp_unit);
    backendSwitch.setValue(settings.backend);
    accentInput.value = settings.accent;
    accentHex.textContent = settings.accent.toUpperCase();
  };

  return {
    el: root,
    update: (snapshot) => {
      syncFromSettings();
      update(snapshot);
    },
  };
}
