import { invoke } from '@tauri-apps/api/tauri';
import { listen } from '@tauri-apps/api/event';
import './style.css';

const app = document.querySelector('#app');

const state = {
  layout: null,
  draggingId: null,
  dragOffset: { x: 0, y: 0 },
  saveTimer: null,
  stealth: false,
  nodes: new Map(),
};

const defaultLayout = {
  nodes: [],
};

async function bootstrap() {
  app.innerHTML = renderShell();
  bindShellEvents();
  await listen('stealth-changed', ({ payload }) => {
    state.stealth = Boolean(payload);
    syncStealthUi();
  });
  await listen('layout-updated', () => {
    void loadLayout().then((next) => {
      state.layout = next ?? defaultLayout;
      renderLayout();
    });
  });
  const layout = await loadLayout();
  state.layout = layout ?? defaultLayout;
  renderLayout();
  syncStealthUi();
}

function renderShell() {
  return `
    <div class="hud-shell">
      <div class="ghost-fin" id="ghost-fin" title="Reveal FinNode"></div>
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
          <button class="chip" id="stealth-toggle" aria-pressed="false">Reveal HUD</button>
        </div>
        <div class="rail__section">
          <div class="section__title">Shortcuts</div>
          <div class="hint">Alt+S toggles stealth mode from Rust.</div>
          <div class="hint">Drag nodes to reposition. Links save automatically.</div>
        </div>
        <div class="rail__section meter">
          <div class="section__title">Resource sharks</div>
          <div class="sharks" id="sharks"></div>
        </div>
      </aside>
      <main class="stage">
        <svg class="links" id="links"></svg>
        <div class="node-layer" id="node-layer"></div>
        <div class="status-bar">
          <span id="status-text">Loading layout...</span>
          <span class="status-dot"></span>
        </div>
      </main>
    </div>
  `;
}

function bindShellEvents() {
  document.querySelector('#stealth-toggle').addEventListener('click', async () => {
    const nextStealth = !state.stealth;
    try {
      await invoke('set_stealth_mode', { enabled: nextStealth });
    } catch (error) {
      updateStatus(error instanceof Error ? error.message : String(error));
    }
  });

  document.querySelector('#ghost-fin').addEventListener('mouseenter', async () => {
    if (!state.stealth) {
      return;
    }

    try {
      await invoke('set_stealth_mode', { enabled: false });
    } catch (error) {
      updateStatus(error instanceof Error ? error.message : String(error));
    }
  });

  window.addEventListener('pointermove', onPointerMove);
  window.addEventListener('pointerup', onPointerUp);
  window.addEventListener('resize', renderConnections);
}

async function loadLayout() {
  const layout = await invoke('load_layout');
  updateStatus('Layout loaded from the app config directory');
  return layout;
}

function renderLayout() {
  const nodeLayer = document.querySelector('#node-layer');
  nodeLayer.innerHTML = '';
  state.nodes.clear();

  const nodes = state.layout?.nodes ?? [];
  nodes.forEach((node) => {
    const element = document.createElement('article');
    element.className = 'node';
    element.dataset.id = node.id;
    element.style.left = `${node.x}px`;
    element.style.top = `${node.y}px`;
    element.innerHTML = `
      <div class="node__surface">
        <header class="node__header">
          <div class="node__icon">${escapeHtml(node.icon ?? '◆')}</div>
          <div>
            <div class="node__name">${escapeHtml(node.name)}</div>
            <div class="node__meta">${escapeHtml(node.id.slice(0, 8))}</div>
          </div>
        </header>
        <p class="node__body">${escapeHtml(node.description ?? 'A linked context node')}</p>
        <div class="node__actions">
          <button data-action="open-path">Folder</button>
          <button data-action="open-editor">Editor</button>
          <button data-action="open-browser">Browser</button>
          <button data-action="run-script">Script</button>
        </div>
      </div>
    `;

    element.addEventListener('pointerdown', (event) => beginDrag(event, node.id));
    element.querySelectorAll('button').forEach((button) => {
      button.addEventListener('click', async (event) => {
        event.stopPropagation();
        await launchNode(node, button.dataset.action);
      });
    });

    nodeLayer.appendChild(element);
    state.nodes.set(node.id, element);
  });

  renderConnections();
  updateStatus(`Loaded ${nodes.length} node${nodes.length === 1 ? '' : 's'}`);
  renderSharks();
}

