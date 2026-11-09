<script lang="ts" module>
  // Outline icons on a 20×20 grid, stroked with currentColor (Fluent-like).
  export const ICONS: Record<string, string> = {
    info: "M10 2.75a7.25 7.25 0 1 0 0 14.5 7.25 7.25 0 0 0 0-14.5Z M10 9v4.75 M10 6.4v.1",
    help: "M10 2.75a7.25 7.25 0 1 0 0 14.5 7.25 7.25 0 0 0 0-14.5Z M7.9 7.9a2.1 2.1 0 0 1 4.1.6c0 1.4-2 1.7-2 3 M10 14v.1",
    bell: "M10 3a4.5 4.5 0 0 0-4.5 4.5v3L4 13.5h12l-1.5-3v-3A4.5 4.5 0 0 0 10 3Z M8.2 16a1.9 1.9 0 0 0 3.6 0",
    copy: "M7 6.5V4.5a1 1 0 0 1 1-1h7a1 1 0 0 1 1 1v8a1 1 0 0 1-1 1h-2 M4.5 6.5h7a1 1 0 0 1 1 1v8a1 1 0 0 1-1 1h-7a1 1 0 0 1-1-1v-8a1 1 0 0 1 1-1Z",
    sparkle: "M10 2.5l1.7 4.3 4.3 1.7-4.3 1.7L10 14.5l-1.7-4.3L4 8.5l4.3-1.7L10 2.5Z M15.5 13l.7 1.8 1.8.7-1.8.7-.7 1.8-.7-1.8-1.8-.7 1.8-.7.7-1.8Z",
    zoomIn: "M8.5 14.5a6 6 0 1 0 0-12 6 6 0 0 0 0 12Z M13 13l4.5 4.5 M8.5 6v5 M6 8.5h5",
    zoomOut: "M8.5 14.5a6 6 0 1 0 0-12 6 6 0 0 0 0 12Z M13 13l4.5 4.5 M6 8.5h5",
    history: "M3.5 10a6.5 6.5 0 1 0 1.9-4.6 M3.5 3.5v3h3 M10 6.5V10l2.5 1.5",
    brain: "M7.5 4a2.5 2.5 0 0 0-2.4 3.2A2.7 2.7 0 0 0 4 11.5a2.6 2.6 0 0 0 2 3A2.5 2.5 0 0 0 10 16V5.5A2 2 0 0 0 7.5 4Z M12.5 4a2.5 2.5 0 0 1 2.4 3.2 2.7 2.7 0 0 1 1.1 4.3 2.6 2.6 0 0 1-2 3A2.5 2.5 0 0 1 10 16 M7 9h1.5 M11.5 11H13",
    tag: "M3 4v5.2L10.8 17 17 10.8 9.2 3H4a1 1 0 0 0-1 1Z M6.5 7.5a1 1 0 1 0 0-.01",
    newChat: "M4 4.5h12v8H9l-3.5 3v-3H4z M10 6.5v4 M8 8.5h4",
    upload: "M10 13.5v-9 M6 8l4-4 4 4 M4 16.5h12",
    checkSquare: "M4.5 3.5h11a1 1 0 0 1 1 1v11a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1v-11a1 1 0 0 1 1-1Z M6.5 10l2.5 2.5 4.5-5",
    square: "M4.5 3.5h11a1 1 0 0 1 1 1v11a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1v-11a1 1 0 0 1 1-1Z",
    fit: "M3.5 7.5v-4h4 M12.5 3.5h4v4 M16.5 12.5v4h-4 M7.5 16.5h-4v-4",
    language: "M3 5h8 M7 3.5V5 M9.5 5c-.7 3.5-3 6-6 7 M5 7.5c1 2 2.6 3.4 4.5 4 M10.5 17l3.2-8 3.3 8 M11.6 14.5h4.2",
    book: "M4 4.5A1.5 1.5 0 0 1 5.5 3H16v12H5.5A1.5 1.5 0 0 0 4 16.5v-12Z M4 16.5A1.5 1.5 0 0 0 5.5 18H16v-3 M7.5 6.5h5",
    rule: "M4 5h12 M4 10h8 M4 15h5 M14 13l2 2 3-3",
    selectAll: "M3.5 6.5v-2a1 1 0 0 1 1-1h2 M13.5 3.5h2a1 1 0 0 1 1 1v2 M16.5 13.5v2a1 1 0 0 1-1 1h-2 M6.5 16.5h-2a1 1 0 0 1-1-1v-2 M7 10l2 2 4-4",
    mail: "M3 5.5A1.5 1.5 0 0 1 4.5 4h11A1.5 1.5 0 0 1 17 5.5v9a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 3 14.5v-9Z M3.5 5.5 10 10.5l6.5-5",
    calendar: "M4.5 4h11A1.5 1.5 0 0 1 17 5.5v10a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 3 15.5v-10A1.5 1.5 0 0 1 4.5 4Z M3 8h14 M7 2.5V5 M13 2.5V5",
    settings: "M10 12.5a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5Z M8.4 2.8h3.2l.5 2.1 1.6.9 2-.7 1.6 2.8-1.6 1.4v1.8l1.6 1.4-1.6 2.8-2-.7-1.6.9-.5 2.1H8.4l-.5-2.1-1.6-.9-2 .7-1.6-2.8 1.6-1.4V9.3L2.7 7.9l1.6-2.8 2 .7 1.6-.9.5-2.1Z",
    compose: "M13.5 3.5l3 3L8 15l-3.5.5L5 12l8.5-8.5Z M11.5 5.5l3 3",
    inbox: "M3 11h4l1 2h4l1-2h4 M4.6 4h10.8L17 11v4a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1v-4l1.6-7Z",
    send: "M3 10 17 3l-4 14-3.5-5.5L3 10Z M9.5 11.5 17 3",
    draft: "M5 3h7l3 3v10a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1Z M12 3v3h3 M7 10h6 M7 13h4",
    trash: "M4 5.5h12 M8 5.5V4h4v1.5 M5.5 5.5l.7 10a1 1 0 0 0 1 1h5.6a1 1 0 0 0 1-1l.7-10 M8.5 8.5v5 M11.5 8.5v5",
    archive: "M3 4h14v3H3z M4 7v8a1 1 0 0 0 1 1h10a1 1 0 0 0 1-1V7 M8 10h4",
    junk: "M10 3a7 7 0 1 0 0 14 7 7 0 0 0 0-14Z M5 5l10 10",
    folder: "M3 6a1.5 1.5 0 0 1 1.5-1.5h3.5l1.5 1.5h6A1.5 1.5 0 0 1 17 7.5v7A1.5 1.5 0 0 1 15.5 16h-11A1.5 1.5 0 0 1 3 14.5V6Z",
    flag: "M5 17V3.5 M5 4h9l-2 3.5 2 3.5H5",
    flagFill: "M5 17V3.5 M5 4h9l-2 3.5 2 3.5H5Z",
    unread: "M3 6.5A1.5 1.5 0 0 1 4.5 5H12 M17 9v5.5a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 3 14.5v-8 M3.5 6.5 10 11l3-2 M16 7.5a2 2 0 1 0 0-4 2 2 0 0 0 0 4Z",
    read: "M3 8.5 10 4l7 4.5v6A1.5 1.5 0 0 1 15.5 16h-11A1.5 1.5 0 0 1 3 14.5v-6Z M3.5 8.5l6.5 4 6.5-4",
    reply: "M8 5 3.5 9.5 8 14 M4 9.5h7a5 5 0 0 1 5 5V16",
    replyAll: "M10 5 5.5 9.5 10 14 M6 5 1.5 9.5 6 14 M6 9.5h5.5a5 5 0 0 1 5 5V16",
    forward: "M12 5l4.5 4.5L12 14 M16 9.5H9a5 5 0 0 0-5 5V16",
    search: "M8.5 14a5.5 5.5 0 1 0 0-11 5.5 5.5 0 0 0 0 11Z M12.5 12.5 17 17",
    sync: "M15.5 7A6 6 0 0 0 4.6 7.5 M4.5 13a6 6 0 0 0 10.9-.5 M15.5 3.5V7H12 M4.5 16.5V13H8",
    close: "M5 5l10 10 M15 5 5 15",
    minimize: "M4 10h12",
    maximize: "M5 5h10v10H5z",
    restore: "M7 7h8v8H7z M5 13V5h8",
    chevronDown: "M5 7.5l5 5 5-5",
    chevronRight: "M7.5 5l5 5-5 5",
    chevronLeft: "M12.5 5l-5 5 5 5",
    chevronUp: "M5 12.5l5-5 5 5",
    attach: "M14.5 9.5 9.2 14.8a3.2 3.2 0 0 1-4.5-4.5l6-6a2.1 2.1 0 0 1 3 3l-6 6a1 1 0 0 1-1.5-1.5l5.3-5.3",
    more: "M5 10h.01 M10 10h.01 M15 10h.01",
    plus: "M10 4v12 M4 10h12",
    check: "M4 10.5l4 4 8-9",
    filter: "M3 5h14 M6 10h8 M8.5 15h3",
    move: "M3 6a1.5 1.5 0 0 1 1.5-1.5h3.5l1.5 1.5h6A1.5 1.5 0 0 1 17 7.5v7A1.5 1.5 0 0 1 15.5 16h-11A1.5 1.5 0 0 1 3 14.5V6Z M8 11h5 M11 9l2 2-2 2",
    person: "M10 10a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7Z M3.5 17a6.5 6.5 0 0 1 13 0",
    signature: "M3 15c2-1 3-6 5-6s-1 6 1 6 2-3 3-3 1 2 2 2 2-1 3-1 M3 17.5h14",
    palette: "M10 3a7 7 0 1 0 0 14c1 0 1.5-.7 1.5-1.5 0-1.2-1-1.5-1-2.5S11.2 11.5 12 11.5h2A3 3 0 0 0 17 8.5C17 5.5 13.9 3 10 3Z M6.5 9.5h.01 M8.5 6.5h.01 M12 6.5h.01",
    key: "M7 13a4 4 0 1 1 3.5-6L17 7v3h-2v2h-2.5l-1-1A4 4 0 0 1 7 13Z M6 9h.01",
    globe: "M10 17a7 7 0 1 0 0-14 7 7 0 0 0 0 14Z M3 10h14 M10 3c2 2 2.8 4.5 2.8 7S12 15 10 17c-2-2-2.8-4.5-2.8-7S8 5 10 3Z",
    bold: "M6 4h5a3 3 0 0 1 0 6H6z M6 10h6a3 3 0 0 1 0 6H6z",
    italic: "M9 4h6 M5 16h6 M12 4 8 16",
    underline: "M6 4v5a4 4 0 0 0 8 0V4 M5 17h10",
    strike: "M4 10h12 M13.5 6.5C13 5 11.7 4 10 4 8 4 6.5 5 6.5 6.6 6.5 8 7.5 8.8 10 9.5 M6.5 13.5C7 15 8.4 16 10.2 16c2 0 3.5-1.1 3.5-2.8 0-.8-.3-1.4-.9-1.9",
    listBullet: "M8 5h9 M8 10h9 M8 15h9 M4 5h.01 M4 10h.01 M4 15h.01",
    listNumber: "M8 5h9 M8 10h9 M8 15h9 M3.5 4l1-.5V7 M3.3 9.5c.3-.4.7-.5 1-.5.6 0 1 .4 1 .9 0 .8-2 1.6-2 2.1h2 M3.4 14.3h1.2c.5 0 .9.3.9.8s-.4.8-.9.8 M3.4 16.8h1.2c.5 0 .9-.3.9-.8",
    link: "M8.5 11.5a3 3 0 0 0 4.2 0l2.6-2.6a3 3 0 0 0-4.2-4.2l-1 1 M11.5 8.5a3 3 0 0 0-4.2 0l-2.6 2.6a3 3 0 0 0 4.2 4.2l1-1",
    image: "M4 4h12a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1Z M3 13l4-4 4 4 2-2 4 4 M13 7.5h.01",
    quote: "M5 8.5h3V12a3 3 0 0 1-3 3 M12 8.5h3V12a3 3 0 0 1-3 3 M5 8.5c0-2 1-3.5 3-4 M12 8.5c0-2 1-3.5 3-4",
    clear: "M5 4h10 M10 4l-3 12 M13 12l4 4 M17 12l-4 4",
    alignLeft: "M3 5h14 M3 9h9 M3 13h14 M3 17h9",
    alignCenter: "M3 5h14 M5.5 9h9 M3 13h14 M5.5 17h9",
    alignRight: "M3 5h14 M8 9h9 M3 13h14 M8 17h9",
    indent: "M8 5h9 M8 10h9 M8 15h9 M3 7.5 5.5 10 3 12.5",
    outdent: "M8 5h9 M8 10h9 M8 15h9 M5.5 7.5 3 10l2.5 2.5",
    textColor: "M6 14l4-10 4 10 M7.4 10.5h5.2 M4 17h12",
    highlight: "M12 3.5l4.5 4.5-7 7H5v-4.5l7-7Z M3 17.5h14",
    undo: "M7 5 3.5 8.5 7 12 M4 8.5h8a4.5 4.5 0 0 1 0 9H9",
    redo: "M13 5l3.5 3.5L13 12 M16 8.5H8a4.5 4.5 0 0 0 0 9h3",
    download: "M10 3.5v9 M6 9l4 4 4-4 M4 16.5h12",
    open: "M11 3.5h5.5V9 M16.5 3.5 9 11 M14 12v3.5a1 1 0 0 1-1 1H4.5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1H8",
    warning: "M10 3 17.5 16h-15L10 3Z M10 8v4 M10 14.3h.01",
    location: "M10 17s5-4.5 5-9a5 5 0 0 0-10 0c0 4.5 5 9 5 9Z M10 10a2 2 0 1 0 0-4 2 2 0 0 0 0 4Z",
    clock: "M10 17a7 7 0 1 0 0-14 7 7 0 0 0 0 14Z M10 6v4l2.5 2",
    video: "M3 6.5A1.5 1.5 0 0 1 4.5 5h7A1.5 1.5 0 0 1 13 6.5v7a1.5 1.5 0 0 1-1.5 1.5h-7A1.5 1.5 0 0 1 3 13.5v-7Z M13 9l4-2.5v7L13 11",
    star: "M10 3l2.1 4.4 4.9.6-3.6 3.3 1 4.8L10 13.7 5.6 16.1l1-4.8L3 8l4.9-.6L10 3Z",
    layout: "M3 4h14v12H3z M8 4v12",
    sun: "M10 13.5a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7Z M10 2v1.5 M10 16.5V18 M2 10h1.5 M16.5 10H18 M4.3 4.3l1.1 1.1 M14.6 14.6l1.1 1.1 M4.3 15.7l1.1-1.1 M14.6 5.4l1.1-1.1",
    moon: "M16 12.5A6.5 6.5 0 0 1 7.5 4 6.5 6.5 0 1 0 16 12.5Z",
    code: "M7 6 3 10l4 4 M13 6l4 4-4 4",
    print: "M6 7V3h8v4 M6 14H4a1 1 0 0 1-1-1V8a1 1 0 0 1 1-1h12a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1h-2 M6 11h8v6H6z",
    eye: "M2.5 10s2.8-5 7.5-5 7.5 5 7.5 5-2.8 5-7.5 5-7.5-5-7.5-5Z M10 12.3a2.3 2.3 0 1 0 0-4.6 2.3 2.3 0 0 0 0 4.6Z",
    eyeOff: "M3 3l14 14 M8.2 5.3A7.6 7.6 0 0 1 10 5c4.7 0 7.5 5 7.5 5a13 13 0 0 1-2.2 2.8 M12.1 14.6A7 7 0 0 1 10 15c-4.7 0-7.5-5-7.5-5a13 13 0 0 1 3-3.4",
    penCheck: "M3.5 13.5l.8-3L12 2.8l2.2 2.2-7.7 7.7-3 .8Z M10.6 4.2l2.2 2.2 M10.5 15.5l2 2 4.5-4.5",
    today: "M4.5 4h11A1.5 1.5 0 0 1 17 5.5v10a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 3 15.5v-10A1.5 1.5 0 0 1 4.5 4Z M3 8h14 M9 11h2v2H9z",
  };
  /** Optical centring: shifts (in grid units) for icons whose drawing isn't centred in the 20×20 box. */
  const OFFSET: Record<string, [number, number]> = {
    sparkle: [-1, -0.25],
    book: [0, -0.5],
    rule: [-1.5, 0],
    compose: [-0.5, 0.5],
    draft: [0.5, 0],
    flag: [0.5, -0.25],
    flagFill: [0.5, -0.25],
    unread: [-0.5, 0.25],
    reply: [0.25, -0.5],
    replyAll: [1, -0.5],
    forward: [-0.25, -0.5],
    attach: [0.75, 0.5],
    signature: [0, -3.25],
    key: [0, 1],
    bold: [-0.5, 0],
    underline: [0, -0.5],
    listBullet: [-0.5, 0],
    clear: [-1, 0],
    alignLeft: [0, -1],
    alignCenter: [0, -1],
    alignRight: [0, -1],
    textColor: [0, -0.5],
    highlight: [0, -0.5],
    undo: [0, -1.25],
    redo: [0, -1.25],
    warning: [0, 0.5],
    today: [0, -0.5],
  };
</script>

<script lang="ts">
  let { name, size = 18, fill = false, class: cls = "" }: { name: string; size?: number; fill?: boolean; class?: string } = $props();
</script>

<svg
  class="icon {cls}"
  width={size}
  height={size}
  viewBox="0 0 20 20"
  fill={fill ? "currentColor" : "none"}
  stroke="currentColor"
  stroke-width="1.45"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  <path d={ICONS[name] ?? ""} transform={OFFSET[name] ? `translate(${OFFSET[name][0]} ${OFFSET[name][1]})` : undefined} />
</svg>

<style>
  .icon {
    flex: none;
    /* Inline next to text it sits on the text's centre line; in flex rows it's a normal item. */
    display: inline-block;
    vertical-align: -0.2em;
    overflow: visible;
  }
</style>
