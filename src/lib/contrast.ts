/** Finding text that would be (nearly) invisible on what's behind it. */

/** CSS colour (computed rgb/rgba) → relative luminance (WCAG); null if (mostly) transparent. */
export function lum(c: string): number | null {
  const m = c.match(/rgba?\(([\d.]+)[, ]+([\d.]+)[, ]+([\d.]+)(?:[,/ ]+([\d.]+))?/);
  if (!m) return null;
  if (m[4] !== undefined && parseFloat(m[4]) < 0.5) return null;
  const ch = (v: string) => {
    const x = parseFloat(v) / 255;
    return x <= 0.03928 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * ch(m[1]) + 0.7152 * ch(m[2]) + 0.0722 * ch(m[3]);
}

/**
 * Calls `fix(el, darkText)` for each element under `root` whose own text has too little contrast
 * with its background (`darkText`: the background is light, so the text should be dark).
 * Text over a picture is left alone. `pageBg` is the luminance behind `root`.
 */
export function lowContrast(root: Element, pageBg: number, fix: (el: HTMLElement, darkText: boolean) => void, limit = 6000) {
  const win = root.ownerDocument.defaultView;
  if (!win) return;
  const cache = new Map<Element, number>();
  const bgOf = (el: Element | null): number => {
    if (!el || el === root.parentElement) return pageBg;
    const hit = cache.get(el);
    if (hit !== undefined) return hit;
    const cs = win.getComputedStyle(el);
    const v = cs.backgroundImage !== "none" ? -1 : (lum(cs.backgroundColor) ?? (el === root ? pageBg : bgOf(el.parentElement)));
    cache.set(el, v);
    return v;
  };
  let n = 0;
  for (const el of [root, ...Array.from(root.querySelectorAll("*"))]) {
    if (++n > limit) break;
    if (!Array.from(el.childNodes).some((c) => c.nodeType === 3 && c.textContent!.trim())) continue;
    const fg = lum(win.getComputedStyle(el).color);
    const bg = bgOf(el);
    if (fg === null || bg < 0) continue;
    if ((Math.max(fg, bg) + 0.05) / (Math.min(fg, bg) + 0.05) < 2.6) fix(el as HTMLElement, bg > 0.35);
  }
}
