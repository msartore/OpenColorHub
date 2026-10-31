/**
 * Browser demo backend.
 *
 * Mirrors the Rust simulator closely enough that every screen can be developed
 * and tested in a plain browser (`npm run dev`) — and, importantly, so the UI
 * can be verified in CI/headless without a desktop shell. The running app never
 * shows this data on a machine with real hardware: it is only used when
 * `window.__TAURI_INTERNALS__` is missing.
 */

import type {
  ActionResult,
  BackendMode,
  ChannelId,
  ChannelStatus,
  CurvePoint,
  FanHeaderId,
  FanModeId,
  FanProfileId,
  FanStatus,
  AboutInfo,
  ActivityEntry,
  BoardInfo,
  Settings,
  SettingsPatch,
  Snapshot,
  TempUnit,
} from "./types";
import { curveFor } from "./curves";
import { toHex } from "./dom";

const HEADERS: {
  id: FanHeaderId;
  label: string;
  description: string;
  bank: number;
  span: number;
  base: number;
}[] = [
  { id: "cpu", label: "CPU_FAN", description: "CPU cooler / AIO radiator", bank: 2, span: 2100, base: 210 },
  { id: "cha_fan1", label: "CHA_FAN1", description: "Chassis intake", bank: 1, span: 1350, base: 210 },
  { id: "cha_fan2", label: "CHA_FAN2", description: "Chassis exhaust", bank: 10, span: 1350, base: 210 },
  { id: "cha_fan3", label: "CHA_FAN3", description: "Chassis auxiliary", bank: 3, span: 1350, base: 210 },
  { id: "pump", label: "AIO_PUMP", description: "AIO pump (keep ≥ 80%)", bank: 11, span: 2900, base: 750 },
];

const CHANNELS: {
  id: ChannelId;
  label: string;
  name: string;
  description: string;
  leds: number;
  start: number;
}[] = [
  { id: "led", label: "Onboard", name: "LED (Onboard)", description: "Chipset shroud + I/O cover, addressable", leds: 18, start: 0 },
  { id: "rgb1", label: "12V_1", name: "12V_1 (12V RGB Header 1)", description: "12V 4-pin, single-zone (G-R-B)", leds: 1, start: 18 },
  { id: "rgb2", label: "12V_2", name: "12V_2 (12V RGB Header 2)", description: "12V 4-pin, single-zone (G-R-B)", leds: 1, start: 19 },
  { id: "argb1", label: "5V_1", name: "5V_1 (ARGB Header 1)", description: "5V 3-pin addressable, per-LED", leds: 60, start: 20 },
  { id: "argb2", label: "5V_2", name: "5V_2 (ARGB Header 2)", description: "5V 3-pin addressable, per-LED", leds: 60, start: 80 },
  { id: "argb3", label: "5V_3", name: "5V_3 (ARGB Header 3)", description: "5V 3-pin addressable, per-LED", leds: 60, start: 140 },
];

const STORAGE_KEY = "opencolorhub.demo.v1";

/** The board the demo pretends to run on (the upstream reference model). */
const DEMO_BOARD: BoardInfo = {
  identity: {
    vendor: "Colorful Technology And Development Co.,LTD",
    name: "CVN B650M GAMING FROZEN V14",
    version: "1.0",
    bios_vendor: "American Megatrends International, LLC.",
    bios_version: "1.0.7 (demo)",
    system_vendor: "Colorful Technology And Development Co.,LTD",
    system_product: "CVN B650M GAMING FROZEN V14",
  },
  confidence: "verified",
  family: "CVN",
  series: "B650",
  matched: "CVN B650M GAMING FROZEN",
  catalog_size: 35,
  source: "demo",
  notes: ["CVN B650M GAMING FROZEN matched the verified catalogue (B650 series).", "BIOS 1.0.7 (demo)"],
};

export const pct = (p: number): number => Math.min(255, Math.round((Math.min(100, p) * 255 + 99) / 100));
export const pwmToPct = (pwm: number): number => Math.round((pwm * 100 + 127) / 255);

interface ChannelConfig {
  color: [number, number, number];
  brightness: number;
  enabled: boolean;
}

interface FanConfig {
  pwm: number;
  mode: FanModeId;
  curve: CurvePoint[] | null;
}

