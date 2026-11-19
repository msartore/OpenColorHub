/**
 * GPU LED rules mirrored from `crates/colorctl-core/src/gpu.rs`.
 * The effect ids are the controller's register values; 0x06 is unused.
 */

export const GPU_MODES: readonly [number, string][] = [
  [0x01, "Static"],
  [0x02, "Breathing"],
  [0x03, "Strobe"],
  [0x04, "RGB cycle"],
  [0x05, "Random"],
  [0x07, "Color shift"],
  [0x08, "Visor"],
  [0x09, "Stacking"],
  [0x0a, "Fill wave"],
  [0x0b, "Traveling wave"],
  [0x0c, "Marquee color"],
  [0x0d, "Marquee random"],
  [0x0e, "Color wave"],
  [0x0f, "Rainbow"],
];

export const GPU_DEFAULT_SPEED = 0x80;
export const GPU_DEFAULT_MODE = 0x01;

/** Brightness crosses the wire as 0–255, the slider as 0–100. */
export function gpuBrightnessToPercent(value: number): number {
  return Math.min(100, Math.max(0, Math.round((value * 100) / 255)));
}

export function gpuPercentToBrightness(percent: number): number {
  return Math.min(255, Math.max(0, Math.round((percent * 255) / 100)));
}
