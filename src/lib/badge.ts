/**
 * Unread count on the taskbar button (Windows overlay icon) and the tray icon. Windows has no
 * numeric badge for desktop apps, so the badge is drawn here and handed over as small images.
 */
import { api } from "./api";

export type BadgeMode = "count" | "dot" | "off";
let last = "";
let logo: HTMLImageElement | null = null;

function loadLogo(): Promise<HTMLImageElement | null> {
  if (logo) return Promise.resolve(logo);
  return new Promise((res) => {
    const img = new Image();
    img.onload = () => res((logo = img));
    img.onerror = () => res(null);
    img.src = "/logo.png";
  });
}

const label = (n: number) => (n > 99 ? "99+" : String(n));

/** A red badge filling a `size`² square at (x, y). */
function drawBadge(g: CanvasRenderingContext2D, x: number, y: number, size: number, n: number, mode: BadgeMode) {
  const r = size / 2;
  g.beginPath();
  g.arc(x + r, y + r, r - 0.5, 0, Math.PI * 2);
  g.fillStyle = "#ffffff";
  g.fill();
  g.beginPath();
  g.arc(x + r, y + r, r - Math.max(1.5, size * 0.07), 0, Math.PI * 2);
  g.fillStyle = "#d92d3a";
  g.fill();
  if (mode !== "count") return;
  const s = label(n);
  const k = s.length === 1 ? 0.66 : s.length === 2 ? 0.54 : 0.4;
  g.fillStyle = "#ffffff";
  g.font = `700 ${Math.round(size * k)}px "Segoe UI Variable Display", "Segoe UI", Arial, sans-serif`;
  g.textAlign = "center";
  g.textBaseline = "middle";
  g.fillText(s, x + r, y + r + size * 0.04);
}

async function png(c: HTMLCanvasElement): Promise<number[]> {
  const blob: Blob | null = await new Promise((res) => c.toBlob(res, "image/png"));
  return blob ? Array.from(new Uint8Array(await blob.arrayBuffer())) : [];
}

export async function updateBadge(total: number, mode: BadgeMode) {
  const key = `${mode}:${mode === "off" ? 0 : total}`;
  if (key === last) return;
  last = key;
  try {
    if (mode === "off" || total <= 0) {
      await api.setUnreadBadge(mode === "off" ? 0 : total, null, null);
      return;
    }
    // Taskbar overlay (Windows draws it at the button's bottom-right corner).
    const o = document.createElement("canvas");
    o.width = o.height = 32;
    drawBadge(o.getContext("2d")!, 0, 0, 32, total, mode);
    // Tray: the logo with the badge in its top-right corner.
    const t = document.createElement("canvas");
    t.width = t.height = 32;
    const g = t.getContext("2d")!;
    const img = await loadLogo();
    if (img) g.drawImage(img, 0, 0, 32, 32);
    drawBadge(g, mode === "count" ? 13 : 17, 0, mode === "count" ? 19 : 15, total, mode);
    await api.setUnreadBadge(total, await png(o), await png(t));
  } catch {
    last = "";
  }
}
