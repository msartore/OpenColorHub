/** Small, unstyled-by-default primitives shared by the ported screens. */

import { Canvas, Text, View } from "nsh";
import type { JSX } from "nsh";

import { boardDisplayName } from "../app/domain/telemetry";
import type { BoardInfo } from "../app/domain/model";

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

/** The title row every screen shares, with an optional right-hand action. */
export function PageHeader(props: {
  title: string;
  subtitle?: string;
  actions?: JSX.Element;
}) {
  return (
    <View
      class="flex-row items-start justify-between gap-4"
      style={{ paddingTop: 2, paddingBottom: 2 }}
    >
      <View class="gap-1" style={{ flexShrink: 1 }}>
        <Text class="text-[25px] font-semibold text-foreground" style={{ letterSpacing: -0.5 }}>
          {props.title}
        </Text>
        <If when={props.subtitle}>
          <Text class="text-xs text-muted-foreground">{props.subtitle}</Text>
        </If>
      </View>
      <If when={props.actions}>{props.actions}</If>
    </View>
  );
}

const CONFIDENCE_DOT: Record<string, string> = {
  verified: "#4ade80",
  compatible: "#7dd3fc",
  unknown: "#fbbf24",
  unsupported: "#f87171",
};

/** Board + detection confidence badge shown on the Dashboard header. */
export function BoardBadge(props: { board: BoardInfo }) {
  const confidence = props.board.confidence;
  return (
    <View
      class="flex-row items-center gap-2 rounded-full border border-[#fbbf24]/40 bg-card px-3 py-1"
      style={{ alignSelf: "flex-start" }}
    >
      <View
        class="rounded-full"
        style={{ width: 6, height: 6, backgroundColor: CONFIDENCE_DOT[confidence] ?? "#fbbf24" }}
      />
      <Text class="text-[11px] font-mono uppercase text-foreground">
        {`CVN ${boardDisplayName(props.board)}`}
      </Text>
      <Text class="text-[10px] uppercase text-muted-foreground">{confidence}</Text>
    </View>
  );
}

/** The 200-LED strip the old Dashboard and Lighting cards used. */
export function LedStrip(props: { cells: readonly string[]; height?: number }) {
  return (
    <Canvas
      style={{ height: props.height ?? 26, width: "100%" }}
      draw={(cx, size) => {
        const cells = props.cells;
        const count = Math.max(1, cells.length);
        const cellWidth = size.width / count;
        for (let index = 0; index < count; index += 1) {
          cx.fillStyle = cells[index] ?? "#000000";
          cx.fillRect(index * cellWidth, 0, Math.max(1, cellWidth - 1), size.height);
        }
      }}
    />
  );
}
