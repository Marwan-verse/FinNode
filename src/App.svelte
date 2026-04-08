<script>
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/tauri';
  import { listen } from '@tauri-apps/api/event';

  const STORAGE_KEY = 'finnode.overlay.nodes.v1';
  const NODE_WIDTH = 236;
  const NODE_HEIGHT = 144;

  const DEFAULT_NODES = [
    {
      id: 'command-hub',
      label: 'Command Hub',
      summary: 'Kick off local dev or build loops.',
      x: 420,
      y: 190,
      color: '#66f5ff',
      target: '.',
      command: 'npm run dev',
      shell: 'bash',
      links: ['docs-node', 'macro-node'],
    },
    {
      id: 'docs-node',
      label: 'Docs Node',
      summary: 'Open docs and references instantly.',
      x: 760,
      y: 120,
      color: '#9effb2',
      target: 'https://tauri.app',
      command: '',
      shell: 'bash',
      links: ['watch-node'],
    },
    {
      id: 'macro-node',
      label: 'Macro Node',
      summary: 'Launch command chains with one click.',
      x: 760,
      y: 360,
      color: '#ffaf63',
      target: 'https://svelte.dev',
      command: 'npm run build:web',
      shell: 'bash',
      links: ['watch-node'],
    },
    {
      id: 'watch-node',
      label: 'Watch Node',
      summary: 'Background status and quick checks.',
      x: 1120,
      y: 235,
      color: '#ff8db1',
      target: '.',
      command: 'npm run make',
      shell: 'bash',
      links: [],
    },
  ];

  const DEFAULT_MACRO = [
    { action: 'run_command', value: 'npm run dev', shell: 'bash' },
    { action: 'sleep', value: '1200' },
    { action: 'open_target', value: 'https://tauri.app' },
  ];

  let nodes = loadNodes();
  let selectedNodeId = nodes[0]?.id ?? null;
  let shell = detectDefaultShell();
  let workingDirectory = '.';
  let quickCommand = 'echo Overlay ready';
  let macroDraft = JSON.stringify(DEFAULT_MACRO, null, 2);
  let overlayClickThrough = false;
  let overlayVisible = true;
  let platform = 'unknown';
  let status = 'Overlay initialized';
  let viewport = { width: 1920, height: 1080 };
  let dragging = null;

  $: selectedNode = nodes.find((node) => node.id === selectedNodeId) ?? null;
  $: edges = buildEdges(nodes);

  function detectDefaultShell() {
    if (typeof navigator === 'undefined') return 'bash';
    return navigator.userAgent.toLowerCase().includes('windows') ? 'powershell' : 'bash';
  }

  function loadNodes() {
    if (typeof window === 'undefined') return [...DEFAULT_NODES];
    try {
      const raw = localStorage.getItem(STORAGE_KEY);
      if (!raw) return [...DEFAULT_NODES];
      const parsed = JSON.parse(raw);
      if (!Array.isArray(parsed) || parsed.length === 0) return [...DEFAULT_NODES];
      return parsed;
    } catch {
      return [...DEFAULT_NODES];
    }
  }

  function saveNodes() {
    if (typeof window === 'undefined') return;
    localStorage.setItem(STORAGE_KEY, JSON.stringify(nodes));
  }

  function setStatus(message) {
    status = message;
  }

  function updateNode(nodeId, patch) {
    nodes = nodes.map((node) => (node.id === nodeId ? { ...node, ...patch } : node));
    saveNodes();
  }

  function buildEdges(nodeList) {
    const nodeMap = new Map(nodeList.map((node) => [node.id, node]));
    const result = [];
    for (const source of nodeList) {
      for (const targetId of source.links ?? []) {
        const target = nodeMap.get(targetId);
        if (!target) continue;
        result.push({
          id: `${source.id}-${targetId}`,
          path: edgePath(source, target),
          color: source.color,
        });
      }
    }
    return result;
  }

  function edgePath(source, target) {
    const sx = source.x + NODE_WIDTH * 0.5;
    const sy = source.y + NODE_HEIGHT * 0.5;
    const tx = target.x + NODE_WIDTH * 0.5;
    const ty = target.y + NODE_HEIGHT * 0.5;
    const bend = Math.max(110, Math.abs(tx - sx) * 0.35);
    return `M ${sx} ${sy} C ${sx + bend} ${sy}, ${tx - bend} ${ty}, ${tx} ${ty}`;
  }

  function clampNodePosition(x, y) {
    const minX = 24;
    const minY = 24;
    const maxX = Math.max(minX, viewport.width - NODE_WIDTH - 24);
    const maxY = Math.max(minY, viewport.height - NODE_HEIGHT - 24);
    return {
      x: Math.min(Math.max(minX, x), maxX),
      y: Math.min(Math.max(minY, y), maxY),
    };
  }

  function startDrag(event, nodeId) {
    if (event.button !== 0) return;
    const node = nodes.find((entry) => entry.id === nodeId);
    if (!node) return;
    selectedNodeId = nodeId;
    dragging = {
      nodeId,
      offsetX: event.clientX - node.x,
      offsetY: event.clientY - node.y,
    };
    event.preventDefault();
  }

  function onPointerMove(event) {
    if (!dragging) return;
    const position = clampNodePosition(event.clientX - dragging.offsetX, event.clientY - dragging.offsetY);
    nodes = nodes.map((node) =>
      node.id === dragging.nodeId ? { ...node, x: position.x, y: position.y } : node
    );
  }

  function stopDrag() {
    if (!dragging) return;
    dragging = null;
    saveNodes();
  }

  async function openNodeTarget(node) {
    if (!node?.target?.trim()) {
      setStatus('Selected node has no target');
      return;
    }
    try {
      await invoke('open_target', { target: node.target.trim() });
      setStatus(`Opened target for ${node.label}`);
    } catch (error) {
      setStatus(String(error));
    }
  }

  async function runNodeCommand(node) {
    if (!node?.command?.trim()) {
      setStatus('Selected node has no command');
      return;
    }
    try {
      await invoke('run_command', {
        command: node.command.trim(),
        shell: node.shell || shell,
        cwd: workingDirectory.trim() || null,
      });
      setStatus(`Executed command for ${node.label}`);
    } catch (error) {
      setStatus(String(error));
    }
  }

  async function runQuickCommand() {
    if (!quickCommand.trim()) {
      setStatus('Quick command is empty');
      return;
    }
    try {
      await invoke('run_command', {
        command: quickCommand.trim(),
        shell,
        cwd: workingDirectory.trim() || null,
      });
      setStatus('Quick command launched');
    } catch (error) {
      setStatus(String(error));
    }
  }

  async function runMacroDraft() {
    let parsed = [];
    try {
      parsed = JSON.parse(macroDraft);
      if (!Array.isArray(parsed)) throw new Error('Macro must be a JSON array');
    } catch (error) {
      setStatus(`Macro parse error: ${error.message ?? String(error)}`);
      return;
    }

    try {
      await invoke('run_macro', { steps: parsed, cwd: workingDirectory.trim() || null });
      setStatus('Macro started');
    } catch (error) {
      setStatus(String(error));
    }
  }

  async function toggleClickThrough() {
    try {
      overlayClickThrough = await invoke('toggle_overlay_click_through');
      setStatus(
        overlayClickThrough
          ? 'Overlay click-through enabled (Alt+Shift+O toggles)'
          : 'Overlay click-through disabled'
      );
    } catch (error) {
      setStatus(String(error));
    }
  }

  async function toggleVisibility() {
    try {
      overlayVisible = await invoke('toggle_overlay_visibility');
      setStatus(
        overlayVisible
          ? 'Overlay visible'
          : 'Overlay hidden (Alt+Shift+H shows it again)'
      );
    } catch (error) {
      setStatus(String(error));
    }
  }

  function refreshViewport() {
    viewport = {
      width: window.innerWidth,
      height: window.innerHeight,
    };
  }

  onMount(() => {
    refreshViewport();

    const onResize = () => refreshViewport();
    const onMove = (event) => onPointerMove(event);
    const onUp = () => stopDrag();

    window.addEventListener('resize', onResize);
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
    window.addEventListener('pointercancel', onUp);

    const listeners = [];

    Promise.all([
      invoke('get_platform'),
      invoke('get_overlay_click_through'),
      invoke('get_overlay_visibility'),
      listen('overlay-click-through', (event) => {
        overlayClickThrough = Boolean(event.payload);
      }),
      listen('overlay-visibility', (event) => {
        overlayVisible = Boolean(event.payload);
      }),
    ])
      .then(([os, click, visible, unlistenClick, unlistenVisible]) => {
        platform = os;
        overlayClickThrough = Boolean(click);
        overlayVisible = Boolean(visible);
        listeners.push(unlistenClick, unlistenVisible);
        setStatus(`Overlay running on ${platform}`);
      })
      .catch((error) => {
        setStatus(String(error));
      });

    return () => {
      window.removeEventListener('resize', onResize);
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
      window.removeEventListener('pointercancel', onUp);
      listeners.forEach((dispose) => dispose());
    };
  });
