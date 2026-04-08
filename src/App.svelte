<script>
  import { onMount, tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/tauri';
  import { listen } from '@tauri-apps/api/event';
  import { appWindow } from '@tauri-apps/api/window';

  const SETTINGS_KEY = 'finnode.settings.v1';
  const settingsTabs = [
    { id: 'general', label: 'General' },
    { id: 'appearance', label: 'Appearance' },
    { id: 'nodes', label: 'Nodes' },
    { id: 'tray', label: 'Tray' },
    { id: 'shortcuts', label: 'Shortcuts' },
  ];

  const nodeTemplates = [
    {
      id: 'web-project',
      name: 'Web Project',
      icon: '◈',
      description: 'Frontend app + docs + browser',
      browser: 'https://vite.dev',
      script: 'npm run dev',
    },
    {
      id: 'rust-app',
      name: 'Rust App',
      icon: '⬡',
      description: 'Cargo workflow and crates links',
      browser: 'https://crates.io',
      script: 'cargo check',
    },
    {
      id: 'docs-hub',
      name: 'Documentation Hub',
      icon: '⟡',
      description: 'Notes, references, and quick links',
      browser: 'https://doc.rust-lang.org',
      script: 'npm run build:web',
    },
    {
      id: 'research-stack',
      name: 'Research Stack',
      icon: '⟁',
      description: 'Context, ideas, and experiments',
      browser: 'https://github.com/trending',
      script: 'npm run build:web',
    },
  ];

  let nodes = [];
  let renderNodes = [];
  let smoothNodes = [];
  let stealth = false;
  let showDesktop = false;
  let activeTab = 'general';
  let draggingId = null;
  let dragOffset = { x: 0, y: 0 };
  let pendingPointer = null;
  let dragFrame = null;
  let nodeSpringFrame = null;
  let saveTimer = null;
  let statusText = 'Loading layout...';
  let fatalError = '';
  let selectedTemplate = nodeTemplates[0].id;
  let activityLog = [];
  let currentWindowLabel = 'main';
  let isDesktopWindow = false;
  let expandedNodeId = null;
  let dragMoved = false;
  let dragStart = { x: 0, y: 0 };
  let editPopup = {
    open: false,
    x: 0,
    y: 0,
    nodeId: null,
  };
  let editDraft = {
    name: '',
    description: '',
    path: '',
    browser: '',
    script: '',
  };
  let editSelectedLinks = [];
  let editNode = null;
  let contextNode = null;
  let contextMenu = {
    open: false,
    x: 0,
    y: 0,
    nodeId: null,
  };

  let nodeLayer;
  let viewBox = '0 0 1 1';
  let links = [];

  const nodeElements = new Map();
  let settings = loadSettings();

  function createDefaultSettings() {
    return {
      general: {
        openOnLogin: false,
        startMinimizedToTray: false,
        restoreLastMode: true,
        lastMode: 'settings',
      },
      appearance: {
        motionScale: 1,
        nodeGlow: 0.45,
        showGrid: true,
      },
      nodes: {
        showDesktop: false,
        smoothness: 0.2,
      },
      tray: {
        leftClickAction: 'open-settings',
      },
      shortcuts: {
        toggleStealth: 'Alt+S',
      },
    };
  }

  function cloneObject(value) {
    return JSON.parse(JSON.stringify(value));
  }

  function mergeSettings(base, incoming) {
    const next = cloneObject(base);
    if (!incoming || typeof incoming !== 'object') {
      return next;
    }

    for (const [key, value] of Object.entries(incoming)) {
      if (value && typeof value === 'object' && !Array.isArray(value) && next[key] && typeof next[key] === 'object' && !Array.isArray(next[key])) {
        next[key] = mergeSettings(next[key], value);
      } else if (value !== undefined) {
        next[key] = value;
      }
    }

    return next;
  }

  function loadSettings() {
    const defaults = createDefaultSettings();
    if (typeof window === 'undefined') {
      return defaults;
    }

    try {
      const raw = window.localStorage.getItem(SETTINGS_KEY);
      if (!raw) {
        return defaults;
      }
      const parsed = JSON.parse(raw);
      return mergeSettings(defaults, parsed);
    } catch {
      return defaults;
    }
  }

  function persistSettings() {
    if (typeof window === 'undefined') {
      return;
    }
    window.localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
  }

  function updateSettings(mutator) {
    const next = cloneObject(settings);
    mutator(next);
    settings = next;
    persistSettings();
  }

  function recordActivity(message) {
    const timestamp = new Date().toLocaleTimeString();
    activityLog = [{ id: `${Date.now()}-${Math.random()}`, text: `${timestamp} - ${message}` }, ...activityLog].slice(0, 24);
  }

  $: {
    const smoothMap = new Map(smoothNodes.map((item) => [item.id, item]));
    renderNodes = nodes.map((node) => {
      const smooth = smoothMap.get(node.id);
      return {
        ...node,
        renderX: smooth ? smooth.x : node.x,
        renderY: smooth ? smooth.y : node.y,
      };
    });
  }

  $: {
    isDesktopWindow = currentWindowLabel === 'desktop';
  }

  $: {
    if (typeof document !== 'undefined') {
      document.body.classList.toggle('is-stealth', stealth);
      document.body.classList.toggle('desktop-mode', isDesktopWindow);
      document.body.classList.toggle('desktop-overlay-window', isDesktopWindow);
      document.body.classList.toggle('settings-app-window', !isDesktopWindow);
      document.documentElement.style.setProperty('--motion-scale', String(Math.max(0.4, settings.appearance.motionScale)));
      document.documentElement.style.setProperty('--node-glow', String(Math.max(0.15, settings.appearance.nodeGlow)));
      document.documentElement.style.setProperty('--grid-opacity', settings.appearance.showGrid ? '0.4' : '0');
    }
  }

  $: contextNode = contextMenu.nodeId ? nodes.find((node) => node.id === contextMenu.nodeId) : null;
  $: editNode = editPopup.nodeId ? nodes.find((node) => node.id === editPopup.nodeId) : null;

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

  function startNodeSpring() {
    if (nodeSpringFrame !== null) {
      return;
    }
    nodeSpringFrame = window.requestAnimationFrame(stepNodeSpring);
  }

  function syncSmoothNodes(immediate = false) {
    const current = new Map(smoothNodes.map((item) => [item.id, item]));

    smoothNodes = nodes.map((node) => {
      const existing = current.get(node.id);
      if (!existing || immediate) {
        return { id: node.id, x: node.x, y: node.y, vx: 0, vy: 0 };
      }
      return existing;
    });

    if (immediate) {
      smoothNodes = smoothNodes.map((item) => {
        const target = nodes.find((node) => node.id === item.id);
        if (!target) {
          return item;
        }
        return { ...item, x: target.x, y: target.y, vx: 0, vy: 0 };
      });
    }

    startNodeSpring();
  }

  function stepNodeSpring() {
    nodeSpringFrame = null;
    if (!smoothNodes.length) {
      return;
    }

    const stiffness = Math.max(0.08, Math.min(0.45, settings.nodes.smoothness));
    const damping = 0.82;
    const targets = new Map(nodes.map((node) => [node.id, node]));
    let active = false;

    smoothNodes = smoothNodes.map((item) => {
      const target = targets.get(item.id);
      if (!target) {
        return item;
      }

      if (draggingId === item.id) {
        return { ...item, x: target.x, y: target.y, vx: 0, vy: 0 };
      }

      const dx = target.x - item.x;
      const dy = target.y - item.y;
      const vx = (item.vx + dx * stiffness) * damping;
      const vy = (item.vy + dy * stiffness) * damping;
      const x = item.x + vx;
      const y = item.y + vy;

      if (Math.abs(dx) > 0.14 || Math.abs(dy) > 0.14 || Math.abs(vx) > 0.14 || Math.abs(vy) > 0.14) {
        active = true;
      }

      return { ...item, x, y, vx, vy };
    });

    queueRenderConnections();
    if (active) {
      nodeSpringFrame = window.requestAnimationFrame(stepNodeSpring);
    }
  }

  async function syncDesktopVisibilityWithBackend(visible) {
    try {
      await invoke('set_desktop_visibility', { visible });
    } catch (error) {
      updateStatus(error instanceof Error ? error.message : String(error));
    }
  }

  function applyDesktopVisibility(visible, syncBackend = true) {
    const nextVisible = Boolean(visible);
    showDesktop = nextVisible;
    closeContextMenu();
    if (!nextVisible) {
      closeNodeEditor();
      expandedNodeId = null;
    }

    if (!isDesktopWindow) {
      updateSettings((draft) => {
        draft.nodes.showDesktop = nextVisible;
        if (draft.general.restoreLastMode) {
          draft.general.lastMode = nextVisible ? 'desktop' : 'settings';
        }
      });
    }

    if (syncBackend) {
      void syncDesktopVisibilityWithBackend(nextVisible);
    }

    queueRenderConnections();
  }

  function closeContextMenu() {
    if (!contextMenu.open) {
      return;
    }

    contextMenu = {
      open: false,
      x: 0,
      y: 0,
      nodeId: null,
    };
  }

  function closeNodeEditor() {
    if (!editPopup.open) {
      return;
    }

    editPopup = {
      open: false,
      x: 0,
      y: 0,
      nodeId: null,
    };
  }

  function openNodeEditor(nodeId) {
    const node = nodes.find((item) => item.id === nodeId);
    const element = nodeElements.get(nodeId);
    if (!node || !element) {
      return;
    }

    const rect = element.getBoundingClientRect();
    const popupWidth = 320;
    const popupHeight = 420;
    let x = rect.right + 14;
    if (x + popupWidth > window.innerWidth - 12) {
      x = rect.left - popupWidth - 14;
    }

    x = Math.max(12, Math.min(x, window.innerWidth - popupWidth - 12));
    const y = Math.max(12, Math.min(rect.top, window.innerHeight - popupHeight - 12));

    editDraft = {
      name: node.name ?? '',
      description: node.description ?? '',
      path: node.targets?.path ?? '',
      browser: node.targets?.browser ?? '',
      script: node.targets?.script ?? '',
    };
    editSelectedLinks = [...(node.links ?? [])];

    editPopup = {
      open: true,
      x,
      y,
      nodeId,
    };
  }

  function toggleEditLink(targetId, enabled) {
    if (enabled) {
      editSelectedLinks = Array.from(new Set([...editSelectedLinks, targetId]));
      return;
    }

    editSelectedLinks = editSelectedLinks.filter((item) => item !== targetId);
  }

  function saveNodeEditor() {
    if (!editPopup.nodeId) {
      return;
    }

    const nodeId = editPopup.nodeId;
    nodes = nodes.map((node) => {
      if (node.id !== nodeId) {
        return node;
      }

      const nextName = editDraft.name.trim() || node.name;
      return {
        ...node,
        name: nextName,
        description: editDraft.description.trim(),
        links: Array.from(new Set(editSelectedLinks.filter((linkId) => linkId !== nodeId))),
        targets: {
          ...(node.targets ?? {}),
          path: editDraft.path.trim() || null,
          browser: editDraft.browser.trim() || null,
          script: editDraft.script.trim() || null,
        },
      };
    });

    scheduleSave();
    queueRenderConnections();
    updateStatus('Node details updated');
    closeNodeEditor();
  }

  function toggleNodeExpanded(nodeId) {
    expandedNodeId = expandedNodeId === nodeId ? null : nodeId;
    if (expandedNodeId !== nodeId) {
      closeNodeEditor();
    }
  }

  function openNodeContextMenu(event, nodeId) {
    event.preventDefault();
    event.stopPropagation();
    expandedNodeId = nodeId;
    closeNodeEditor();

    const maxX = window.innerWidth - 240;
    const maxY = window.innerHeight - 250;

    contextMenu = {
      open: true,
      x: Math.max(10, Math.min(event.clientX, maxX)),
      y: Math.max(10, Math.min(event.clientY, maxY)),
      nodeId,
    };
  }

  function addConnectedNodeFromMenu() {
    const source = contextNode;
    if (!source) {
      closeContextMenu();
      return;
    }

    const template = nodeTemplates.find((item) => item.id === selectedTemplate) ?? nodeTemplates[0];
    const node = {
      id: uniqueNodeId(template.id),
      name: `${template.name} Node`,
      icon: template.icon,
      description: template.description,
      x: source.x + 260,
      y: source.y + 30,
      links: [],
      targets: {
        path: '.',
        editor: '.',
        browser: template.browser,
        script: template.script,
      },
    };

    nodes = [
      ...nodes.map((item) => {
        if (item.id !== source.id) {
          return item;
        }

        return {
          ...item,
          links: Array.from(new Set([...(item.links ?? []), node.id])),
        };
      }),
      node,
    ];

    syncSmoothNodes(true);
    scheduleSave();
    updateStatus(`Added connected node from ${source.name}`);
    closeContextMenu();
  }

  function connectNearestNodeFromMenu() {
    const source = contextNode;
    if (!source) {
      closeContextMenu();
      return;
    }

    const candidates = nodes.filter((node) => node.id !== source.id);
    if (!candidates.length) {
      updateStatus('No other nodes available to connect');
      closeContextMenu();
      return;
    }

    const nearest = candidates.reduce((best, current) => {
      const bestDist = (best.x - source.x) ** 2 + (best.y - source.y) ** 2;
      const currentDist = (current.x - source.x) ** 2 + (current.y - source.y) ** 2;
      return currentDist < bestDist ? current : best;
    });

    nodes = nodes.map((node) => {
      if (node.id !== source.id) {
        return node;
      }

      return {
        ...node,
        links: Array.from(new Set([...(node.links ?? []), nearest.id])),
      };
    });

    scheduleSave();
    queueRenderConnections();
    updateStatus(`Connected ${source.name} to ${nearest.name}`);
    closeContextMenu();
  }

  function clearNodeLinksFromMenu() {
    const source = contextNode;
    if (!source) {
      closeContextMenu();
      return;
    }

    nodes = nodes.map((node) => (node.id === source.id ? { ...node, links: [] } : node));
    scheduleSave();
    queueRenderConnections();
    updateStatus(`Cleared outgoing links from ${source.name}`);
    closeContextMenu();
  }

  function cloneNodeFromMenu() {
    if (contextNode) {
      cloneNode(contextNode.id);
    }
    closeContextMenu();
  }

  function deleteNodeFromMenu() {
    if (contextNode) {
      deleteNode(contextNode.id);
    }
    closeContextMenu();
  }

  async function bootstrap() {
    const unlistenStealth = await listen('stealth-changed', ({ payload }) => {
      stealth = Boolean(payload);
      updateStatus(`Stealth mode ${stealth ? 'enabled' : 'disabled'}`);
    });

    const unlistenLayout = await listen('layout-updated', async () => {
      const layout = await loadLayout();
      nodes = layout?.nodes ?? [];
      syncSmoothNodes(true);
      updateStatus(`Loaded ${nodes.length} node${nodes.length === 1 ? '' : 's'}`);
    });

    const unlistenDesktop = await listen('desktop-visibility-changed', ({ payload }) => {
      const visible = Boolean(payload);
      if (isDesktopWindow) {
        showDesktop = visible;
        if (!visible) {
          expandedNodeId = null;
          closeContextMenu();
          closeNodeEditor();
        }
      } else {
        applyDesktopVisibility(visible, false);
      }
      updateStatus(`Desktop nodes ${Boolean(payload) ? 'shown' : 'hidden'} from tray`);
    });

    const unlistenOpenSettings = await listen('open-settings-tab', ({ payload }) => {
      if (isDesktopWindow) {
        return;
      }
      activeTab = typeof payload === 'string' ? payload : 'general';
      updateStatus('Opened settings from tray');
    });

    const layout = await loadLayout();
    nodes = layout?.nodes ?? [];
    syncSmoothNodes(true);

    if (isDesktopWindow) {
      showDesktop = true;
    } else {
      if (settings.general.restoreLastMode) {
        showDesktop = settings.general.lastMode === 'desktop';
      } else {
        showDesktop = settings.nodes.showDesktop;
      }

      await syncDesktopVisibilityWithBackend(showDesktop);
    }

    updateStatus(`Loaded ${nodes.length} node${nodes.length === 1 ? '' : 's'}`);
    queueRenderConnections();

    if (!isDesktopWindow && settings.general.startMinimizedToTray) {
      void hideToTray();
    }

    const onResize = () => queueRenderConnections();
    const onMove = (event) => onPointerMove(event);
    const onUp = () => onPointerUp();
    const onPointerDown = (event) => {
      if (!(event.target instanceof Element)) {
        closeContextMenu();
        closeNodeEditor();
        return;
      }

      if (!event.target.closest('.context-menu')) {
        closeContextMenu();
      }

      if (!event.target.closest('.node-editor-popup')) {
        closeNodeEditor();
      }
    };
    const onKeyDown = (event) => {
      if (event.key === 'Escape') {
        closeContextMenu();
        closeNodeEditor();
      }
    };

    window.addEventListener('resize', onResize);
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
    window.addEventListener('pointerdown', onPointerDown);
    window.addEventListener('keydown', onKeyDown);

    return () => {
      unlistenStealth();
      unlistenLayout();
      unlistenDesktop();
      unlistenOpenSettings();
      window.removeEventListener('resize', onResize);
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
      window.removeEventListener('pointerdown', onPointerDown);
      window.removeEventListener('keydown', onKeyDown);
      if (saveTimer) {
        window.clearTimeout(saveTimer);
      }
      if (dragFrame !== null) {
        window.cancelAnimationFrame(dragFrame);
      }
      if (nodeSpringFrame !== null) {
        window.cancelAnimationFrame(nodeSpringFrame);
      }
    };
  }

  async function loadLayout() {
    const layout = await invoke('load_layout');
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
    for (const node of renderNodes) {
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
    if (event.button !== 0) {
      return;
    }

    if (event.target instanceof Element && event.target.closest('button')) {
      return;
    }

    const node = nodes.find((item) => item.id === id);
    if (!node) {
      return;
    }

    draggingId = id;
    dragMoved = false;
    dragStart = {
      x: event.clientX,
      y: event.clientY,
    };
    closeContextMenu();
    closeNodeEditor();
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

    if (!dragMoved) {
      const movedX = Math.abs(event.clientX - dragStart.x);
      const movedY = Math.abs(event.clientY - dragStart.y);
      dragMoved = movedX > 4 || movedY > 4;
    }

    pendingPointer = {
      x: event.clientX,
      y: event.clientY,
    };

    if (dragFrame !== null) {
      return;
    }

    dragFrame = window.requestAnimationFrame(() => {
      dragFrame = null;

      if (!pendingPointer || !draggingId || !nodeLayer) {
        return;
      }

      const pointer = pendingPointer;
      pendingPointer = null;

      const node = nodes.find((item) => item.id === draggingId);
      if (!node) {
        return;
      }

      const layerRect = nodeLayer.getBoundingClientRect();
      node.x = pointer.x - layerRect.left - dragOffset.x;
      node.y = pointer.y - layerRect.top - dragOffset.y;
      nodes = [...nodes];
      startNodeSpring();
      scheduleSave();
    });
  }

  function onPointerUp() {
    if (!draggingId) {
      return;
    }

    const releasedNodeId = draggingId;

    if (dragFrame !== null) {
      window.cancelAnimationFrame(dragFrame);
      dragFrame = null;
    }
    pendingPointer = null;

    const element = nodeElements.get(draggingId);
    if (element) {
      element.classList.remove('is-dragging');
    }

    draggingId = null;

    if (!dragMoved) {
      toggleNodeExpanded(releasedNodeId);
    }

    dragMoved = false;
  }

  function scheduleSave() {
    if (saveTimer) {
      window.clearTimeout(saveTimer);
    }

    saveTimer = window.setTimeout(async () => {
      await invoke('save_layout', { layout: { nodes } });
      updateStatus('Layout saved to disk');
    }, 220);
  }

  function updateStatus(text) {
    statusText = text;
    recordActivity(text);
  }

  async function launchNode(node, action) {
    try {
      await invoke('launch_node', { node, action });
      updateStatus(`Launched ${node.name} via ${action}`);
    } catch (error) {
      updateStatus(error instanceof Error ? error.message : String(error));
    }
  }

  async function openLinkedFolder(node) {
    if (!node?.targets?.path) {
      updateStatus(`No linked folder configured for ${node?.name ?? 'node'}`);
      return;
    }

    await launchNode(node, 'open-path');
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

  async function hideToTray() {
    try {
      await invoke('hide_main_window');
    } catch (error) {
      updateStatus(error instanceof Error ? error.message : String(error));
    }
  }

  async function openSettingsFromTray() {
    try {
      await invoke('show_settings_view');
      activeTab = 'general';
    } catch (error) {
      updateStatus(error instanceof Error ? error.message : String(error));
    }
  }

  async function exitApp() {
    try {
      await invoke('exit_app');
    } catch (error) {
      updateStatus(error instanceof Error ? error.message : String(error));
    }
  }

  function toggleDesktop() {
    applyDesktopVisibility(!showDesktop, true);
  }

  function updateSmoothness(value) {
    const parsed = Number(value);
    updateSettings((draft) => {
      draft.nodes.smoothness = Number.isFinite(parsed) ? parsed : draft.nodes.smoothness;
    });
    startNodeSpring();
  }

  function updateMotionScale(value) {
    const parsed = Number(value);
    updateSettings((draft) => {
      draft.appearance.motionScale = Number.isFinite(parsed) ? parsed : draft.appearance.motionScale;
    });
  }

  function updateNodeGlow(value) {
    const parsed = Number(value);
    updateSettings((draft) => {
      draft.appearance.nodeGlow = Number.isFinite(parsed) ? parsed : draft.appearance.nodeGlow;
    });
  }

  function toggleGrid(enabled) {
    updateSettings((draft) => {
      draft.appearance.showGrid = enabled;
    });
  }

  function uniqueNodeId(base) {
    return `${base}-${Math.random().toString(36).slice(2, 8)}`;
  }

  function addNodeFromTemplate() {
    const template = nodeTemplates.find((item) => item.id === selectedTemplate) ?? nodeTemplates[0];
    const offset = nodes.length * 18;
    const node = {
      id: uniqueNodeId(template.id),
      name: template.name,
      icon: template.icon,
      description: template.description,
      x: 90 + offset,
      y: 110 + offset,
      links: [],
      targets: {
        path: '.',
        editor: '.',
        browser: template.browser,
        script: template.script,
      },
    };

    nodes = [...nodes, node];
    syncSmoothNodes(true);
    scheduleSave();
    updateStatus(`Added node ${node.name}`);
  }

  function renameNode(id, value) {
    nodes = nodes.map((node) => (node.id === id ? { ...node, name: value } : node));
    scheduleSave();
  }

  function setNodeLinkedFolder(id, value) {
    const trimmed = value.trim();
    nodes = nodes.map((node) => {
      if (node.id !== id) {
        return node;
      }

      return {
        ...node,
        targets: {
          ...(node.targets ?? {}),
          path: trimmed || null,
        },
      };
    });
    scheduleSave();
    updateStatus('Updated linked folder path');
  }

  function moveNode(id, direction) {
    const index = nodes.findIndex((node) => node.id === id);
    if (index < 0) {
      return;
    }

    const targetIndex = index + direction;
    if (targetIndex < 0 || targetIndex >= nodes.length) {
      return;
    }

    const next = [...nodes];
    const [moved] = next.splice(index, 1);
    next.splice(targetIndex, 0, moved);
    nodes = next;
    scheduleSave();
    updateStatus(`Reordered node ${moved.name}`);
  }

  function cloneNode(id) {
    const node = nodes.find((item) => item.id === id);
    if (!node) {
      return;
    }

    const clone = {
      ...node,
      id: uniqueNodeId(node.id),
      name: `${node.name} Copy`,
      x: node.x + 26,
      y: node.y + 26,
      links: [...(node.links ?? [])],
      targets: { ...(node.targets ?? {}) },
    };

    nodes = [...nodes, clone];
    expandedNodeId = clone.id;
    syncSmoothNodes(true);
    scheduleSave();
    updateStatus(`Cloned node ${node.name}`);
  }

  function deleteNode(id) {
    const removed = nodes.find((node) => node.id === id);
    if (!removed) {
      return;
    }

    if (contextMenu.nodeId === id) {
      closeContextMenu();
    }

    if (editPopup.nodeId === id) {
      closeNodeEditor();
    }

    if (expandedNodeId === id) {
      expandedNodeId = null;
    }

    nodes = nodes
      .filter((node) => node.id !== id)
      .map((node) => ({
        ...node,
        links: (node.links ?? []).filter((linkId) => linkId !== id),
      }));

    syncSmoothNodes(true);
    scheduleSave();
    updateStatus(`Deleted node ${removed.name}`);
  }

  function setTab(tabId) {
    activeTab = tabId;
  }

  onMount(() => {
    currentWindowLabel = appWindow.label ?? 'main';

    let disposed = false;
    let cleanup = () => {};

    void bootstrap()
      .then((nextCleanup) => {
        if (disposed) {
          nextCleanup();
          return;
        }
        cleanup = nextCleanup;
      })
      .catch((error) => {
        fatalError = error?.stack ?? error?.message ?? String(error);
      });

    return () => {
      disposed = true;
      cleanup();
    };
  });
</script>

{#if fatalError}
  <pre class="fatal">{fatalError}</pre>
{:else if isDesktopWindow}
  <main class="desktop-overlay stage" class:stage--hidden={!showDesktop}>
    <svg class="links" {viewBox}>
      {#each links as d}
        <path class="link" {d}></path>
      {/each}
    </svg>

    <div class="node-layer" bind:this={nodeLayer}>
      {#each renderNodes as node (node.id)}
        <article
          class="node"
          class:node--expanded={expandedNodeId === node.id}
          use:nodeRef={node.id}
          style={`left:${node.renderX}px;top:${node.renderY}px;`}
          on:pointerdown={(event) => beginDrag(event, node.id)}
          on:contextmenu={(event) => openNodeContextMenu(event, node.id)}
        >
          <div class="node__surface">
            {#if expandedNodeId === node.id}
              <header class="node__header">
                <div class="node__icon">{node.icon ?? '◆'}</div>
                <div class="node__header-copy">
                  <div class="node__name">{node.name}</div>
                  <div class="node__meta">{node.id.slice(0, 8)}</div>
                </div>
                <button class="node__edit-trigger" data-open-editor on:click|stopPropagation={() => openNodeEditor(node.id)}>Edit</button>
              </header>
              <p class="node__body">{node.description ?? 'A linked context node'}</p>
              <div class="node__actions">
                <button on:click|stopPropagation={() => openLinkedFolder(node)}>Open Linked Folder</button>
                <button on:click|stopPropagation={() => launchNode(node, 'open-editor')}>Editor</button>
                <button on:click|stopPropagation={() => launchNode(node, 'open-browser')}>Browser</button>
                <button on:click|stopPropagation={() => launchNode(node, 'run-script')}>Script</button>
              </div>
            {:else}
              <div class="node__compact-title">{node.name}</div>
            {/if}
          </div>
        </article>
      {/each}
    </div>

    {#if contextMenu.open}
      <div
        class="context-menu"
        style={`left:${contextMenu.x}px;top:${contextMenu.y}px;`}
        role="menu"
        tabindex="-1"
        on:pointerdown|stopPropagation
        on:contextmenu|preventDefault
      >
        <div class="context-menu__title">{contextNode?.name ?? 'Node'}</div>
        <button
          on:click={() => {
            if (contextNode) {
              openNodeEditor(contextNode.id);
            }
            closeContextMenu();
          }}
        >
          Edit Node
        </button>
        <button on:click={addConnectedNodeFromMenu}>Add Connecting Node</button>
        <button on:click={connectNearestNodeFromMenu}>Connect Nearest Node</button>
        <button on:click={clearNodeLinksFromMenu}>Clear Node Links</button>
        <button on:click={cloneNodeFromMenu}>Clone Node</button>
        <button class="danger" on:click={deleteNodeFromMenu}>Delete Node</button>
      </div>
    {/if}

    {#if editPopup.open && editNode}
      <div
        class="node-editor-popup"
        style={`left:${editPopup.x}px;top:${editPopup.y}px;`}
        on:pointerdown|stopPropagation
      >
        <div class="node-editor-popup__title">Edit Node</div>
        <label>
          <span>Title</span>
          <input bind:value={editDraft.name} />
        </label>
        <label>
          <span>Description</span>
          <textarea rows="2" bind:value={editDraft.description}></textarea>
        </label>
        <label>
          <span>Linked folder path</span>
          <input bind:value={editDraft.path} placeholder="/home/user/project" />
        </label>
        <label>
          <span>Browser URL</span>
          <input bind:value={editDraft.browser} placeholder="https://..." />
        </label>
        <label>
          <span>Script command</span>
          <input bind:value={editDraft.script} placeholder="npm run build:web" />
        </label>

        <div class="node-editor-popup__links">
          <div class="section__title">Connected Node Links</div>
          {#if nodes.filter((node) => node.id !== editNode.id).length === 0}
            <div class="hint">No other nodes available.</div>
          {:else}
            {#each nodes.filter((node) => node.id !== editNode.id) as candidate (candidate.id)}
              <label class="link-toggle">
                <span>{candidate.name}</span>
                <input
                  type="checkbox"
                  checked={editSelectedLinks.includes(candidate.id)}
                  on:change={(event) => toggleEditLink(candidate.id, event.currentTarget.checked)}
                />
              </label>
            {/each}
          {/if}
        </div>

        <div class="node-editor-popup__actions">
          <button on:click={saveNodeEditor}>Save</button>
          <button class="ghost" on:click={closeNodeEditor}>Cancel</button>
        </div>
      </div>
    {/if}
  </main>
{:else}
  <div class="settings-app">
    <button
      class="ghost-fin"
      title="Reveal FinNode"
      aria-label="Reveal FinNode"
      on:mouseenter={revealFromGhost}
    ></button>

    <aside class="rail rail--settings">
      <div class="brand">
        <div class="brand__mark">⟡</div>
        <div>
          <div class="brand__name">FinNode Settings</div>
          <div class="brand__tag">desktop node control center</div>
        </div>
      </div>

      <div class="rail__tabs">
        {#each settingsTabs as tab}
          <button class:tab--active={activeTab === tab.id} class="tab" on:click={() => setTab(tab.id)}>{tab.label}</button>
        {/each}
      </div>

      <div class="rail__section settings-center">
        {#if activeTab === 'general'}
          <div class="section__title">Startup</div>
          <label class="toggle-row">
            <span>Open on login</span>
            <input
              type="checkbox"
              checked={settings.general.openOnLogin}
              on:change={(event) => updateSettings((draft) => {
                draft.general.openOnLogin = event.currentTarget.checked;
              })}
            />
          </label>
          <label class="toggle-row">
            <span>Start minimized to tray</span>
            <input
              type="checkbox"
              checked={settings.general.startMinimizedToTray}
              on:change={(event) => updateSettings((draft) => {
                draft.general.startMinimizedToTray = event.currentTarget.checked;
              })}
            />
          </label>
          <label class="toggle-row">
            <span>Restore last mode</span>
            <input
              type="checkbox"
              checked={settings.general.restoreLastMode}
              on:change={(event) => updateSettings((draft) => {
                draft.general.restoreLastMode = event.currentTarget.checked;
              })}
            />
          </label>
          <p class="hint">Window size is fixed now. Use tray controls to show or hide desktop nodes.</p>
        {:else if activeTab === 'appearance'}
          <div class="section__title">Appearance</div>
          <label class="toggle-row">
            <span>Show background grid</span>
            <input type="checkbox" checked={settings.appearance.showGrid} on:change={(event) => toggleGrid(event.currentTarget.checked)} />
          </label>
          <label class="slider-row">
            <span>Motion scale: {settings.appearance.motionScale.toFixed(2)}</span>
            <input type="range" min="0.4" max="1.6" step="0.05" value={settings.appearance.motionScale} on:input={(event) => updateMotionScale(event.currentTarget.value)} />
          </label>
          <label class="slider-row">
            <span>Node glow: {settings.appearance.nodeGlow.toFixed(2)}</span>
            <input type="range" min="0.15" max="1" step="0.05" value={settings.appearance.nodeGlow} on:input={(event) => updateNodeGlow(event.currentTarget.value)} />
          </label>
        {:else if activeTab === 'nodes'}
          <div class="section__title">Desktop Nodes</div>
          <button class="chip" on:click={toggleDesktop}>{showDesktop ? 'Hide Desktop Nodes' : 'Show Desktop Nodes'}</button>
          <label class="slider-row">
            <span>Smoothness: {settings.nodes.smoothness.toFixed(2)}</span>
            <input type="range" min="0.08" max="0.45" step="0.01" value={settings.nodes.smoothness} on:input={(event) => updateSmoothness(event.currentTarget.value)} />
          </label>

          <div class="section__title node-manager__title">Node Manager</div>
          <div class="template-row">
            <select bind:value={selectedTemplate}>
              {#each nodeTemplates as template}
                <option value={template.id}>{template.name}</option>
              {/each}
            </select>
            <button class="chip" on:click={addNodeFromTemplate}>Add Node</button>
          </div>

          <div class="hint">Node editing now happens on desktop: click a node circle, then click Edit.</div>

          <div class="node-manager">
            {#each nodes as node, index (node.id)}
              <div class="node-row">
                <div class="node-row__title">{node.name}</div>
                <div class="node-row__actions">
                  <button on:click={() => moveNode(node.id, -1)} disabled={index === 0}>Up</button>
                  <button on:click={() => moveNode(node.id, 1)} disabled={index === nodes.length - 1}>Down</button>
                  <button on:click={() => cloneNode(node.id)}>Clone</button>
                  <button class="danger" on:click={() => deleteNode(node.id)}>Delete</button>
                </div>
              </div>
            {/each}
          </div>
        {:else if activeTab === 'tray'}
          <div class="section__title">Tray Quick Actions</div>
          <button class="chip" on:click={openSettingsFromTray}>Open Settings</button>
          <button class="chip" on:click={toggleStealth}>{stealth ? 'Disable Stealth' : 'Toggle Stealth'}</button>
          <button class="chip" on:click={toggleDesktop}>{showDesktop ? 'Hide Desktop Nodes' : 'Show Desktop Nodes'}</button>
          <button class="chip" on:click={hideToTray}>Hide To Tray</button>
          <button class="chip chip--danger" on:click={exitApp}>Exit</button>
        {:else}
          <div class="section__title">Shortcuts</div>
          <div class="hint">Stealth Toggle: {settings.shortcuts.toggleStealth}</div>
          <div class="hint">Tray click: Open Settings</div>
          <div class="hint">Tip: right-click any desktop node for connection actions.</div>
        {/if}
      </div>

      <div class="rail__section meter">
        <div class="section__title">Activity</div>
        <div class="activity-list">
          {#if activityLog.length === 0}
            <div class="hint">No activity yet.</div>
          {:else}
            {#each activityLog as item (item.id)}
              <div class="activity-item">{item.text}</div>
            {/each}
          {/if}
        </div>
      </div>

      <div class="status-bar status-bar--settings">
        <span>{statusText}</span>
        <span class="status-dot"></span>
      </div>
    </aside>
  </div>
{/if}

<style>
  :global(:root) {
    color-scheme: dark;
    --bg: #081321;
    --panel: rgba(9, 18, 31, 0.88);
    --panel-strong: rgba(13, 24, 39, 0.96);
    --line: rgba(120, 227, 255, 0.25);
    --text: #e8f7ff;
    --muted: rgba(200, 238, 255, 0.72);
    --glow: rgba(124, 244, 255, 0.45);
    --accent: #7cf4ff;
    --accent-2: #9dffb9;
    --danger: #ff8fa3;
    --shadow: 0 20px 56px rgba(0, 0, 0, 0.42);
    --motion-scale: 1;
    --node-glow: 0.45;
    --grid-opacity: 0.4;
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
      radial-gradient(circle at 18% 15%, rgba(52, 201, 255, 0.2), transparent 34%),
      radial-gradient(circle at 88% 8%, rgba(157, 255, 185, 0.16), transparent 28%),
      linear-gradient(145deg, #050b14 0%, #0a1628 55%, #050b14 100%);
    color: var(--text);
  }

  :global(body)::before {
    content: '';
    position: fixed;
    inset: 0;
    pointer-events: none;
    background-image: linear-gradient(rgba(255, 255, 255, 0.03) 1px, transparent 1px), linear-gradient(90deg, rgba(255, 255, 255, 0.03) 1px, transparent 1px);
    background-size: 44px 44px;
    mask-image: radial-gradient(circle at center, black 45%, transparent 82%);
    opacity: var(--grid-opacity);
  }

  :global(body.desktop-overlay-window) {
    background: transparent;
  }

  :global(body.desktop-overlay-window)::before {
    opacity: 0;
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

  .settings-app {
    position: relative;
    display: flex;
    width: 100%;
    height: 100%;
  }

  .desktop-overlay {
    position: relative;
    width: 100%;
    height: 100%;
    background: transparent;
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
    gap: 14px;
    padding: 20px;
    background: linear-gradient(180deg, rgba(10, 18, 30, 0.98), rgba(7, 14, 24, 0.95));
    border-right: 1px solid rgba(124, 244, 255, 0.2);
    backdrop-filter: blur(24px) saturate(160%);
    box-shadow: inset -1px 0 0 rgba(255, 255, 255, 0.06);
    transition: transform 260ms ease, opacity 260ms ease;
  }

  .rail--settings {
    width: 100%;
    border-right: 0;
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
    font-size: 1.32rem;
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
    padding: 16px;
    border: 1px solid rgba(124, 244, 255, 0.2);
    border-radius: 18px;
    background: rgba(6, 12, 20, 0.52);
  }

  .rail__tabs {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 7px;
  }

  .tab {
    border: 1px solid rgba(124, 244, 255, 0.2);
    background: rgba(8, 15, 26, 0.6);
    color: var(--muted);
    border-radius: 11px;
    padding: 9px 8px;
    font: inherit;
    font-size: 0.76rem;
    cursor: pointer;
    transition: border-color 140ms ease, color 140ms ease, background 140ms ease, transform 140ms ease;
  }

  .tab:hover {
    transform: translateY(-1px);
    border-color: rgba(124, 244, 255, 0.38);
  }

  .tab--active {
    color: var(--text);
    border-color: rgba(124, 244, 255, 0.45);
    background: rgba(16, 30, 45, 0.88);
    box-shadow: 0 0 18px rgba(124, 244, 255, 0.16);
  }

  .settings-center {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 300px;
  }

  .toggle-row,
  .slider-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 10px;
    color: rgba(233, 248, 255, 0.92);
    font-size: 0.84rem;
    margin-top: 8px;
  }

  .toggle-row input {
    accent-color: #7cf4ff;
  }

  .slider-row input[type='range'] {
    width: 46%;
  }

  .template-row {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 10px;
    margin-top: 6px;
  }

  .template-row select,
  .node-editor-popup input,
  .node-editor-popup textarea {
    width: 100%;
    border: 1px solid rgba(124, 244, 255, 0.22);
    background: rgba(8, 15, 26, 0.8);
    color: var(--text);
    border-radius: 12px;
    padding: 8px 10px;
    font: inherit;
  }

  .node-manager {
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-height: 180px;
    overflow: auto;
    margin-top: 8px;
    padding-right: 2px;
  }

  .node-manager__title {
    margin-top: 14px;
  }

  .node-row {
    display: flex;
    flex-direction: row;
    justify-content: space-between;
    align-items: center;
    gap: 10px;
    padding: 9px 10px;
    border: 1px solid rgba(124, 244, 255, 0.12);
    border-radius: 12px;
    background: rgba(8, 14, 23, 0.58);
  }

  .node-row__title {
    font-size: 0.82rem;
    font-weight: 600;
    color: rgba(233, 248, 255, 0.94);
    max-width: 120px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .node-row__actions {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 6px;
    width: 220px;
  }

  .node-row__actions button {
    border: 1px solid rgba(124, 244, 255, 0.2);
    background: rgba(8, 15, 26, 0.78);
    color: var(--text);
    border-radius: 10px;
    padding: 6px 8px;
    font-size: 0.72rem;
    cursor: pointer;
  }

  .node-row__actions button:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .node-row__actions .danger {
    border-color: rgba(255, 143, 163, 0.4);
    color: #ffd9e1;
  }

  .activity-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-height: 110px;
    overflow: auto;
  }

  .activity-item {
    font-size: 0.76rem;
    line-height: 1.35;
    color: rgba(233, 248, 255, 0.8);
    border-bottom: 1px dashed rgba(124, 244, 255, 0.16);
    padding-bottom: 6px;
  }

  .section__title {
    margin-bottom: 10px;
    text-transform: uppercase;
    letter-spacing: 0.16em;
    font-size: 0.72rem;
  }

  .chip,
  .node__actions button,
  .node-editor-popup__actions button {
    border: 1px solid rgba(124, 244, 255, 0.22);
    background: rgba(8, 15, 26, 0.78);
    color: var(--text);
    border-radius: 12px;
    padding: 9px 12px;
    font: inherit;
    font-size: 0.8rem;
    cursor: pointer;
    transition: transform 140ms ease, border-color 140ms ease, box-shadow 140ms ease;
  }

  .chip:hover,
  .node__actions button:hover {
    transform: translateY(-1px);
    border-color: rgba(124, 244, 255, 0.45);
    box-shadow: 0 0 18px rgba(124, 244, 255, 0.16);
  }

  .chip {
    width: 100%;
    margin-top: 8px;
  }

  .chip--danger {
    border-color: rgba(255, 143, 163, 0.4);
    color: #ffd9e1;
  }

  .chip:disabled {
    opacity: 0.45;
    cursor: not-allowed;
    transform: none;
    box-shadow: none;
  }

  .stage {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
    transition: opacity calc(220ms * var(--motion-scale)) ease, transform calc(220ms * var(--motion-scale)) ease;
  }

  .stage--hidden {
    opacity: 0;
    transform: scale(0.98);
    pointer-events: none;
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
    padding: 26px;
  }

  .node {
    position: absolute;
    width: 84px;
    height: 84px;
    border-radius: 50%;
    cursor: grab;
    user-select: none;
    will-change: left, top;
    z-index: 2;
    transition: left calc(120ms * var(--motion-scale)) cubic-bezier(0.22, 0.61, 0.36, 1), top calc(120ms * var(--motion-scale)) cubic-bezier(0.22, 0.61, 0.36, 1);
  }

  .node.node--expanded {
    width: 272px;
    height: auto;
    min-height: 190px;
    border-radius: 22px;
    z-index: 5;
  }

  .node__surface {
    position: relative;
    width: 100%;
    height: 100%;
    display: grid;
    place-items: center;
    padding: 12px;
    border-radius: inherit;
    background: linear-gradient(180deg, rgba(18, 27, 41, 0.94), rgba(10, 15, 24, 0.82));
    border: 1px solid rgba(124, 244, 255, 0.2);
    box-shadow: var(--shadow), 0 0 calc(30px * var(--node-glow)) rgba(124, 244, 255, calc(0.2 * var(--node-glow))) inset;
    backdrop-filter: blur(20px) saturate(150%);
    animation: drift calc(18s * var(--motion-scale)) ease-in-out infinite;
    transition: transform calc(140ms * var(--motion-scale)) ease, box-shadow calc(140ms * var(--motion-scale)) ease;
    will-change: transform;
  }

  .node__surface::after {
    content: '';
    position: absolute;
    inset: 0;
    border-radius: inherit;
    box-shadow: 0 0 calc(24px * var(--node-glow)) rgba(124, 244, 255, calc(0.14 * var(--node-glow)));
    pointer-events: none;
  }

  .node--expanded .node__surface {
    display: flex;
    flex-direction: column;
    min-height: 190px;
    padding: 16px;
    border-radius: 22px;
    align-items: stretch;
  }

  .node__compact-title {
    max-width: 90%;
    text-align: center;
    font-size: 0.74rem;
    letter-spacing: 0.03em;
    font-weight: 700;
    line-height: 1.15;
    color: rgba(233, 248, 255, 0.95);
    text-shadow: 0 0 12px rgba(124, 244, 255, 0.26);
  }

  :global(.node.is-dragging) {
    cursor: grabbing;
    transition: none;
  }

  :global(.node.is-dragging) .node__surface {
    animation-play-state: paused;
    transform: scale(1.02);
  }

  .node__header {
    display: flex;
    gap: 10px;
    align-items: center;
  }

  .node__header-copy {
    min-width: 0;
    flex: 1;
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

  .node__edit-trigger {
    border: 1px solid rgba(124, 244, 255, 0.25);
    background: rgba(7, 14, 24, 0.86);
    color: var(--text);
    border-radius: 10px;
    padding: 5px 10px;
    font: inherit;
    font-size: 0.76rem;
    cursor: pointer;
  }

  .node__edit-trigger:hover {
    border-color: rgba(124, 244, 255, 0.45);
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
    margin: 12px 0;
    color: rgba(233, 248, 255, 0.82);
    line-height: 1.4;
    font-size: 0.82rem;
  }

  .node__actions {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px;
  }

  .status-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 14px;
    border-radius: 14px;
    background: rgba(5, 9, 16, 0.58);
    border: 1px solid rgba(124, 244, 255, 0.2);
    backdrop-filter: blur(16px) saturate(150%);
    color: var(--muted);
    font-size: 0.82rem;
  }

  .status-bar--settings {
    margin-top: auto;
  }

  .context-menu {
    position: fixed;
    width: 236px;
    z-index: 120;
    border: 1px solid rgba(124, 244, 255, 0.28);
    border-radius: 14px;
    background: rgba(8, 14, 24, 0.95);
    box-shadow: 0 18px 40px rgba(0, 0, 0, 0.45), 0 0 18px rgba(124, 244, 255, 0.15);
    backdrop-filter: blur(18px);
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .context-menu__title {
    font-size: 0.82rem;
    color: var(--muted);
    letter-spacing: 0.03em;
    padding: 2px 4px 8px;
    border-bottom: 1px solid rgba(124, 244, 255, 0.14);
    margin-bottom: 2px;
  }

  .context-menu button {
    border: 1px solid rgba(124, 244, 255, 0.2);
    background: rgba(10, 18, 29, 0.88);
    color: var(--text);
    border-radius: 10px;
    padding: 8px 10px;
    font: inherit;
    font-size: 0.82rem;
    text-align: left;
    cursor: pointer;
    transition: border-color 120ms ease, background 120ms ease;
  }

  .context-menu button:hover {
    border-color: rgba(124, 244, 255, 0.42);
    background: rgba(14, 28, 43, 0.95);
  }

  .context-menu button.danger {
    border-color: rgba(255, 143, 163, 0.35);
    color: #ffd9e1;
  }

  .node-editor-popup {
    position: fixed;
    width: 320px;
    max-height: min(420px, calc(100vh - 24px));
    overflow: auto;
    z-index: 130;
    border: 1px solid rgba(124, 244, 255, 0.35);
    border-radius: 14px;
    background: rgba(8, 14, 24, 0.96);
    box-shadow: 0 20px 48px rgba(0, 0, 0, 0.5), 0 0 20px rgba(124, 244, 255, 0.18);
    backdrop-filter: blur(16px);
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .node-editor-popup__title {
    font-size: 0.88rem;
    font-weight: 700;
    color: rgba(233, 248, 255, 0.95);
    margin-bottom: 2px;
  }

  .node-editor-popup label {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .node-editor-popup label > span {
    font-size: 0.74rem;
    letter-spacing: 0.03em;
    color: var(--muted);
  }

  .node-editor-popup textarea {
    resize: vertical;
    min-height: 58px;
  }

  .node-editor-popup__links {
    border-top: 1px solid rgba(124, 244, 255, 0.15);
    margin-top: 4px;
    padding-top: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .link-toggle {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    font-size: 0.78rem;
    color: rgba(233, 248, 255, 0.9);
  }

  .link-toggle input {
    accent-color: #7cf4ff;
  }

  .node-editor-popup__actions {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px;
    margin-top: 6px;
  }

  .node-editor-popup__actions .ghost {
    background: rgba(10, 18, 28, 0.65);
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
    min-height: 0;
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

  @media (max-width: 760px) {
    .node-editor-popup {
      width: min(92vw, 320px);
    }

    .node-layer {
      padding: 18px;
    }

    .node.node--expanded {
      width: min(90vw, 272px);
    }
  }
</style>