function renderConnections() {
  const svg = document.querySelector('#links');
  const layer = document.querySelector('#node-layer');
  const bounds = layer.getBoundingClientRect();
  if (!bounds.width || !bounds.height) {
    return;
  }

  svg.setAttribute('viewBox', `0 0 ${bounds.width} ${bounds.height}`);
  svg.innerHTML = '';

  const nodes = state.layout?.nodes ?? [];
  nodes.forEach((node) => {
    (node.links ?? []).forEach((targetId) => {
      const sourceEl = state.nodes.get(node.id);
      const targetEl = state.nodes.get(targetId);
      if (!sourceEl || !targetEl) {
        return;
      }

      const source = centerOf(sourceEl);
      const target = centerOf(targetEl);
      const offsetX = Math.max(100, Math.abs(target.x - source.x) * 0.35);
      const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
      path.setAttribute('d', `M ${source.x} ${source.y} C ${source.x + offsetX} ${source.y}, ${target.x - offsetX} ${target.y}, ${target.x} ${target.y}`);
      path.setAttribute('class', 'link');
      svg.appendChild(path);
    });
  });
}

function renderSharks() {
  const sharks = document.querySelector('#sharks');
  if (!sharks) {
    return;
  }

  sharks.innerHTML = '';
  const widths = [48, 74, 56, 92, 66];
  widths.forEach((width, index) => {
    const item = document.createElement('span');
    item.className = `shark shark--${index % 3}`;
    item.style.width = `${width}px`;
    item.style.animationDelay = `${index * 0.35}s`;
    sharks.appendChild(item);
  });
}

function centerOf(element) {
  const rect = element.getBoundingClientRect();
  const layerRect = document.querySelector('#node-layer').getBoundingClientRect();
  return {
    x: rect.left - layerRect.left + rect.width / 2,
    y: rect.top - layerRect.top + rect.height / 2,
  };
}

function beginDrag(event, id) {
  if (event.target instanceof Element && event.target.closest('button')) {
    return;
  }

  const node = state.layout.nodes.find((item) => item.id === id);
  if (!node) {
    return;
  }

  state.draggingId = id;
  const rect = event.currentTarget.getBoundingClientRect();
  state.dragOffset = {
    x: event.clientX - rect.left,
    y: event.clientY - rect.top,
  };
  event.currentTarget.classList.add('is-dragging');
}

function onPointerMove(event) {
  if (!state.draggingId) {
    return;
  }

  const node = state.layout.nodes.find((item) => item.id === state.draggingId);
  if (!node) {
    return;
  }

  const layerRect = document.querySelector('#node-layer').getBoundingClientRect();
  node.x = event.clientX - layerRect.left - state.dragOffset.x;
  node.y = event.clientY - layerRect.top - state.dragOffset.y;
  const element = state.nodes.get(node.id);
  if (element) {
    element.style.left = `${node.x}px`;
    element.style.top = `${node.y}px`;
  }
  renderConnections();
  scheduleSave();
}

function onPointerUp() {
  if (!state.draggingId) {
    return;
  }

  const element = state.nodes.get(state.draggingId);
  if (element) {
    element.classList.remove('is-dragging');
  }
  state.draggingId = null;
}

function scheduleSave() {
  window.clearTimeout(state.saveTimer);
  state.saveTimer = window.setTimeout(async () => {
    await invoke('save_layout', { layout: state.layout });
    updateStatus('Layout saved to disk');
  }, 250);
}

async function launchNode(node, action) {
  try {
    await invoke('launch_node', {
      node,
      action,
    });
    updateStatus(`Launched ${node.name} via ${action}`);
  } catch (error) {
    updateStatus(error instanceof Error ? error.message : String(error));
  }
}

function updateStatus(text) {
  const statusText = document.querySelector('#status-text');
  if (statusText) {
    statusText.textContent = text;
  }
}

function syncStealthUi() {
  document.body.classList.toggle('is-stealth', state.stealth);
  const button = document.querySelector('#stealth-toggle');
  if (button) {
    button.textContent = state.stealth ? 'Reveal HUD' : 'Hide HUD';
    button.setAttribute('aria-pressed', String(state.stealth));
  }
}

function escapeHtml(value) {
  return String(value ?? '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
}

bootstrap().catch((error) => {
  document.body.innerHTML = `<pre class="fatal">${escapeHtml(error.stack ?? error.message ?? String(error))}</pre>`;
});
