/**
 * One error type at the boundary between the native adapter, use cases and
 * screens. The Rust side answers with a message string; the adapter keeps it
 * and tags it, so a screen can branch on the kind instead of the prose.
 */

export type AppErrorKind =
  | "backend"
  | "starting"
  | "invalid_curve"
  | "invalid_name"
  | "unavailable";

export class AppError extends Error {
  readonly kind: AppErrorKind;

  constructor(kind: AppErrorKind, message: string) {
    super(message);
    this.name = "AppError";
    this.kind = kind;
  }
}

export function toAppError(error: unknown): AppError {
  if (error instanceof AppError) return error;
  const message = String(error);
  if (message.includes("still starting")) return new AppError("starting", message);
  if (message.includes("curve")) return new AppError("invalid_curve", message);
  return new AppError("backend", message);
}
