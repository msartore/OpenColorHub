/** The app frame: a sidebar, the scrolling screen area, and the toast stack. */

import { Image, Scroll, Text, View } from "nsh";
import type { JSX } from "nsh";
import { Button } from "nsh/base";

import logo from "../assets/icon.png";
import { layoutSize, sidebarWidth } from "../app/presentation/layout";

export type ScreenId = "dashboard" | "fans" | "leds" | "drivers" | "settings" | "about";

export interface ScreenMeta {
  id: ScreenId;
  label: string;
}

export const SCREENS: readonly ScreenMeta[] = [
  { id: "dashboard", label: "Dashboard" },
  { id: "fans", label: "Fan control" },
  { id: "leds", label: "Lighting" },
  { id: "drivers", label: "Drivers" },
  { id: "settings", label: "Settings" },
  { id: "about", label: "About" },
];

export interface Toast {
  id: number;
  message: string;
  tone: "ok" | "warn" | "danger" | "muted";
}

const TOAST_COLOR: Record<Toast["tone"], string> = {
  ok: "text-primary",
  warn: "text-[#fbbf24]",
  danger: "text-[#f87171]",
  muted: "text-muted-foreground",
};

export function Shell(props: {
  screen: ScreenId;
  version: string;
  demo: boolean;
  toasts: readonly Toast[];
  onNavigate: (screen: ScreenId) => void;
  children?: JSX.Element;
}) {
  const wide = () => layoutSize() !== "narrow";

  return (
    <View class="flex-1 flex-row bg-background">
      <View
        class="border-r border-border bg-sidebar py-4 gap-4"
        style={{
          width: sidebarWidth(),
          paddingLeft: wide() ? 12 : 6,
          paddingRight: wide() ? 12 : 6,
          alignItems: wide() ? "stretch" : "center",
        }}
      >
        <View
          class="flex-row items-center gap-2"
          style={{ justifyContent: wide() ? "flex-start" : "center", paddingLeft: wide() ? 4 : 0 }}
        >
          <Image src={logo} style={{ width: 28, height: 28, borderRadius: 7 }} />
          <If when={wide()}>
            <View class="gap-0">
              <Text class="text-sm font-semibold text-foreground">OpenColorHub</Text>
              <Text class="text-[11px] text-muted-foreground">fan + ARGB control</Text>
            </View>
          </If>
        </View>

        <View class="gap-1">
          <Each of={SCREENS} key={(item) => item.id}>
            {(item) => (
              <Button
                class={
                  props.screen === item().id
                    ? "rounded-[7px] bg-sidebar-accent text-foreground"
                    : "rounded-[7px] text-muted-foreground"
                }
                style={{
                  flexDirection: "row",
                  alignItems: "center",
                  justifyContent: wide() ? "flex-start" : "center",
                  gap: 8,
                  paddingLeft: wide() ? 10 : 0,
                  paddingRight: wide() ? 10 : 0,
                  paddingTop: 8,
                  paddingBottom: 8,
                }}
                onPress={() => props.onNavigate(item().id)}
                accessibilityLabel={item().label}
              >
                <Text class="text-sm">{wide() ? item().label : item().label.slice(0, 1)}</Text>
              </Button>
            )}
          </Each>
        </View>

        <View style={{ flex: 1 }} />

        <If when={wide()}>
          <View class="gap-1 px-1">
            <If when={props.demo}>
              <Text class="text-[11px] text-[#fbbf24]">browser demo</Text>
            </If>
            <Text class="text-[11px] font-mono text-muted-foreground">
              {props.version ? `v${props.version}` : "—"}
            </Text>
            <Text class="text-[11px] text-muted-foreground">GPL-3.0</Text>
          </View>
        </If>
      </View>

      <View style={{ flex: 1 }}>
        <Scroll class="flex-1">
          <View
            style={{
              width: "100%",
              maxWidth: 1180,
              alignSelf: "center",
              paddingLeft: wide() ? 24 : 14,
              paddingRight: wide() ? 24 : 14,
              paddingTop: 20,
              paddingBottom: 40,
              gap: 16,
            }}
          >
            {props.children}
          </View>
        </Scroll>
      </View>

      <View
        style={{
          position: "absolute",
          left: 16,
          right: 16,
          bottom: 16,
          gap: 8,
          alignItems: "flex-end",
        }}
      >
        <Each of={props.toasts} key={(toast) => toast.id}>
          {(toast) => (
            <View class="rounded-[10px] border border-border bg-popover px-3 py-2">
              <Text class={`text-xs ${TOAST_COLOR[toast().tone]}`}>{toast().message}</Text>
            </View>
          )}
        </Each>
      </View>
    </View>
  );
}