interface DemoState {
  fans: Record<FanHeaderId, FanConfig>;
  channels: Record<ChannelId, ChannelConfig>;
  settings: Settings;
  activity: ActivityEntry[];
}

export interface DemoBackend {
  invoke<T>(cmd: string, args: Record<string, unknown>): Promise<T>;
  subscribe(cb: (snapshot: Snapshot) => void): () => void;
}

const DEFAULT_SETTINGS: Settings = {
  backend: "simulated",
  poll_interval_ms: 1000,
  temp_unit: "celsius",
  smoothing: true,
  reapply_led_on_launch: false,
  confirm_writes: false,
  accent: "#4ade80",
  state_file: null,
  last_screen: "dashboard",
  low_duty_warning: 20,
};

function defaultState(): DemoState {
  const fans = {} as Record<FanHeaderId, FanConfig>;
  for (const h of HEADERS) fans[h.id] = { pwm: 128, mode: "manual", curve: null };
  const channels = {} as Record<ChannelId, ChannelConfig>;
  for (const c of CHANNELS) channels[c.id] = { color: [0, 0, 0], brightness: 0, enabled: false };
  return { fans, channels, settings: { ...DEFAULT_SETTINGS }, activity: [] };
}

function load(): DemoState {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return defaultState();
    const parsed = JSON.parse(raw) as Partial<DemoState>;
    const base = defaultState();
    return {
      fans: { ...base.fans, ...(parsed.fans ?? {}) },
      channels: { ...base.channels, ...(parsed.channels ?? {}) },
      settings: { ...base.settings, ...(parsed.settings ?? {}) },
      activity: parsed.activity ?? [],
    };
  } catch {
    return defaultState();
  }
}

/** Piecewise-linear curve evaluation, identical to `curve::eval`. */
export function evalCurve(curve: CurvePoint[], tempC: number): number {
  if (!curve.length) return 0;
  const first = curve[0];
  const last = curve[curve.length - 1];
  if (tempC <= first.temp_c) return first.percent;
  if (tempC >= last.temp_c) return last.percent;
  for (let i = 1; i < curve.length; i += 1) {
    const a = curve[i - 1];
    const b = curve[i];
    if (tempC >= a.temp_c && tempC <= b.temp_c) {
      const span = Math.max(1e-6, b.temp_c - a.temp_c);
      const t = (tempC - a.temp_c) / span;
      return Math.round(Math.min(100, Math.max(0, a.percent + t * (b.percent - a.percent))));
    }
  }
  return last.percent;
}

