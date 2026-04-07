<script>
  import { onDestroy, onMount, tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/tauri';
  import { listen } from '@tauri-apps/api/event';

  let nodes = [];
  let stealth = false;
  let draggingId = null;
  let dragOffset = { x: 0, y: 0 };
  let saveTimer = null;
  let statusText = 'Loading layout...';
  let fatalError = '';

  let nodeLayer;
  let viewBox = '0 0 1 1';
  let links = [];

  const nodeElements = new Map();
  const sharkWidths = [48, 74, 56, 92, 66];

  function nodeRef(element, id) {
    nodeElements.set(id, element);
    queueRenderConnections();

    return {
      destroy() {
        nodeElements.delete(id);
      },
    };
  }

  function queueRenderConnections() {
    void tick().then(renderConnections);
  }

  async function bootstrap() {
    const unlistenStealth = await listen('stealth-changed', ({ payload }) => {
      stealth = Boolean(payload);
      syncStealthUi();
    });

    const unlistenLayout = await listen('layout-updated', async () => {
      const layout = await loadLayout();
      nodes = layout?.nodes ?? [];
      updateStatus(`Loaded ${nodes.length} node${nodes.length === 1 ? '' : 's'}`);
      queueRenderConnections();
    });

    const layout = await loadLayout();
    nodes = layout?.nodes ?? [];
    updateStatus(`Loaded ${nodes.length} node${nodes.length === 1 ? '' : 's'}`);
    syncStealthUi();
    queueRenderConnections();

    const onResize = () => renderConnections();
    const onMove = (event) => onPointerMove(event);
    const onUp = () => onPointerUp();

    window.addEventListener('resize', onResize);
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);

    onDestroy(() => {
      unlistenStealth();
      unlistenLayout();
      window.removeEventListener('resize', onResize);
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
      if (saveTimer) {
        window.clearTimeout(saveTimer);
      }
    });
  }

  async function loadLayout() {
    const layout = await invoke('load_layout');
    updateStatus('Layout loaded from the app config directory');
    return layout;
  }

  function centerOf(element) {
    const rect = element.getBoundingClientRect();
    const layerRect = nodeLayer.getBoundingClientRect();
    return {
      x: rect.left - layerRect.left + rect.width / 2,
      y: rect.top - layerRect.top + rect.height / 2,
    };
  }

  function renderConnections() {
    if (!nodeLayer) {
      return;
    }

    const bounds = nodeLayer.getBoundingClientRect();
    if (!bounds.width || !bounds.height) {
      return;
    }

    viewBox = `0 0 ${bounds.width} ${bounds.height}`;

    const next = [];
    for (const node of nodes) {
      for (const targetId of node.links ?? []) {
        const sourceEl = nodeElements.get(node.id);
        const targetEl = nodeElements.get(targetId);
        if (!sourceEl || !targetEl) {
          continue;
        }

        const source = centerOf(sourceEl);
        const target = centerOf(targetEl);
        const offsetX = Math.max(100, Math.abs(target.x - source.x) * 0.35);
        next.push(`M ${source.x} ${source.y} C ${source.x + offsetX} ${source.y}, ${target.x - offsetX} ${target.y}, ${target.x} ${target.y}`);
      }
    }

    links = next;
  }

  function beginDrag(event, id) {
    if (event.target instanceof Element && event.target.closest('button')) {
      return;
    }

    const node = nodes.find((item) => item.id === id);
    if (!node) {
      return;
    }

    draggingId = id;
    const rect = event.currentTarget.getBoundingClientRect();
    dragOffset = {
      x: event.clientX - rect.left,
      y: event.clientY - rect.top,
    };
    event.currentTarget.classList.add('is-dragging');
  }

  function onPointerMove(event) {
    if (!draggingId || !nodeLayer) {
      return;
    }

    const node = nodes.find((item) => item.id === draggingId);
    if (!node) {
      return;
    }

    const layerRect = nodeLayer.getBoundingClientRect();
    node.x = event.clientX - layerRect.left - dragOffset.x;
    node.y = event.clientY - layerRect.top - dragOffset.y;
    nodes = [...nodes];
    queueRenderConnections();
    scheduleSave();
  }

  function onPointerUp() {
    if (!draggingId) {
      return;
    }

    const element = nodeElements.get(draggingId);
    if (element) {
      element.classList.remove('is-dragging');
    }

    draggingId = null;
  }

  function scheduleSave() {
    if (saveTimer) {
      window.clearTimeout(saveTimer);
    }

    saveTimer = window.setTimeout(async () => {
      await invoke('save_layout', { layout: { nodes } });
      updateStatus('Layout saved to disk');
    }, 250);
  }

  async function launchNode(node, action) {
    try {
      await invoke('launch_node', { node, action });
      updateStatus(`Launched ${node.name} via ${action}`);
    } catch (error) {
      updateStatus(error instanceof Error ? error.message : String(error));
    }
  }

  function updateStatus(text) {
    statusText = text;
  }

  function syncStealthUi() {
    document.body.classList.toggle('is-stealth', stealth);
  }

  async function toggleStealth() {
    try {
      await invoke('set_stealth_mode', { enabled: !stealth });
    } catch (error) {
      updateStatus(error instanceof Error ? error.message : String(error));
    }
  }

  async function revealFromGhost() {
    if (!stealth) {
      return;
    }

    try {
      await invoke('set_stealth_mode', { enabled: false });
    } catch (error) {
      updateStatus(error instanceof Error ? error.message : String(error));
    }
  }

  onMount(() => {
    void bootstrap().catch((error) => {
      fatalError = error?.stack ?? error?.message ?? String(error);
    });
  });
