/**
 * Self-update: ask GitHub for the newest release, offer it, download and swap.
 *
 * Deliberately not the Tauri updater plugin — that needs signing keys, a
 * `latest.json` on every release and an installer re-run. The release already
 * ships a bare `OpenColorHub.exe`, so we download that, verify it against
 * `SHA256SUMS.txt` and let Rust rename-and-swap the running image.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { button } from "./components/ui";
import { el, inTauri } from "./dom";

export interface UpdateStatus {
  current: string;
  latest: string | null;
  available: boolean;
  notes: string | null;
  asset_url: string | null;
  error: string | null;
}

interface Progress {
  received: number;
  total: number | null;
}

/** Returns `null` outside a Tauri build (browser demo has no updater). */
export async function checkForUpdate(): Promise<UpdateStatus | null> {
  if (!inTauri()) return null;
  try {
    return await invoke<UpdateStatus>("update_check");
  } catch {
    return null;
  }
}

function formatBytes(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

/**
 * Modal offering the update. Resolves when it closes; installing exits the app.
 */
export function showUpdateDialog(status: UpdateStatus): Promise<void> {
  return new Promise((resolve) => {
    let unlisten: (() => void) | null = null;

    const close = () => {
      unlisten?.();
      backdrop.remove();
      resolve();
    };

    const statusLine = el("p", { class: "update-status" }, "Ready to download");
    const progressFill = el("span", { class: "update-progress-fill" });
    const progress = el("div", { class: "update-progress" }, progressFill);
    progress.style.display = "none";
    statusLine.style.display = "none";

    const later = button("Later", { variant: "quiet", onClick: close });
    const install = button("Download & install", {
      variant: "primary",
      onClick: async () => {
        install.disabled = true;
        later.disabled = true;
        statusLine.style.display = "";
        progress.style.display = "";

        unlisten = await listen<Progress>("update://progress", (event) => {
          const { received, total } = event.payload;
          if (total) {
            const pct = Math.min(100, Math.round((received / total) * 100));
            progressFill.style.width = `${pct}%`;
            statusLine.textContent = `Downloading… ${pct}% of ${formatBytes(total)}`;
          } else {
            statusLine.textContent = `Downloading… ${formatBytes(received)}`;
          }
        });

        try {
          await invoke("update_install");
          statusLine.textContent = "Restarting…";
        } catch (err) {
          statusLine.textContent = String(err);
          statusLine.classList.add("is-error");
          install.disabled = false;
          later.disabled = false;
          progress.style.display = "none";
        }
      },
    });

    const notes = status.notes
      ? el("pre", { class: "update-notes" }, status.notes.slice(0, 900))
      : null;

    const backdrop = el(
      "div",
      { class: "update-backdrop", role: "dialog", "aria-modal": "true" },
      el(
        "div",
        { class: "update-card" },
        el("h2", { class: "update-title" }, "Update available"),
        el(
          "p",
          { class: "update-lead" },
          `OpenColorHub ${status.latest?.replace(/^v/, "") ?? ""} is ready — you're on ${status.current}.`,
        ),
        notes,
        statusLine,
        progress,
        el("div", { class: "update-actions" }, later, install),
      ),
    );

    backdrop.addEventListener("click", (event) => {
      if (event.target === backdrop) close();
    });
    document.body.appendChild(backdrop);
    install.focus();
  });
}

/** Startup hook: quietly checks once, then offers anything newer. */
export async function maybeOfferUpdate(): Promise<void> {
  const status = await checkForUpdate();
  if (status?.available) void showUpdateDialog(status);
}
