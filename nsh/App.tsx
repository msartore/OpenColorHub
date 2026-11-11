/**
 * Composition root of the NSH port. This is the only place that builds the
 * native adapter and the store; screens receive them, never a `Controller`.
 */

import { Text, View, mount, signal } from "nsh";
import { Application, Tray, currentWindow } from "nsh/desktop";

import { createDeps } from "./app/composition/deps";
import { Shell, type ScreenId, type Toast } from "./components/Shell";
import { About } from "./screens/About";
import { Dashboard } from "./screens/Dashboard";
import { Fans } from "./screens/Fans";
import { Lighting } from "./screens/Lighting";
import { Placeholder } from "./screens/Placeholder";
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

  const win = currentWindow();
  if (!deps.controller.minimized) win.show();
  win.on("closePress", () => win.hide());

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

  return (
    <Shell
      screen={screen()}
      version={deps.store.version()}
      demo={false}
      toasts={toasts()}
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
          <If
            when={
              screen() !== "dashboard" &&
              screen() !== "fans" &&
              screen() !== "leds" &&
              screen() !== "settings" &&
              screen() !== "about"
            }
          >
            <Placeholder screen={screen()} />
          </If>
        </If>
      </If>
    </Shell>
  );
}

mount(App);