</script>

{#if fatalError}
  <pre class="fatal">{fatalError}</pre>
{:else}
  <div class="hud-shell">
    <button
      class="ghost-fin"
      title="Reveal FinNode"
      aria-label="Reveal FinNode"
      on:mouseenter={revealFromGhost}
    ></button>

    <aside class="rail">
      <div class="brand">
        <div class="brand__mark">⟡</div>
        <div>
          <div class="brand__name">FinNode</div>
          <div class="brand__tag">desktop contexts, linked</div>
        </div>
      </div>

      <div class="rail__section">
        <div class="section__title">Stealth mode</div>
        <button class="chip" aria-pressed={stealth} on:click={toggleStealth}>
          {stealth ? 'Reveal HUD' : 'Hide HUD'}
        </button>
      </div>

      <div class="rail__section">
        <div class="section__title">Shortcuts</div>
        <div class="hint">Alt+S toggles stealth mode from Rust.</div>
        <div class="hint">Drag nodes to reposition. Links save automatically.</div>
      </div>

      <div class="rail__section meter">
        <div class="section__title">Resource sharks</div>
        <div class="sharks">
          {#each sharkWidths as width, index}
            <span class={`shark shark--${index % 3}`} style={`width:${width}px;animation-delay:${index * 0.35}s`}></span>
          {/each}
        </div>
      </div>
    </aside>

    <main class="stage">
      <svg class="links" {viewBox}>
        {#each links as d}
          <path class="link" {d}></path>
        {/each}
      </svg>

      <div class="node-layer" bind:this={nodeLayer}>
        {#each nodes as node (node.id)}
          <article
            class="node"
            use:nodeRef={node.id}
            style={`left:${node.x}px;top:${node.y}px;`}
            on:pointerdown={(event) => beginDrag(event, node.id)}
          >
            <div class="node__surface">
              <header class="node__header">
                <div class="node__icon">{node.icon ?? '◆'}</div>
                <div>
                  <div class="node__name">{node.name}</div>
                  <div class="node__meta">{node.id.slice(0, 8)}</div>
                </div>
              </header>
              <p class="node__body">{node.description ?? 'A linked context node'}</p>
              <div class="node__actions">
                <button on:click|stopPropagation={() => launchNode(node, 'open-path')}>Folder</button>
                <button on:click|stopPropagation={() => launchNode(node, 'open-editor')}>Editor</button>
                <button on:click|stopPropagation={() => launchNode(node, 'open-browser')}>Browser</button>
                <button on:click|stopPropagation={() => launchNode(node, 'run-script')}>Script</button>
              </div>
            </div>
          </article>
        {/each}
      </div>

      <div class="status-bar">
        <span>{statusText}</span>
        <span class="status-dot"></span>
      </div>
    </main>
  </div>
{/if}

<style>
  :global(:root) {
    color-scheme: dark;
    --bg: #04070d;
    --panel: rgba(12, 18, 29, 0.68);
    --panel-strong: rgba(16, 26, 41, 0.88);
    --line: rgba(120, 227, 255, 0.22);
    --text: #e8f7ff;
    --muted: rgba(200, 238, 255, 0.66);
    --glow: rgba(0, 255, 255, 0.45);
    --accent: #7cf4ff;
    --accent-2: #9dffb9;
    --danger: #ff8fa3;
    --shadow: 0 24px 80px rgba(0, 0, 0, 0.55);
    font-family: 'Space Grotesk', sans-serif;
  }

  * {
    box-sizing: border-box;
  }

  :global(html),
  :global(body) {
    width: 100%;
    height: 100%;
    margin: 0;
    overflow: hidden;
    background:
      radial-gradient(circle at 20% 20%, rgba(0, 255, 255, 0.18), transparent 28%),
      radial-gradient(circle at 80% 10%, rgba(89, 255, 197, 0.16), transparent 24%),
      linear-gradient(135deg, #02040a 0%, #07111d 48%, #02040a 100%);
    color: var(--text);
  }

  :global(body)::before {
    content: '';
    position: fixed;
    inset: 0;
    pointer-events: none;
    background-image: linear-gradient(rgba(255, 255, 255, 0.03) 1px, transparent 1px), linear-gradient(90deg, rgba(255, 255, 255, 0.03) 1px, transparent 1px);
    background-size: 42px 42px;
    mask-image: radial-gradient(circle at center, black 45%, transparent 82%);
    opacity: 0.4;
  }

  :global(body.is-stealth) .rail {
    transform: translateX(-18px);
    opacity: 0.15;
  }

  :global(body.is-stealth) .ghost-fin {
    opacity: 1;
  }

  :global(#app) {
    width: 100%;
    height: 100%;
  }

  .hud-shell {
    display: grid;
    grid-template-columns: 320px 1fr;
    width: 100%;
    height: 100%;
  }

  .ghost-fin {
    border: 0;
    padding: 0;
    position: fixed;
    top: 0;
    left: 0;
    width: 4px;
    height: 100vh;
    background: linear-gradient(180deg, transparent, rgba(124, 244, 255, 0.8), transparent);
    box-shadow: 0 0 18px rgba(124, 244, 255, 0.85);
    opacity: 0;
    transition: opacity 180ms ease;
    z-index: 30;
  }

  .rail {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 20px;
    padding: 24px;
    background: linear-gradient(180deg, rgba(12, 18, 29, 0.92), rgba(8, 12, 19, 0.48));
    border-right: 1px solid rgba(124, 244, 255, 0.16);
    backdrop-filter: blur(24px) saturate(160%);
    box-shadow: inset -1px 0 0 rgba(255, 255, 255, 0.04);
    transition: transform 260ms ease, opacity 260ms ease;
  }

  .brand {
    display: flex;
    gap: 14px;
    align-items: center;
  }

  .brand__mark {
    width: 48px;
    height: 48px;
    display: grid;
    place-items: center;
    border-radius: 16px;
    background: radial-gradient(circle at 30% 30%, rgba(124, 244, 255, 0.45), rgba(6, 15, 24, 0.95));
    box-shadow: 0 0 24px rgba(124, 244, 255, 0.25);
    color: var(--accent);
    font-size: 1.4rem;
  }

  .brand__name {
    font-size: 1.4rem;
    font-weight: 700;
    letter-spacing: 0.04em;
  }

  .brand__tag,
  .section__title,
  .hint,
  .status-bar {
    color: var(--muted);
  }

  .rail__section {
    padding: 18px;
    border: 1px solid rgba(124, 244, 255, 0.16);
    border-radius: 22px;
    background: rgba(7, 12, 20, 0.42);
  }

  .section__title {
    margin-bottom: 10px;
    text-transform: uppercase;
    letter-spacing: 0.16em;
    font-size: 0.72rem;
  }

  .chip,
  .node__actions button {
    border: 1px solid rgba(124, 244, 255, 0.22);
    background: rgba(8, 15, 26, 0.78);
    color: var(--text);
    border-radius: 999px;
    padding: 10px 14px;
    font: inherit;
    cursor: pointer;
    transition: transform 140ms ease, border-color 140ms ease, box-shadow 140ms ease;
  }

  .chip:hover,
  .node__actions button:hover {
    transform: translateY(-1px);
    border-color: rgba(124, 244, 255, 0.45);
    box-shadow: 0 0 18px rgba(124, 244, 255, 0.16);
  }

  .stage {
    position: relative;
    overflow: hidden;
  }

  .links,
  .node-layer {
    position: absolute;
    inset: 0;
  }

  .links {
    pointer-events: none;
  }

  .link {
    fill: none;
    stroke: var(--line);
    stroke-width: 2;
    stroke-linecap: round;
    filter: drop-shadow(0 0 8px rgba(124, 244, 255, 0.2));
  }

  .node-layer {
    padding: 32px;
  }

  .node {
    position: absolute;
    width: 240px;
    min-height: 170px;
    border-radius: 26px;
    cursor: grab;
    user-select: none;
    will-change: left, top;
  }

  .node__surface {
    position: relative;
    display: flex;
    flex-direction: column;
    min-height: 170px;
    padding: 16px;
    border-radius: 26px;
    background: linear-gradient(180deg, rgba(18, 27, 41, 0.94), rgba(10, 15, 24, 0.82));
    border: 1px solid rgba(124, 244, 255, 0.2);
    box-shadow: var(--shadow), 0 0 22px rgba(124, 244, 255, 0.08) inset;
    backdrop-filter: blur(20px) saturate(150%);
    animation: drift 18s ease-in-out infinite;
    transition: transform 140ms ease, box-shadow 140ms ease;
    will-change: transform;
  }

  .node__surface::after {
    content: '';
    position: absolute;
    inset: 0;
    border-radius: 26px;
    box-shadow: 0 0 24px rgba(124, 244, 255, 0.14);
    pointer-events: none;
  }

  .node.is-dragging {
    cursor: grabbing;
  }

  :global(.node.is-dragging) .node__surface {
    animation-play-state: paused;
    transform: scale(1.02);
  }

  .node__header {
    display: flex;
    gap: 12px;
    align-items: center;
  }

  .node__icon {
    width: 42px;
    height: 42px;
    display: grid;
    place-items: center;
    border-radius: 14px;
    background: rgba(124, 244, 255, 0.12);
    color: var(--accent);
    box-shadow: 0 0 15px rgba(124, 244, 255, 0.18);
  }

  .node__name {
    font-size: 1.05rem;
    font-weight: 700;
  }

  .node__meta {
    color: var(--muted);
    font-size: 0.82rem;
  }

  .node__body {
    margin: 14px 0;
    color: rgba(233, 248, 255, 0.82);
    line-height: 1.4;
  }

  .node__actions {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;
  }

  .status-bar {
    position: absolute;
    left: 24px;
    right: 24px;
    bottom: 24px;
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 14px 18px;
    border-radius: 18px;
    background: rgba(5, 9, 16, 0.58);
    border: 1px solid rgba(124, 244, 255, 0.16);
    backdrop-filter: blur(18px) saturate(160%);
  }

  .status-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--accent-2);
    box-shadow: 0 0 16px rgba(157, 255, 185, 0.65);
    animation: pulse 2.4s ease-in-out infinite;
  }

  .meter {
    margin-top: auto;
  }

  .sharks {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding-top: 8px;
  }

  .shark {
    height: 10px;
    border-radius: 999px;
    background: linear-gradient(90deg, rgba(124, 244, 255, 0.18), rgba(124, 244, 255, 0.92), rgba(157, 255, 185, 0.88));
    box-shadow: 0 0 18px rgba(124, 244, 255, 0.18);
    animation: swim 4.8s ease-in-out infinite;
  }

  .shark--1 {
    opacity: 0.72;
  }

  .shark--2 {
    opacity: 0.48;
  }

  .hint {
    line-height: 1.5;
    margin-top: 6px;
  }

  .fatal {
    white-space: pre-wrap;
    margin: 0;
    padding: 24px;
    color: #ffb4c2;
  }

  @keyframes drift {
    0%, 100% {
      transform: translateY(0) translateX(0);
    }
    50% {
      transform: translateY(-6px) translateX(3px);
    }
  }

  @keyframes pulse {
    0%, 100% {
      opacity: 0.45;
      transform: scale(0.82);
    }
    50% {
      opacity: 1;
      transform: scale(1.08);
    }
  }

  @keyframes swim {
    0%, 100% {
      transform: translateX(0);
    }
    50% {
      transform: translateX(18px);
    }
  }

  @media (max-width: 1000px) {
    .hud-shell {
      grid-template-columns: 1fr;
    }

    .rail {
      max-height: 280px;
      overflow: auto;
    }

    .node-layer {
      padding: 18px;
    }

    .status-bar {
      left: 14px;
      right: 14px;
      bottom: 14px;
    }
  }
</style>