export function createDemoBackend(): DemoBackend {
  const state = load();
  const started = Date.now();
  const listeners = new Set<(snapshot: Snapshot) => void>();
  let timer: number | undefined;

  const persist = () => {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
    } catch {
      /* storage may be unavailable in private mode — demo continues in memory */
    }
  };

  const temperature = (): number => {
    const t = (Date.now() - started) / 1000;
    const drift = Math.sin(t / 42) * 6.5;
    const load = Math.max(0, Math.sin(t / 95)) * 9;
    return 37 + drift + load;
  };

  const effectivePwm = (header: FanHeaderId, tempC: number): number => {
    const cfg = state.fans[header];
    if (cfg.mode === "smart_fan" && cfg.curve) return pct(evalCurve(cfg.curve, tempC));
    return cfg.pwm;
  };

  const rpmFor = (header: (typeof HEADERS)[number], pwm: number): number => {
    const duty = pwm / 255;
    const wobble = 1 + Math.sin(Date.now() / 900 + header.base) * 0.004;
    return Math.round((header.base + header.span * duty * Math.sqrt(duty)) * wobble);
  };

  const fanStatuses = (tempC: number): FanStatus[] =>
    HEADERS.map((header) => {
      const pwm = effectivePwm(header.id, tempC);
      const cfg = state.fans[header.id];
      return {
        header: header.id,
        label: header.label,
        description: header.description,
        bank: header.bank,
        rpm: rpmFor(header, pwm),
        pwm,
        percent: pwmToPct(pwm),
        detected: true,
        mode: cfg.mode,
        curve: cfg.curve,
        curve_percent: cfg.curve ? evalCurve(cfg.curve, tempC) : null,
      };
    });

  const channelStatuses = (): ChannelStatus[] =>
    CHANNELS.map((channel) => {
      const cfg = state.channels[channel.id];
      const scaled: [number, number, number] = [
        Math.round((cfg.color[0] * cfg.brightness) / 100),
        Math.round((cfg.color[1] * cfg.brightness) / 100),
        Math.round((cfg.color[2] * cfg.brightness) / 100),
      ];
      return {
        id: channel.id,
        label: channel.label,
        name: channel.name,
        description: channel.description,
        leds: channel.leds,
        color: cfg.color,
        hex: toHex(cfg.color),
        render_hex: cfg.enabled ? toHex(scaled) : "#000000",
        brightness: cfg.brightness,
        enabled: cfg.enabled,
      };
    });

  const snapshot = (): Snapshot => {
    const tempC = temperature();
    const fans = fanStatuses(tempC);
    return {
      timestamp_ms: Date.now(),
      temperature_c: tempC,
      temperature_source: "synthetic",
      fans,
      channels: channelStatuses(),
      fan_device: {
        name: "Simulated Super I/O",
        detail: "Browser demo — no hardware is touched",
        path: "browser://demo",
        available: true,
        simulated: true,
        error: null,
      },
      led_device: {
        name: "Simulated RGB controller",
        detail: "Frames are kept in localStorage (2F4C:1000 not present)",
        path: "localStorage://opencolorhub.demo",
        available: true,
        simulated: true,
        error: null,
      },
      capabilities: {
        fan_control: true,
        led_control: true,
        notes: [
          "Browser demo mode: readings are synthetic.",
          "Run the desktop app on the target machine for real hardware access.",
        ],
      },
      avg_percent: Math.round(fans.reduce((sum, f) => sum + f.percent, 0) / fans.length),
      read_error: null,
      board: DEMO_BOARD,
    };
  };

  const record = (kind: string, message: string) => {
    state.activity.unshift({ at_ms: Date.now(), kind, message });
    state.activity = state.activity.slice(0, 40);
  };

  const result = (message: string): ActionResult => ({ snapshot: snapshot(), message });

  const clampDuty = (header: FanHeaderId, percent: number): number =>
    Math.min(100, Math.max(header === "pump" ? 60 : 0, percent));

  const invoke = async <T,>(cmd: string, args: Record<string, unknown>): Promise<T> => {
    switch (cmd) {
      case "get_snapshot":
        return snapshot() as T;

      case "get_board":
        return DEMO_BOARD as T;

      case "get_settings":
        return { ...state.settings } as T;

      case "get_elevation":
        return {
          elevated: false,
          can_request: false,
          detail: "Browser demo — elevation only exists in the desktop build.",
        } as T;

      case "request_elevation":
        return "declined" as T;

      case "get_autostart":
        return {
          supported: false,
          enabled: false,
          mechanism: "none",
          detail: "Browser demo — starting with the OS only exists in the desktop build.",
        } as T;

      case "set_autostart":
        return {
          supported: false,
          enabled: false,
          mechanism: "none",
          detail: "Browser demo — starting with the OS only exists in the desktop build.",
        } as T;

      case "save_settings": {
        const patch = (args.patch ?? {}) as SettingsPatch;
        state.settings = {
          ...state.settings,
          ...patch,
          poll_interval_ms: Math.min(10000, Math.max(250, patch.poll_interval_ms ?? state.settings.poll_interval_ms)),
        } as Settings;
        persist();
        schedule();
        return { ...state.settings } as T;
      }

      case "get_activity":
        return [...state.activity] as T;

      case "set_fan_percent": {
        const header = args.header as FanHeaderId;
        const percent = clampDuty(header, Math.round(Number(args.percent)));
        state.fans[header] = { pwm: pct(percent), mode: "manual", curve: null };
        persist();
        const label = HEADERS.find((h) => h.id === header)?.label ?? header;
        record("fan", `${label} set to PWM ${pct(percent)} (~${pwmToPct(pct(percent))}%)`);
        return result(`${label} set to PWM ${pct(percent)} (~${percent}%)`) as T;
      }

      case "set_fan_curve": {
        const header = args.header as FanHeaderId;
        const points = args.points as CurvePoint[];
        state.fans[header] = { pwm: state.fans[header].pwm, mode: "smart_fan", curve: points };
        persist();
        const label = HEADERS.find((h) => h.id === header)?.label ?? header;
        record("fan", `${label} curve set to ${points.map((p) => `${p.temp_c}:${p.percent}`).join(",")}`);
        return result(`${label} curve updated`) as T;
      }

      case "apply_profile": {
        const headers = (args.headers as FanHeaderId[]) ?? [];
        const profile = args.profile as string;
        for (const header of headers) {
          state.fans[header] = {
            pwm: state.fans[header].pwm,
            mode: "smart_fan",
            curve: curveFor(profile as FanProfileId, header === "pump"),
          };
        }
        persist();
        const message = `Applied '${profile}' profile to ${headers.length} headers`;
        record("fan", message);
        return result(message) as T;
      }

      case "set_channel": {
        const channel = args.channel as ChannelId;
        const color = (args.color as [number, number, number]) ?? [0, 0, 0];
        state.channels[channel] = {
          color,
          brightness: Math.round(Number(args.brightness ?? 100)),
          enabled: Boolean(args.enabled ?? true),
        };
        persist();
        const label = CHANNELS.find((c) => c.id === channel)?.label ?? channel;
        const message = state.channels[channel].enabled
          ? `${label} set to ${toHex(color)} at ${state.channels[channel].brightness}%`
          : `${label} turned off`;
        record("led", message);
        return result(message) as T;
      }

      case "set_all_channels": {
        const color = (args.color as [number, number, number]) ?? [0, 0, 0];
        const brightness = Math.round(Number(args.brightness ?? 100));
        const enabled = Boolean(args.enabled ?? true);
        for (const channel of CHANNELS) {
          state.channels[channel.id] = enabled
            ? { color, brightness, enabled: true }
            : { color: [0, 0, 0], brightness: 0, enabled: false };
        }
        persist();
        const message = enabled
          ? `All channels set to ${toHex(color)} at ${brightness}%`
          : "All lighting turned off";
        record("led", message);
        return result(message) as T;
      }

      case "set_brightness": {
        const brightness = Math.round(Number(args.percent ?? 0));
        for (const channel of CHANNELS) {
          if (state.channels[channel.id].enabled) state.channels[channel.id].brightness = brightness;
        }
        persist();
        const message = `Brightness set to ${brightness}%`;
        record("led", message);
        return result(message) as T;
      }

      case "turn_off_channel": {
        const channel = args.channel as ChannelId;
        state.channels[channel] = { color: [0, 0, 0], brightness: 0, enabled: false };
        persist();
        const label = CHANNELS.find((c) => c.id === channel)?.label ?? channel;
        record("led", `${label} turned off`);
        return result(`${label} turned off`) as T;
      }

      case "apply_led":
        persist();
        return result("Lighting applied") as T;

      case "open_external":
        return undefined as T;

      case "get_drivers": {
        // Shape mirrors the real catalogue so the view can be developed in a
        // plain browser without touching the network.
        const files = [
          {
            title: "CVN X570M GAMING PRO V14 BIOS 1016",
            kind: "bios" as const,
            url: "https://endownload.colorful.cn/demo/bios-1016.zip",
            file_name: "CVN X570M BIOS 1016.zip",
            published: "2026-07-20",
            newer: true,
          },
          {
            title: "AMD Chipset Driver",
            kind: "driver" as const,
            url: "https://endownload.colorful.cn/demo/chipset.exe",
            file_name: "amd_chipset_software_8.02.18.557.exe",
            published: "2026-03-01",
            newer: false,
          },
          {
            title: "CVN X570M GAMING PRO V14",
            kind: "manual" as const,
            url: "https://endownload.colorful.cn/demo/manual.pdf",
            file_name: "CVN X570M GAMING PRO V14.pdf",
            published: "2020-05-27",
            newer: false,
          },
        ];
        return {
          board: "CVN X570M GAMING PRO V14",
          product_id: 880,
          installed_bios: "1015",
          files,
          fetched_at_ms: Date.now(),
          cached: false,
          warning: null,
        } as T;
      }

      case "download_driver": {
        const file = (args.file ?? {}) as { file_name?: string };
        await new Promise((resolve) => setTimeout(resolve, 400));
        return {
          path: "C:\\Users\\demo\\drivers\\" + (file.file_name ?? "download.bin"),
          bytes: 15_461_350,
          sha256: "0".repeat(64),
        } as T;
      }

      case "reveal_driver":
        return undefined as T;

      case "list_setups": {
        const kind = (args.kind as "fans" | "lighting") ?? "fans";
        const fans = [
          { header: "cpu" as const, percent: 40, curve: null },
          { header: "cha_fan1" as const, percent: 30, curve: null },
          { header: "pump" as const, percent: 80, curve: null },
        ];
        const channels = [
          { channel: "led" as const, color: [251, 191, 36] as [number, number, number], brightness: 40, enabled: true },
        ];
        return {
          setups: [
            {
              id: kind === "fans" ? "s1-f" : "s1-l",
              name: kind === "fans" ? "Night" : "Amber",
              kind,
              saved_at_ms: Date.now() - 86_400_000,
              fans: kind === "fans" ? fans : [],
              channels: kind === "lighting" ? channels : [],
            },
          ],
          warning: null,
        } as T;
      }

      case "save_setup":
      case "delete_setup":
        return { setups: [], warning: null } as T;

      case "apply_setup":
        return result("Setup applied") as T;

      case "get_about": {
        const about: AboutInfo = {
          name: "OpenColorHub",
          version: "0.2.0",
          tagline: "Fan and lighting control for Colorful motherboards",
          license: "MIT",
          build: {
            version: "0.2.0",
            tauri: null,
            rustc: "browser demo",
            profile: "debug",
            os: "browser",
            arch: "wasm",
            family: "unix",
          },
          upstream: {
            name: "colorctl",
            url: "https://github.com/Curstantine/colorctl",
            author: "Curstantine",
            license: "MIT",
          },
          chip: "Nuvoton NCT5584D",
          chip_id: 0xd42a,
          vid: 0x2f4c,
          pid: 0x1000,
          total_leds: 200,
          board: DEMO_BOARD,
          boards: [
            "CVN B650M Gaming Frozen V14 (reference board)",
            "CVN B850M ARK FROZEN V14",
            "CVN B850M GAMING FROZEN V14",
            "CVN X870 GAMING FROZEN V14",
            "iGame B850I MINI OC V14",
            "BATTLE-AX B850M-PLUS WIFI V14",
            "COLORFIRE B850M-A MEOW WIFI ORANGE",
          ],
          headers: HEADERS.map((h) => ({
            id: h.id,
            label: h.label,
            bank: h.bank,
            description: h.description,
          })),
          channels: CHANNELS.map((c) => ({
            id: c.id,
            label: c.label,
            leds: c.leds,
            range: `${c.start}..${c.start + c.leds}`,
            description: c.description,
          })),
          capabilities: [
            "5 PWM fan headers with 4-point SmartFan curves",
            "6 lighting channels across 200 LEDs",
            "Manual duty, built-in profiles, live tachometers",
            "State compatible with upstream colorctl (CCTL v1)",
            "Linux hardware backends plus a cross-platform simulator",
          ],
          paths: {
            data_dir: "browser://demo",
            settings: "localStorage://opencolorhub.demo",
            state: "localStorage://opencolorhub.demo",
            config_override: null,
          },
          links: [
            { label: "opencolorhub on GitHub", url: "https://github.com/msartore/OpenColorHub", kind: "primary" },
            { label: "Upstream colorctl", url: "https://github.com/Curstantine/colorctl", kind: "secondary" },
            { label: "Tauri", url: "https://tauri.app", kind: "secondary" },
          ],
        };
        return about as T;
      }

      default:
        throw new Error(`demo backend: unknown command '${cmd}'`);
    }
  };

  const tick = () => {
    if (!listeners.size) return;
    const snap = snapshot();
    for (const cb of listeners) cb(snap);
  };

  const schedule = () => {
    if (timer !== undefined) window.clearInterval(timer);
    timer = window.setInterval(tick, Math.max(250, state.settings.poll_interval_ms));
  };

  schedule();

  return {
    invoke,
    subscribe(cb) {
      listeners.add(cb);
      cb(snapshot());
      return () => listeners.delete(cb);
    },
  };
}

/** Exposed for tests: default settings used by the demo. */
export const demoDefaults = DEFAULT_SETTINGS;
export type { Settings as DemoSettings, TempUnit as DemoTempUnit, BackendMode as DemoBackendMode };
