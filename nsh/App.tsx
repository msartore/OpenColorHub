/**
 * Composition root of the NSH port. This is the only place that builds the
 * native adapter and the store; screens receive them, never a `Controller`.
 */

import { Text, View, Storage, cleanup, effect, keymap, mount, signal } from "nsh";
import { Application, Tray, Window, currentWindow } from "nsh/desktop";
import { AlertDialog } from "nsh/base";

import { createDeps } from "./app/composition/deps";
import { applyAccent } from "./app/presentation/theme";
import { shouldOfferElevation } from "./app/application/usecases/system";
import { SCREENS, Shell, type ScreenId, type Toast } from "./components/Shell";
import { About } from "./screens/About";
import { Dashboard } from "./screens/Dashboard";
import { Drivers } from "./screens/Drivers";
import { Fans } from "./screens/Fans";
import { Lighting } from "./screens/Lighting";
import { SettingsScreen } from "./screens/Settings";
import trayIcon from "./assets/icon.png";

const TONE_ORDER: Toast["tone"][] = ["ok", "warn", "danger", "muted"];

function App() {
  const deps = createDeps();
  const screen = signal<ScreenId>("dashboard");
  const toasts = signal<Toast[]>([]);
  let toastId = 0;

  const toast = (message: string, tone: Toast["tone"] = "muted"): void => {
    const entry = { id: ++toastId, message, tone: TONE_ORDER.includes(tone) ? tone : "muted" };
    toasts.set([...toasts(), entry]);
    setTimeout(() => {
      toasts.set(toasts().filter((candidate) => candidate.id !== entry.id));
    }, 4200);
  };

  const navigate = (next: ScreenId): void => {
    screen.set(next);
    if (deps.store.settings().last_screen !== next) {
      void deps.store.saveSettings({ last_screen: next }).catch(() => {
        // Navigation is not worth a toast; the next launch falls back to Dashboard.
      });
    }
  };

  // The accent is a runtime theme: follow the setting wherever it changes,
  // including the first snapshot from the settings file.
  effect(() => {
    applyAccent(deps.store.settings().accent);
  });

  // Restore the last screen once settings have loaded.
  let restored = false;
  effect(() => {
    if (!deps.store.loaded() || restored) return;
    restored = true;
    const saved = deps.store.settings().last_screen;
    if (SCREENS.some((entry) => entry.id === saved) && saved !== screen()) {
      screen.set(saved as ScreenId);
    }
  });

  /**
   * Ask before elevating. The installer is per-user and intentionally does not
   * request administrator; PawnIO's driver does need it, so the user consents
   * here and Windows shows its own UAC prompt on the relaunch. A simulated
   * backend is never asked: it has nothing to gain from the driver.
   */
  const offerElevation = async (): Promise<void> => {
    if (deps.store.settings().backend === "simulated") return;
    try {
      const [elevation, pawnio] = await Promise.all([
        deps.store.system.elevation(),
        deps.store.pawnio.status(),
      ]);
      if (!shouldOfferElevation(elevation, pawnio)) return;

      const answer = await AlertDialog.show({
        title: "Fan control needs administrator rights",
        description:
          "PawnIO is installed, but this process cannot open its driver. Lighting, settings and the dashboard work without it. Restart OpenColorHub as administrator to read fans and write curves.",
        actions: [
          { id: "continue", label: "Continue without fan control", role: "cancel" },
          { id: "restart", label: "Restart as administrator", role: "default" },
        ],
      });
      if (answer.action !== "restart") return;

      const outcome = await deps.store.system.requestElevation();
      if (outcome === "launched") {
        // The elevated copy takes over; this process must not keep the
        // single-instance lock or the driver handle.
        Application.quit();
      } else {
        toast("Elevation cancelled — still running as a standard user", "warn");
      }
    } catch (error) {
      toast(String(error), "danger");
    }
  };

  let elevationAsked = false;
  effect(() => {
    if (!deps.store.loaded() || elevationAsked) return;
    elevationAsked = true;
    void offerElevation();
  });

  const shortcuts = SCREENS.map((entry, index) =>
    keymap.bind(`mod+${index + 1}`, () => navigate(entry.id)),
  );
  cleanup(() => {
    for (const stop of shortcuts) stop();
  });

  const win = currentWindow();
  const main = Window.get("main") ?? win;
  if (!deps.controller.minimized) win.show();
  win.on("closePress", () => win.hide());

  // Restore where the user left the window, and keep that placement current.
  void Storage.getItem("window").then((saved) => {
    if (!saved) return;
    try {
      main.setPlacement(JSON.parse(saved));
    } catch {
      // A placement from another display layout is simply ignored.
    }
  });
  effect(() => {
    const state = main.state();
    if (state) void Storage.setItem("window", JSON.stringify(state.placement));
  });

  Tray.set({
    icon: trayIcon,
    tooltip: "OpenColorHub",
    onClick: () => win.focus(),
    menu: [
      { label: "Open", onSelect: () => win.focus() },
      { type: "separator" },
      { role: "quit" },
    ],
  });

  // A second launch brings the running app forward unless it came from the
  // autostart entry, which carries `--minimized`.
  Application.on("secondInstance", ({ args }) => {
    if (!args.includes("--minimized")) win.focus();
  });
  Application.on("quit", () => deps.store.dispose());

  const snapshot = () => deps.store.snapshot();
  const status = () => ({
    fans: snapshot()?.fan_device.simulated ? "simulated fan" : (snapshot()?.fan_device.name ?? "probing"),
    lighting: snapshot()?.led_device.available ? "led · ok" : (snapshot()?.led_device.error ?? "probing"),
  });

  return (
    <Shell
      screen={screen()}
      version={deps.store.version()}
      demo={false}
      toasts={toasts()}
      status={status()}
      onNavigate={navigate}
    >
      <If when={deps.store.error()}>
        {(error) => (
          <View class="gap-3 p-8">
            <Text class="text-xl font-semibold text-[#f87171]">
              OpenColorHub could not start
            </Text>
            <Text class="text-sm text-muted-foreground">
              The hardware probe or the settings file failed. The original message is below.
            </Text>
            <Text class="text-xs font-mono text-muted-foreground">{error()}</Text>
          </View>
        )}
      </If>

      <If when={!deps.store.error()}>
        <If
          when={deps.store.loaded()}
          else={
            <View class="p-8">
              <Text class="text-sm text-muted-foreground">
                Probing the hardware — the first reading takes a moment…
              </Text>
            </View>
          }
        >
          <If when={screen() === "dashboard"}>
            <Dashboard store={deps.store} />
          </If>
          <If when={screen() === "fans"}>
            <Fans store={deps.store} toast={toast} />
          </If>
          <If when={screen() === "leds"}>
            <Lighting store={deps.store} toast={toast} />
          </If>
          <If when={screen() === "settings"}>
            <SettingsScreen store={deps.store} toast={toast} />
          </If>
          <If when={screen() === "about"}>
            <About store={deps.store} toast={toast} />
          </If>
          <If when={screen() === "drivers"}>
            <Drivers store={deps.store} toast={toast} />
          </If>
        </If>
      </If>
    </Shell>
  );
}

mount(App);
