// Render the pages of a scanned PDF to images so a vision model can read them (OCR).
// pdf.js is loaded only when needed.
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";

export interface PageImage {
  mime: string;
  data: string; // base64
}

export async function pdfPages(b64: string, maxPages = 8, width = 1400): Promise<{ pages: PageImage[]; total: number }> {
  const pdfjs = await import("pdfjs-dist");
  pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  const doc = await pdfjs.getDocument({ data: bytes }).promise;
  const pages: PageImage[] = [];
  const n = Math.min(doc.numPages, maxPages);
  for (let p = 1; p <= n; p++) {
    const page = await doc.getPage(p);
    const base = page.getViewport({ scale: 1 });
    const viewport = page.getViewport({ scale: Math.min(2.5, width / base.width) });
    const canvas = document.createElement("canvas");
    canvas.width = Math.ceil(viewport.width);
    canvas.height = Math.ceil(viewport.height);
    const ctx = canvas.getContext("2d")!;
    ctx.fillStyle = "#fff";
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    await page.render({ canvasContext: ctx, viewport, canvas }).promise;
    const url = canvas.toDataURL("image/jpeg", 0.82);
    pages.push({ mime: "image/jpeg", data: url.slice(url.indexOf(",") + 1) });
    page.cleanup();
  }
  const total = doc.numPages;
  await doc.loadingTask?.destroy?.();
  return { pages, total };
}

/** Read a browser File (drag & drop / paste) as base64. */
export function fileToBase64(f: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => {
      const s = String(r.result);
      resolve(s.slice(s.indexOf(",") + 1));
    };
    r.onerror = () => reject(r.error);
    r.readAsDataURL(f);
  });
}
