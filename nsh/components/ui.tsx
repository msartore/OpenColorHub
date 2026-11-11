/** Small, unstyled-by-default primitives shared by the ported screens. */

import { Text, View } from "nsh";
import type { JSX } from "nsh";

export type Tone = "ok" | "warn" | "danger" | "muted";

const TONE_CLASS: Record<Tone, string> = {
  ok: "text-primary",
  warn: "text-[#fbbf24]",
  danger: "text-[#f87171]",
  muted: "text-muted-foreground",
};

export function Panel(props: {
  title?: string;
  subtitle?: string;
  actions?: JSX.Element;
  children?: JSX.Element;
}) {
  return (
    <View
      class="rounded-[10px] border border-border bg-card p-4 gap-3"
      style={{
        backgroundImage: "linear-gradient(180deg, rgba(255,255,255,0.028), transparent 55%)",
        boxShadow: "inset 0 1px 0 rgba(255,255,255,0.02), 0 18px 40px -32px rgba(0,0,0,0.95)",
      }}
    >
      <If when={props.title || props.subtitle || props.actions}>
        <View class="flex-row items-start justify-between gap-3">
          <View class="gap-0.5">
            <If when={props.title}>
              <Text class="text-sm font-semibold text-foreground">{props.title}</Text>
            </If>
            <If when={props.subtitle}>
              <Text class="text-xs text-muted-foreground">{props.subtitle}</Text>
            </If>
          </View>
          {props.actions}
        </View>
      </If>
      {props.children}
    </View>
  );
}

export function Stat(props: {
  label: string;
  value: string;
  unit?: string;
  hint?: string;
  tone?: Tone;
}) {
  return (
    <View
      class="rounded-[10px] border border-border bg-card p-3 gap-1"
      style={{
        backgroundImage: "linear-gradient(180deg, rgba(255,255,255,0.03), transparent 70%)",
      }}
    >
      <Text class="text-[11px] text-muted-foreground uppercase tracking-wide">{props.label}</Text>
      <View class="flex-row items-baseline gap-1">
        <Text
          class={`text-2xl font-semibold ${
            props.tone ? TONE_CLASS[props.tone] : "text-foreground"
          }`}
        >
          {props.value}
        </Text>
        <If when={props.unit}>
          <Text class="text-xs text-muted-foreground">{props.unit}</Text>
        </If>
      </View>
      <If when={props.hint}>
        <Text class="text-[11px] text-muted-foreground">{props.hint}</Text>
      </If>
    </View>
  );
}

export function Bar(props: { percent: number; color: string; thin?: boolean }) {
  return (
    <View class={`w-full rounded-full bg-muted ${props.thin ? "h-1" : "h-1.5"}`}>
      <View
        class={`rounded-full ${props.thin ? "h-1" : "h-1.5"}`}
        style={{
          width: `${Math.min(100, Math.max(0, props.percent))}%`,
          backgroundColor: props.color,
        }}
      />
    </View>
  );
}

export function Chip(props: { label: string; tone?: Tone }) {
  return (
    <View class="rounded-full border border-border px-2 py-0.5">
      <Text class={`text-[11px] ${TONE_CLASS[props.tone ?? "muted"]}`}>{props.label}</Text>
    </View>
  );
}

export function Notice(props: {
  tone: "warn" | "danger" | "muted";
  title: string;
  children?: JSX.Element;
}) {
  const border =
    props.tone === "danger"
      ? "border-[#f87171]/40"
      : props.tone === "warn"
        ? "border-[#fbbf24]/40"
        : "border-border";
  return (
    <View class={`rounded-[10px] border ${border} bg-card p-3 gap-1`}>
      <Text class={`text-xs font-semibold ${TONE_CLASS[props.tone === "muted" ? "muted" : props.tone]}`}>
        {props.title}
      </Text>
      {props.children}
    </View>
  );
}

export function KeyValue(props: { label: string; value: string; tone?: Tone }) {
  return (
    <View class="flex-row items-baseline justify-between gap-4">
      <Text class="text-xs text-muted-foreground">{props.label}</Text>
      <Text class={`text-xs font-mono ${TONE_CLASS[props.tone ?? "muted"]}`} style={{ flexShrink: 1 }}>
        {props.value}
      </Text>
    </View>
  );
}
