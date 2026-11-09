/** Small motion helpers shared by the UI. All of them respect "reduce motion". */

export const reduced = () => typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;

/**
 * Sliding selection pill for a `.segmented` control: the highlight moves to the active button
 * instead of jumping. Use as `<div class="segmented" use:pill>` with `class:on` on the buttons.
 */
export function pill(node: HTMLElement) {
  node.classList.add("pill");
  let first = true;
  const update = () => {
    const on = node.querySelector<HTMLElement>(":scope > button.on");
    if (!on) {
      node.style.setProperty("--pill-o", "0");
      return;
    }
    node.style.setProperty("--pill-x", `${on.offsetLeft}px`);
    node.style.setProperty("--pill-w", `${on.offsetWidth}px`);
    node.style.setProperty("--pill-o", "1");
    if (first) {
      // Place it without animating the very first time.
      first = false;
      node.classList.add("pill-init");
      requestAnimationFrame(() => requestAnimationFrame(() => node.classList.remove("pill-init")));
    }
  };
  const mo = new MutationObserver(update);
  mo.observe(node, { subtree: true, attributes: true, attributeFilter: ["class"], childList: true, characterData: true });
  const ro = new ResizeObserver(update);
  ro.observe(node);
  update();
  return {
    destroy() {
      mo.disconnect();
      ro.disconnect();
    },
  };
}

/**
 * Smoothly animates a number shown in an element (unread counts…): `<span use:countUp={n}>`.
 * The element's text is managed by the action.
 */
export function countUp(node: HTMLElement, value: number) {
  let shown = value;
  let raf = 0;
  node.textContent = String(value);
  return {
    update(next: number) {
      cancelAnimationFrame(raf);
      if (next === shown || reduced()) {
        shown = next;
        node.textContent = String(next);
        return;
      }
      const from = shown;
      const start = performance.now();
      const dur = Math.min(450, 120 + Math.abs(next - from) * 25);
      node.animate([{ transform: "scale(1.18)" }, { transform: "scale(1)" }], { duration: 260, easing: "cubic-bezier(.2,.8,.2,1)" });
      const step = (now: number) => {
        const k = Math.min(1, (now - start) / dur);
        const e = 1 - Math.pow(1 - k, 3);
        shown = Math.round(from + (next - from) * e);
        node.textContent = String(shown);
        if (k < 1) raf = requestAnimationFrame(step);
      };
      raf = requestAnimationFrame(step);
    },
    destroy() {
      cancelAnimationFrame(raf);
    },
  };
}

/**
 * Animate height changes of an element whose content grows or shrinks (`use:autoHeight`), so
 * blocks that appear inside it push the rest down smoothly instead of jumping.
 */
export function autoHeight(node: HTMLElement) {
  let last = node.offsetHeight;
  let anim: Animation | null = null;
  const ro = new ResizeObserver(() => {
    const h = node.offsetHeight;
    if (h === last || reduced()) {
      last = h;
      return;
    }
    anim?.cancel();
    anim = node.animate([{ height: `${last}px` }, { height: `${h}px` }], { duration: 200, easing: "cubic-bezier(.2,.8,.2,1)" });
    last = h;
  });
  ro.observe(node.firstElementChild ?? node);
  return { destroy: () => ro.disconnect() };
}

/**
 * Move an element to <body> (`use:portal`), so `position: fixed` is relative to the window even
 * inside transformed (animating) containers.
 */
export function portal(node: HTMLElement) {
  document.body.appendChild(node);
  return { destroy: () => node.remove() };
}