</script>

<main class="overlay-shell">
  <div class="atmosphere atmosphere--a"></div>
  <div class="atmosphere atmosphere--b"></div>
  <div class="mesh"></div>

  <section class="control-dock">
    <div class="dock-head">
      <p class="eyebrow">Rust + Tauri Overlay Runtime</p>
      <h1>FinNode Core</h1>
      <p class="support-copy">
        Lightweight desktop nodes with native command execution and optional click-through stealth.
      </p>
    </div>

    <div class="dock-actions">
      <button class="action action--primary" on:click={toggleClickThrough}>
        {overlayClickThrough ? 'Disable Click-Through' : 'Enable Click-Through'}
      </button>
      <button class="action" on:click={toggleVisibility}>
        {overlayVisible ? 'Hide Overlay' : 'Show Overlay'}
      </button>
    </div>

    <div class="dock-grid">
      <label>
        Shell
        <select bind:value={shell}>
          <option value="bash">bash</option>
          <option value="powershell">powershell</option>
          <option value="cmd">cmd</option>
          <option value="zsh">zsh</option>
        </select>
      </label>
      <label>
        Working Directory
        <input bind:value={workingDirectory} placeholder="." />
      </label>
    </div>

    <label class="full-field">
      Quick Command
      <textarea bind:value={quickCommand} rows="2"></textarea>
    </label>
    <button class="action" on:click={runQuickCommand}>Run Quick Command</button>

    <label class="full-field">
      Macro JSON
      <textarea bind:value={macroDraft} rows="6"></textarea>
    </label>
    <button class="action" on:click={runMacroDraft}>Run Macro</button>

    {#if selectedNode}
      <div class="node-editor">
        <div class="node-editor__title">Selected Node: {selectedNode.label}</div>
        <label>
          Target
          <input
            value={selectedNode.target}
            on:input={(event) => updateNode(selectedNode.id, { target: event.currentTarget.value })}
          />
        </label>
        <label>
          Command
          <input
            value={selectedNode.command}
            on:input={(event) => updateNode(selectedNode.id, { command: event.currentTarget.value })}
          />
        </label>
        <label>
          Shell
          <select
            value={selectedNode.shell || shell}
            on:change={(event) => updateNode(selectedNode.id, { shell: event.currentTarget.value })}
          >
            <option value="bash">bash</option>
            <option value="powershell">powershell</option>
            <option value="cmd">cmd</option>
            <option value="zsh">zsh</option>
          </select>
        </label>
      </div>
    {/if}

    <div class="status-panel">
      <div class="status-text">{status}</div>
      <div class="status-meta">Hotkeys: Alt+Shift+O and Alt+Shift+H</div>
    </div>
  </section>

  <section class="graph-stage" aria-label="Node graph overlay">
    <svg class="link-layer" viewBox={`0 0 ${viewport.width} ${viewport.height}`} preserveAspectRatio="none">
      {#each edges as edge (edge.id)}
        <path d={edge.path} style={`--edge:${edge.color};`} />
      {/each}
    </svg>

    {#each nodes as node (node.id)}
      <div
        class="node-card"
        class:is-dragging={dragging?.nodeId === node.id}
        class:is-selected={selectedNodeId === node.id}
        style={`left:${node.x}px;top:${node.y}px;--node:${node.color};`}
        role="button"
        tabindex="0"
        on:keydown={(event) => {
          if (event.key === 'Enter' || event.key === ' ') {
            selectedNodeId = node.id;
          }
        }}
        on:pointerdown={(event) => startDrag(event, node.id)}
      >
        <h3>{node.label}</h3>
        <p>{node.summary}</p>
        <div class="node-row">
          <button on:click|stopPropagation={() => openNodeTarget(node)}>Open Target</button>
          <button on:click|stopPropagation={() => runNodeCommand(node)}>Run Command</button>
        </div>
      </div>
    {/each}
  </section>
</main>

<style>
  :global(:root) {
    color-scheme: dark;
    --ink: #e9fbff;
    --muted: rgba(220, 244, 255, 0.72);
    --panel: rgba(5, 14, 22, 0.78);
    --line: rgba(117, 222, 255, 0.26);
    --focus: #68f4ff;
    --focus-2: #b7ff9f;
    font-family: 'Sora', sans-serif;
  }

  :global(body) {
    background: transparent;
    color: var(--ink);
  }

  .overlay-shell {
    position: fixed;
    inset: 0;
    overflow: hidden;
    isolation: isolate;
  }

  .atmosphere {
    position: absolute;
    filter: blur(90px);
    opacity: 0.55;
    pointer-events: none;
  }

  .atmosphere--a {
    width: 52vw;
    height: 40vh;
    top: -10vh;
    left: 10vw;
    background: linear-gradient(110deg, rgba(56, 228, 255, 0.5), rgba(87, 255, 154, 0.36));
    animation: driftA 20s ease-in-out infinite;
  }

  .atmosphere--b {
    width: 48vw;
    height: 44vh;
    right: -10vw;
    bottom: -8vh;
    background: linear-gradient(130deg, rgba(255, 173, 99, 0.32), rgba(255, 120, 180, 0.3));
    animation: driftB 24s ease-in-out infinite;
  }

  .mesh {
    position: absolute;
    inset: 0;
    background-image:
      linear-gradient(to right, rgba(124, 218, 255, 0.06) 1px, transparent 1px),
      linear-gradient(to bottom, rgba(124, 218, 255, 0.06) 1px, transparent 1px);
    background-size: 56px 56px;
    mask-image: radial-gradient(circle at 45% 30%, black 35%, transparent 85%);
    pointer-events: none;
  }

  .control-dock {
    position: absolute;
    top: 24px;
    left: 24px;
    width: min(380px, calc(100vw - 48px));
    max-height: calc(100vh - 48px);
    overflow: auto;
    z-index: 12;
    padding: 18px;
    border-radius: 22px;
    border: 1px solid rgba(116, 242, 255, 0.28);
    background: linear-gradient(160deg, rgba(4, 13, 20, 0.9), rgba(8, 21, 34, 0.78));
    box-shadow: 0 22px 60px rgba(0, 0, 0, 0.45), inset 0 0 26px rgba(95, 228, 255, 0.08);
    backdrop-filter: blur(16px);
  }

  .dock-head h1 {
    margin: 4px 0;
    font-size: 1.46rem;
    letter-spacing: 0.02em;
  }

  .eyebrow {
    margin: 0;
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.18em;
    color: var(--focus-2);
  }

  .support-copy {
    margin: 0;
    line-height: 1.45;
    color: var(--muted);
    font-size: 0.84rem;
  }

  .dock-actions {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
    margin: 14px 0;
  }

  .action,
  .node-row button {
    border: 1px solid rgba(109, 236, 255, 0.28);
    background: rgba(8, 22, 35, 0.88);
    color: var(--ink);
    border-radius: 12px;
    padding: 9px 12px;
    cursor: pointer;
    transition: transform 130ms ease, border-color 130ms ease, box-shadow 130ms ease;
  }

  .action:hover,
  .node-row button:hover {
    transform: translateY(-1px);
    border-color: rgba(109, 236, 255, 0.58);
    box-shadow: 0 0 18px rgba(109, 236, 255, 0.2);
  }

  .action--primary {
    border-color: rgba(109, 236, 255, 0.62);
    background: linear-gradient(130deg, rgba(34, 145, 168, 0.58), rgba(38, 112, 182, 0.62));
  }

  .dock-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }

  label {
    display: grid;
    gap: 6px;
    margin: 10px 0 0;
    color: var(--muted);
    font-size: 0.76rem;
    letter-spacing: 0.03em;
  }

  .full-field {
    margin-top: 12px;
  }

  input,
  select,
  textarea {
    width: 100%;
    border-radius: 11px;
    border: 1px solid rgba(108, 220, 255, 0.26);
    background: rgba(2, 10, 18, 0.66);
    color: var(--ink);
    padding: 9px 10px;
    font-size: 0.82rem;
    font-family: 'JetBrains Mono', monospace;
  }

  textarea {
    resize: vertical;
    min-height: 70px;
  }

  .node-editor {
    margin-top: 12px;
    padding-top: 10px;
    border-top: 1px solid rgba(110, 225, 255, 0.2);
  }

  .node-editor__title {
    margin-bottom: 2px;
    color: var(--focus);
    font-size: 0.84rem;
  }

  .status-panel {
    margin-top: 12px;
    border-radius: 12px;
    border: 1px solid rgba(110, 225, 255, 0.2);
    background: rgba(5, 16, 26, 0.56);
    padding: 10px;
  }

  .status-text {
    font-size: 0.8rem;
    line-height: 1.45;
  }

  .status-meta {
    margin-top: 4px;
    font-size: 0.72rem;
    color: var(--muted);
  }

  .graph-stage {
    position: absolute;
    inset: 0;
  }

  .link-layer {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    pointer-events: none;
  }

  .link-layer path {
    fill: none;
    stroke: var(--line);
    stroke-width: 2.4;
    stroke-linecap: round;
    stroke: color-mix(in oklab, var(--edge) 62%, white 38%);
    filter: drop-shadow(0 0 9px color-mix(in oklab, var(--edge) 65%, white 35%));
    opacity: 0.82;
    animation: pulse 4s ease-in-out infinite;
  }

  .node-card {
    position: absolute;
    width: 236px;
    min-height: 144px;
    border-radius: 20px;
    padding: 14px;
    cursor: grab;
    user-select: none;
    border: 1px solid color-mix(in oklab, var(--node) 45%, white 20%);
    background: linear-gradient(
      168deg,
      color-mix(in oklab, var(--node) 22%, rgb(12, 23, 35) 78%),
      rgba(5, 14, 24, 0.82)
    );
    box-shadow:
      0 16px 40px rgba(0, 0, 0, 0.36),
      0 0 20px color-mix(in oklab, var(--node) 40%, transparent 60%);
    transition: transform 140ms ease, border-color 140ms ease;
  }

  .node-card h3 {
    margin: 0;
    font-size: 1rem;
    letter-spacing: 0.02em;
  }

  .node-card p {
    margin: 8px 0 12px;
    color: var(--muted);
    font-size: 0.8rem;
    line-height: 1.42;
  }

  .node-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }

  .node-card.is-selected {
    border-color: color-mix(in oklab, var(--node) 65%, white 35%);
    transform: translateY(-2px);
  }

  .node-card.is-dragging {
    cursor: grabbing;
    transform: scale(1.02);
  }

  @keyframes pulse {
    0%,
    100% {
      opacity: 0.72;
    }
    50% {
      opacity: 1;
    }
  }

  @keyframes driftA {
    0%,
    100% {
      transform: translate3d(0, 0, 0);
    }
    50% {
      transform: translate3d(-3vw, 2vh, 0);
    }
  }

  @keyframes driftB {
    0%,
    100% {
      transform: translate3d(0, 0, 0);
    }
    50% {
      transform: translate3d(4vw, -2vh, 0);
    }
  }

  @media (max-width: 980px) {
    .control-dock {
      left: 12px;
      right: 12px;
      top: 12px;
      width: auto;
      max-height: 56vh;
    }

    .dock-grid,
    .dock-actions,
    .node-row {
      grid-template-columns: 1fr;
    }

    .node-card {
      width: 208px;
      min-height: 132px;
    }
  }
</style>