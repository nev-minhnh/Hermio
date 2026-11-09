<script lang="ts">
  import { store } from "../lib/store.svelte";
  import FolderPane from "./FolderPane.svelte";
  import Toolbar from "./Toolbar.svelte";
  import MessageList from "./MessageList.svelte";
  import ReadingPane from "./ReadingPane.svelte";
  import Composer from "./Composer.svelte";
  import Splitter from "./Splitter.svelte";
  import AiPanel from "./AiPanel.svelte";
  import OutboxView from "./OutboxView.svelte";
  import { fly } from "svelte/transition";
  import { reduced } from "../lib/motion";

  const pane = $derived(store.settings.reading_pane);
  let listSize = $state(store.settings.list_width);
  let folderSize = $state(store.settings.folder_pane_width);
  let bottomSize = $state(320);
  // When the reading pane is off, opening a message replaces the list.
  let openFull = $state(false);

  $effect(() => {
    if (store.selected.length !== 1) openFull = false;
  });

  // Changing the reading-pane layout (right / bottom / off) eases into the new arrangement.
  let splitEl = $state<HTMLElement>();
  let lastPane = "";
  $effect(() => {
    const p = pane;
    if (lastPane && p !== lastPane && splitEl && !reduced()) {
      splitEl.animate([{ opacity: 0.4, scale: "0.99" }, { opacity: 1, scale: "1" }], { duration: 240, easing: "cubic-bezier(.2,.8,.2,1)" });
    }
    lastPane = p;
  });
</script>

<div class="mail">
  <div class="folders" style:width="{folderSize}px">
    <FolderPane />
  </div>
  <Splitter
    bind:size={folderSize}
    min={180}
    max={420}
    quiet
    onend={() => store.set("folder_pane_width", folderSize)}
  />
  <section class="work">
    <Toolbar />
    <div class="rowbody">
    <div class="split" class:bottom={pane === "bottom"} bind:this={splitEl}>
      {#if store.view === "outbox" && !store.compose}
        <OutboxView />
      {:else if pane === "off"}
        {#if store.compose}
          <div class="full" in:fly={{ y: 10, duration: 220 }}><Composer /></div>
        {:else if openFull && store.current}
          <div class="full" in:fly={{ x: 24, duration: 220 }}><ReadingPane onback={() => (openFull = false)} /></div>
        {:else}
          <div class="full list" in:fly={{ x: -16, duration: 220 }}><MessageList onopen={() => (openFull = true)} /></div>
        {/if}
      {:else}
        <div
          class="list"
          style:width={pane === "right" ? `${listSize}px` : null}
          style:height={pane === "bottom" ? `calc(100% - ${bottomSize}px)` : null}
        >
          <MessageList />
        </div>
        {#if pane === "right"}
          <Splitter bind:size={listSize} min={300} max={760} onend={() => store.set("list_width", listSize)} />
        {:else}
          <Splitter vertical invert bind:size={bottomSize} min={160} max={900} />
        {/if}
        <div class="reader">
          {#if store.compose}
            {#key store.compose.key}<Composer />{/key}
          {:else}
            <ReadingPane />
          {/if}
        </div>
      {/if}
    </div>
    {#if store.aiOpen}<AiPanel />{/if}
    <!-- The writing assistant's panel is placed here (same place as the AI assistant). -->
    <div id="wa-slot"></div>
    </div>
  </section>
</div>

<style>
  .mail {
    flex: 1;
    min-width: 0;
    display: flex;
  }
  .folders {
    flex: none;
    min-width: 0;
    display: flex;
  }
  .work {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border-radius: var(--radius-l);
    border: 1px solid var(--border);
    box-shadow: var(--shadow-s);
    overflow: hidden;
  }
  .rowbody {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .split {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
  }
  .split.bottom {
    flex-direction: column;
  }
  .list {
    flex: 0 1 auto;
    min-width: 0;
    min-height: 0;
    display: flex;
  }
  /* When space runs out (e.g. the assistant is open) the list gives way before the message. */
  .split:not(.bottom) .list {
    min-width: 260px;
  }
  .split:not(.bottom) .reader {
    min-width: 340px;
  }
  .reader {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    background: var(--surface-2);
  }
  .full {
    flex: 1;
    min-width: 0;
    display: flex;
  }
  #wa-slot {
    display: contents;
  }
</style>
