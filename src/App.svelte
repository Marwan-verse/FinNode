<script>
  import { onMount, tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/tauri';
  import { listen } from '@tauri-apps/api/event';
  import { appWindow } from '@tauri-apps/api/window';
  import appLogo from '../src-tauri/icons/icon.png';

  const MAIN_NODE_ID = 'main-node';
  const MAIN_NODE_NAME = 'main';
  const LOGO_ICON = 'logo';
  const NODE_SIZE = 96;
  const SAVE_DEBOUNCE_MS = 220;
  const SPRING_STIFFNESS = 0.2;
  const SPRING_DAMPING = 0.82;
  const MAX_NODE_ICON_BYTES = 512 * 1024;

  const NODE_COLORS = ['slate', 'cyan', 'green', 'amber', 'rose', 'violet'];
  const NODE_COLOR_MAP = {
    slate: '#8fa3b5',
    cyan: '#4dd0e1',
    green: '#79d089',
    amber: '#f6c25b',
    rose: '#f08aa3',
    violet: '#b092ff'
  };

  let nodes = [];
  let renderNodes = [];
  let links = [];
  let viewBox = '0 0 1 1';

  let workspaces = [];
  let activeWorkspaceId = 'default';
  let workspaceName = '';
  let commandHistoryCache = [];

  let statusText = 'Loading...';
  let activityLog = [];
  let fatalError = '';
  let currentWindowLabel = 'main';
  let isNodeBoardWindow = false;

  let nodeLayer;
  const nodeElements = new Map();
  let renderQueued = false;

  let smoothNodes = [];
  let smoothLookup = new Map();
  let springFrame = null;

  let selectedIds = new Set();
  let hoveredId = null;
  let highlightNodeId = null;
  let expandedNodeId = null;
  let suppressExpandNodeId = null;

  let draggingId = null;
  let dragOffset = { x: 0, y: 0 };
  let dragStart = { x: 0, y: 0 };
  let dragMoved = false;
  let pendingPointer = null;
  let dragFrame = null;

  let saveTimer = null;

  let contextMenu = { open: false, x: 0, y: 0, nodeId: null };
  let contextNode = null;

  let editPopup = { open: false, nodeId: null };
  let editNode = null;
  let editDraft = createEditDraft();
  let editSelectedLinks = [];

  let showLauncher = false;
  let launcherQuery = '';
  let launcherIndex = 0;
  let launcherResults = [];

  function createEditDraft() {
    return {
      name: '',
      icon: '',
      description: '',
      path: '',
      editor: '',
      browser: '',
      script: '',
      color: 'slate'
    };
  }

  function uid(prefix) {
    return `${prefix}-${Math.random().toString(36).slice(2, 8)}`;
  }

  function clamp(value, min, max) {
    return Math.max(min, Math.min(value, max));
  }

  function normalizeOptionalString(value) {
    if (typeof value !== 'string') return null;
    const trimmed = value.trim();
    return trimmed ? trimmed : null;
  }

  function isLockedNode(nodeOrId) {
    const id = typeof nodeOrId === 'string' ? nodeOrId : nodeOrId?.id;
    return id === MAIN_NODE_ID || Boolean(nodeOrId?.locked);
  }

  function isLogoIcon(icon) {
    return icon === LOGO_ICON || icon === 'fin';
  }

  function isImageIcon(icon) {
    return typeof icon === 'string' && icon.startsWith('data:image/');
  }

  function nodeColor(node) {
    return NODE_COLOR_MAP[node?.color] || NODE_COLOR_MAP.slate;
  }

  function createMainNode(anchor) {
    let x = 30;
    let y = 30;
    if (anchor) {
      x = Math.max(20, Number(anchor.x) - 130);
      y = Math.max(20, Number(anchor.y) - 30);
    }
    return {
      id: MAIN_NODE_ID,
      name: MAIN_NODE_NAME,
      icon: LOGO_ICON,
      description: 'Core entry node',
      x,
      y,
      links: [],
      targets: {
        path: null,
        editor: null,
        browser: null,
        script: null
      },
      color: 'cyan',
      locked: true,
      last_launched: null
    };
  }

  function createEmptyNode(x, y) {
    return {
      id: uid('node'),
      name: '',
      icon: '',
      description: '',
      x,
      y,
      links: [],
      targets: {
        path: null,
        editor: null,
        browser: null,
        script: null
      },
      color: 'slate',
      locked: false,
      last_launched: null
    };
  }

  function normalizeNode(raw, index = 0) {
    const targets = raw?.targets ?? {};
    return {
      id: typeof raw?.id === 'string' && raw.id ? raw.id : uid('node'),
      name: typeof raw?.name === 'string' ? raw.name : '',
      icon: typeof raw?.icon === 'string' ? raw.icon : '',
      description: typeof raw?.description === 'string' ? raw.description : '',
      x: Number.isFinite(Number(raw?.x)) ? Number(raw.x) : 40 + (index % 4) * 130,
      y: Number.isFinite(Number(raw?.y)) ? Number(raw.y) : 40 + Math.floor(index / 4) * 130,
      links: Array.isArray(raw?.links) ? raw.links.filter((id) => typeof id === 'string') : [],
      targets: {
        path: normalizeOptionalString(targets.path),
        editor: normalizeOptionalString(targets.editor),
        browser: normalizeOptionalString(targets.browser),
        script: normalizeOptionalString(targets.script)
      },
      color: NODE_COLORS.includes(raw?.color) ? raw.color : 'slate',
      locked: Boolean(raw?.locked),
      last_launched: typeof raw?.last_launched === 'string' ? raw.last_launched : null
    };
  }

  function ensureMainNode(list) {
    let changed = false;
    let hasMain = false;

    const normalized = list.map((node) => {
      if (node.id !== MAIN_NODE_ID) return node;
      hasMain = true;
      const forced = {
        ...node,
        name: MAIN_NODE_NAME,
        icon: LOGO_ICON,
        locked: true
      };
      if (forced.name !== node.name || forced.icon !== node.icon || !node.locked) {
        changed = true;
      }
      return forced;
    });

    if (hasMain) return { nodes: normalized, changed };

    const anchor = normalized[0];
    return {
      nodes: [createMainNode(anchor), ...normalized],
      changed: true
    };
  }

  function createDefaultWorkspace() {
    return {
      id: 'default',
      name: 'Default',
      nodes: [createMainNode(null)],
      zoom: 1,
      pan_x: 0,
      pan_y: 0
    };
  }

  function normalizeWorkspace(raw, index) {
    const id = typeof raw?.id === 'string' && raw.id ? raw.id : `ws-${index + 1}`;
    const name = typeof raw?.name === 'string' && raw.name.trim() ? raw.name.trim() : `Workspace ${index + 1}`;
    const nodeList = Array.isArray(raw?.nodes) ? raw.nodes.map((node, i) => normalizeNode(node, i)) : [];
    const ensured = ensureMainNode(nodeList);
    return {
      id,
      name,
      nodes: ensured.nodes,
      zoom: 1,
      pan_x: 0,
      pan_y: 0
    };
  }

  function normalizeWorkspaces(rawList) {
    if (!Array.isArray(rawList) || rawList.length === 0) {
      return [createDefaultWorkspace()];
    }

    const ids = new Set();
    const normalized = rawList.map((raw, index) => {
      const ws = normalizeWorkspace(raw, index);
      if (ids.has(ws.id)) {
        ws.id = uid('ws');
      }
      ids.add(ws.id);
      return ws;
    });

    return normalized.length ? normalized : [createDefaultWorkspace()];
  }

  function recordActivity(message) {
    const stamp = new Date().toLocaleTimeString();
    activityLog = [{ id: `${Date.now()}-${Math.random().toString(36).slice(2, 7)}`, text: `${stamp} ${message}` }, ...activityLog].slice(0, 8);
  }

  function updateStatus(message) {
    statusText = message;
    recordActivity(message);
  }

  function updateActiveWorkspaceNodes(nextNodes) {
    workspaces = workspaces.map((workspace) => {
      if (workspace.id !== activeWorkspaceId) return workspace;
      return {
        ...workspace,
        nodes: nextNodes,
        zoom: 1,
        pan_x: 0,
        pan_y: 0
      };
    });
  }

  async function persistLayout() {
    updateActiveWorkspaceNodes(nodes);
    await invoke('save_layout', {
      layout: {
        active_workspace: activeWorkspaceId,
        workspaces,
        command_history: commandHistoryCache
      }
    });
  }

  async function flushPendingSave() {
    if (saveTimer === null) return;
    clearTimeout(saveTimer);
    saveTimer = null;
    await persistLayout();
  }

  function scheduleSave() {
    if (saveTimer !== null) {
      clearTimeout(saveTimer);
    }
    saveTimer = setTimeout(async () => {
      saveTimer = null;
      try {
        await persistLayout();
      } catch (error) {
        updateStatus(String(error));
      }
    }, SAVE_DEBOUNCE_MS);
  }

  function applyLayout(layout) {
    commandHistoryCache = Array.isArray(layout?.command_history) ? layout.command_history : [];

    workspaces = normalizeWorkspaces(layout?.workspaces ?? []);

    let nextActive = typeof layout?.active_workspace === 'string' ? layout.active_workspace : workspaces[0].id;
    if (!workspaces.some((workspace) => workspace.id === nextActive)) {
      nextActive = workspaces[0].id;
    }
    activeWorkspaceId = nextActive;

    const workspace = workspaces.find((item) => item.id === activeWorkspaceId) ?? workspaces[0];
    nodes = workspace.nodes;
    syncSmooth(true);

    void tick().then(() => {
      clampAllNodesToCanvas(false);
      queueRender();
    });
  }

  async function loadWorkspaces() {
    try {
      const layout = await invoke('load_layout');
      applyLayout(layout);
      updateStatus(`Loaded ${nodes.length} nodes`);
    } catch (error) {
      updateStatus(String(error));
    }
  }

  async function switchWorkspace(workspaceId) {
    if (!workspaceId || workspaceId === activeWorkspaceId) return;
    try {
      await flushPendingSave();
      const layout = await invoke('switch_workspace', { workspaceId });
      applyLayout(layout);
      const workspace = workspaces.find((item) => item.id === activeWorkspaceId);
      updateStatus(`Switched to ${workspace?.name ?? 'workspace'}`);
    } catch (error) {
      updateStatus(String(error));
    }
  }

  async function createWorkspace() {
    const name = workspaceName.trim();
    if (!name) return;

    try {
      await flushPendingSave();
      const created = await invoke('create_workspace', { name });
      workspaceName = '';
      await loadWorkspaces();
      if (created && typeof created.id === 'string') {
        await switchWorkspace(created.id);
      }
      updateStatus('Workspace created');
    } catch (error) {
      updateStatus(String(error));
    }
  }

  async function deleteWorkspace(workspaceId) {
    if (!workspaceId || workspaces.length <= 1) return;

    try {
      await flushPendingSave();
      await invoke('delete_workspace', { workspaceId });
      await loadWorkspaces();
      updateStatus('Workspace deleted');
    } catch (error) {
      updateStatus(String(error));
    }
  }

  async function launchNode(node, action) {
    try {
      await invoke('launch_node', { node, action });
      nodes = nodes.map((item) => {
        if (item.id !== node.id) return item;
        return {
          ...item,
          last_launched: new Date().toISOString()
        };
      });
      queueRender();
      updateStatus(`Ran ${action} on ${node.name || 'node'}`);
    } catch (error) {
      updateStatus(String(error));
    }
  }

  async function minimizeToTray() {
    try {
      await invoke('hide_main_window');
    } catch (error) {
      updateStatus(`Unable to minimize: ${String(error)}`);
    }
  }

  async function shutdownApp() {
    try {
      await invoke('exit_app');
    } catch (error) {
      updateStatus(`Unable to shutdown: ${String(error)}`);
    }
  }

  function addNode() {
    const offset = nodes.length * 14;
    const next = createEmptyNode(50 + offset, 70 + offset);
    nodes = [...nodes, next];
    syncSmooth(true);
    scheduleSave();
    void tick().then(() => {
      clampAllNodesToCanvas(true);
      queueRender();
    });
    updateStatus('Added node');
  }

  function cloneNode(nodeId) {
    if (isLockedNode(nodeId)) return;
    const source = nodes.find((node) => node.id === nodeId);
    if (!source) return;

    const clone = {
      ...source,
      id: uid('node'),
      name: source.name ? `${source.name} copy` : 'Node copy',
      x: source.x + 24,
      y: source.y + 24,
      links: [...(source.links ?? [])],
      targets: { ...(source.targets ?? {}) },
      locked: false
    };

    nodes = [...nodes, clone];
    syncSmooth(true);
    scheduleSave();
    void tick().then(() => {
      clampAllNodesToCanvas(true);
      queueRender();
    });
    updateStatus('Node cloned');
  }

  function deleteNode(nodeId) {
    if (isLockedNode(nodeId)) {
      updateStatus('Main node is locked');
      return;
    }

    if (contextMenu.nodeId === nodeId) closeCtx();
    if (editPopup.nodeId === nodeId) closeEditor();

    nodes = nodes
      .filter((node) => node.id !== nodeId)
      .map((node) => ({
        ...node,
        links: (node.links ?? []).filter((id) => id !== nodeId)
      }));

    syncSmooth(true);
    scheduleSave();
    updateStatus('Node deleted');
  }

  function layoutGrid() {
    const cols = Math.max(1, Math.ceil(Math.sqrt(nodes.length)));
    const gapX = 140;
    const gapY = 128;

    nodes = nodes.map((node, index) => ({
      ...node,
      x: 36 + (index % cols) * gapX,
      y: 36 + Math.floor(index / cols) * gapY
    }));

    syncSmooth(true);
    scheduleSave();
    void tick().then(() => {
      clampAllNodesToCanvas(true);
      queueRender();
    });
    updateStatus('Auto layout applied');
  }

  function toggleSelect(nodeId, event) {
    if (!event.ctrlKey && !event.metaKey) return false;
    const next = new Set(selectedIds);
    if (next.has(nodeId)) next.delete(nodeId);
    else next.add(nodeId);
    selectedIds = next;
    return true;
  }

  function batchLaunch() {
    for (const id of selectedIds) {
      const node = nodes.find((item) => item.id === id);
      if (node) {
        void launchNode(node, 'open-path');
      }
    }
    selectedIds = new Set();
  }

  function batchDelete() {
    const toDelete = [...selectedIds];
    for (const id of toDelete) {
      deleteNode(id);
    }
    selectedIds = new Set();
  }

  function clampNodePosition(node, x, y) {
    if (!nodeLayer) return { x, y };

    const layerRect = nodeLayer.getBoundingClientRect();
    const element = nodeElements.get(node.id);
    const width = element ? element.offsetWidth : NODE_SIZE;
    const height = element ? element.offsetHeight : NODE_SIZE;

    return {
      x: clamp(x, 0, Math.max(0, layerRect.width - width)),
      y: clamp(y, 0, Math.max(0, layerRect.height - height))
    };
  }

  function clampAllNodesToCanvas(save = false) {
    if (!nodeLayer || nodes.length === 0) return;

    let changed = false;
    const next = nodes.map((node) => {
      const clamped = clampNodePosition(node, node.x, node.y);
      if (clamped.x === node.x && clamped.y === node.y) return node;
      changed = true;
      return {
        ...node,
        x: clamped.x,
        y: clamped.y
      };
    });

    if (!changed) return;

    nodes = next;
    syncSmooth(true);
    queueRender();
    if (save) scheduleSave();
  }

  function beginDrag(event, nodeId) {
    if (event.button !== 0) return;
    if (event.target instanceof Element && event.target.closest('button, input, textarea, select, label, a')) return;
    if (toggleSelect(nodeId, event)) return;

    const node = nodes.find((item) => item.id === nodeId);
    if (!node) return;

    closeCtx();
    closeEditor();

    draggingId = nodeId;
    dragMoved = false;
    dragStart = { x: event.clientX, y: event.clientY };

    const rect = event.currentTarget.getBoundingClientRect();
    dragOffset = {
      x: event.clientX - rect.left,
      y: event.clientY - rect.top
    };

    event.preventDefault();
  }

  function onPointerMove(event) {
    if (!draggingId || !nodeLayer) return;

    if (!dragMoved) {
      dragMoved = Math.abs(event.clientX - dragStart.x) > 4 || Math.abs(event.clientY - dragStart.y) > 4;
    }

    pendingPointer = { x: event.clientX, y: event.clientY };
    if (dragFrame !== null) return;

    dragFrame = requestAnimationFrame(() => {
      dragFrame = null;
      if (!pendingPointer || !draggingId || !nodeLayer) return;

      const pointer = pendingPointer;
      pendingPointer = null;

      const node = nodes.find((item) => item.id === draggingId);
      if (!node) return;

      const layerRect = nodeLayer.getBoundingClientRect();
      const next = clampNodePosition(node, pointer.x - layerRect.left - dragOffset.x, pointer.y - layerRect.top - dragOffset.y);

      node.x = next.x;
      node.y = next.y;
      nodes = [...nodes];
      queueRender();
    });
  }

  function onPointerUp() {
    if (!draggingId) return;

    const releasedId = draggingId;

    if (dragFrame !== null) {
      cancelAnimationFrame(dragFrame);
      dragFrame = null;
    }

    pendingPointer = null;

    const moved = dragMoved;
    draggingId = null;
    dragMoved = false;

    if (moved) {
      syncSmooth(true);
      scheduleSave();
      suppressExpandNodeId = releasedId;
      setTimeout(() => {
        if (suppressExpandNodeId === releasedId) suppressExpandNodeId = null;
      }, 0);
    }
  }

  function onNodeClick(event, nodeId) {
    if (event.button !== 0) return;
    if (suppressExpandNodeId === nodeId) {
      suppressExpandNodeId = null;
      return;
    }

    expandedNodeId = expandedNodeId === nodeId ? null : nodeId;
  }

  function onNodeKeydown(event, nodeId) {
    if (event.key !== 'Enter' && event.key !== ' ') return;
    event.preventDefault();
    expandedNodeId = expandedNodeId === nodeId ? null : nodeId;
  }

  function nodeRef(element, id) {
    nodeElements.set(id, element);
    queueRender();

    return {
      destroy() {
        nodeElements.delete(id);
        queueRender();
      }
    };
  }

  function queueRender() {
    if (renderQueued) return;
    renderQueued = true;

    void tick().then(() => {
      renderQueued = false;
      renderConnections();
    });
  }

  function centerOf(element) {
    const rect = element.getBoundingClientRect();
    const layerRect = nodeLayer.getBoundingClientRect();
    return {
      x: rect.left - layerRect.left + rect.width / 2,
      y: rect.top - layerRect.top + rect.height / 2,
      radius: Math.max(20, Math.min(rect.width, rect.height) / 2 - 6)
    };
  }

  function renderConnections() {
    if (!nodeLayer) return;

    const bounds = nodeLayer.getBoundingClientRect();
    if (!bounds.width || !bounds.height) return;

    viewBox = `0 0 ${bounds.width} ${bounds.height}`;

    const next = [];
    for (const node of renderNodes) {
      for (const targetId of node.links ?? []) {
        const from = nodeElements.get(node.id);
        const to = nodeElements.get(targetId);
        if (!from || !to) continue;

        const startCenter = centerOf(from);
        const endCenter = centerOf(to);

        const dx = endCenter.x - startCenter.x;
        const dy = endCenter.y - startCenter.y;
        const distance = Math.hypot(dx, dy) || 1;
        const ux = dx / distance;
        const uy = dy / distance;

        const start = {
          x: startCenter.x + ux * startCenter.radius,
          y: startCenter.y + uy * startCenter.radius
        };
        const end = {
          x: endCenter.x - ux * endCenter.radius,
          y: endCenter.y - uy * endCenter.radius
        };

        const bend = Math.min(120, distance * 0.34);
        const c1 = { x: start.x + ux * bend, y: start.y + uy * bend };
        const c2 = { x: end.x - ux * bend, y: end.y - uy * bend };

        next.push({
          from: node.id,
          to: targetId,
          d: `M ${start.x} ${start.y} C ${c1.x} ${c1.y}, ${c2.x} ${c2.y}, ${end.x} ${end.y}`
        });
      }
    }

    links = next;
  }

  function syncSmooth(immediate = false) {
    const previous = new Map(smoothNodes.map((item) => [item.id, item]));
    smoothNodes = nodes.map((node) => {
      const existing = previous.get(node.id);
      if (!existing || immediate) {
        return { id: node.id, x: node.x, y: node.y, vx: 0, vy: 0 };
      }
      return existing;
    });
    startSpring();
  }

  function startSpring() {
    if (springFrame !== null) return;
    springFrame = requestAnimationFrame(stepSpring);
  }

  function stepSpring() {
    springFrame = null;
    if (!smoothNodes.length) return;

    const targetById = new Map(nodes.map((node) => [node.id, node]));
    let active = false;

    smoothNodes = smoothNodes.map((item) => {
      const target = targetById.get(item.id);
      if (!target) return item;
      if (draggingId === item.id) {
        return { ...item, x: target.x, y: target.y, vx: 0, vy: 0 };
      }

      const dx = target.x - item.x;
      const dy = target.y - item.y;
      const vx = (item.vx + dx * SPRING_STIFFNESS) * SPRING_DAMPING;
      const vy = (item.vy + dy * SPRING_STIFFNESS) * SPRING_DAMPING;

      if (Math.abs(dx) > 0.15 || Math.abs(dy) > 0.15 || Math.abs(vx) > 0.15 || Math.abs(vy) > 0.15) {
        active = true;
      }

      return {
        ...item,
        x: item.x + vx,
        y: item.y + vy,
        vx,
        vy
      };
    });

    queueRender();
    if (active) {
      springFrame = requestAnimationFrame(stepSpring);
    }
  }

  function closeCtx() {
    if (!contextMenu.open) return;
    contextMenu = { open: false, x: 0, y: 0, nodeId: null };
    highlightNodeId = hoveredId;
  }

  function closeEditor() {
    if (!editPopup.open) return;
    editPopup = { open: false, nodeId: null };
    editDraft = createEditDraft();
    editSelectedLinks = [];
  }

  function openEditor(nodeId) {
    if (isLockedNode(nodeId)) return;
    const node = nodes.find((item) => item.id === nodeId);
    if (!node) return;

    editDraft = {
      name: node.name ?? '',
      icon: node.icon ?? '',
      description: node.description ?? '',
      path: node.targets?.path ?? '',
      editor: node.targets?.editor ?? '',
      browser: node.targets?.browser ?? '',
      script: node.targets?.script ?? '',
      color: node.color ?? 'slate'
    };
    editSelectedLinks = [...(node.links ?? [])];
    editPopup = { open: true, nodeId };
  }

  function clearUploadedNodeIcon() {
    editDraft = {
      ...editDraft,
      icon: ''
    };
  }

  function handleNodeIconUpload(event) {
    const input = event.currentTarget;
    const file = input.files?.[0];
    if (!file) return;

    if (!file.type.startsWith('image/')) {
      updateStatus('Please choose an image file');
      input.value = '';
      return;
    }

    if (file.size > MAX_NODE_ICON_BYTES) {
      updateStatus('Icon image must be 512KB or smaller');
      input.value = '';
      return;
    }

    const reader = new FileReader();
    reader.onload = () => {
      if (typeof reader.result !== 'string') return;
      editDraft = {
        ...editDraft,
        icon: reader.result
      };
      updateStatus('Node image uploaded');
    };
    reader.readAsDataURL(file);
    input.value = '';
  }

  function toggleEditLink(targetId, enabled) {
    if (enabled) {
      editSelectedLinks = [...new Set([...editSelectedLinks, targetId])];
    } else {
      editSelectedLinks = editSelectedLinks.filter((id) => id !== targetId);
    }
  }

  function saveEditor() {
    if (!editPopup.nodeId || isLockedNode(editPopup.nodeId)) return;

    const nodeId = editPopup.nodeId;
    const normalizedIcon = isImageIcon(editDraft.icon) ? editDraft.icon : editDraft.icon.trim();
    nodes = nodes.map((node) => {
      if (node.id !== nodeId) return node;

      const nextName = editDraft.name.trim();
      return {
        ...node,
        name: nextName || node.name,
        icon: normalizedIcon,
        description: editDraft.description.trim(),
        color: NODE_COLORS.includes(editDraft.color) ? editDraft.color : 'slate',
        links: [...new Set(editSelectedLinks.filter((id) => id !== nodeId))],
        targets: {
          path: normalizeOptionalString(editDraft.path),
          editor: normalizeOptionalString(editDraft.editor),
          browser: normalizeOptionalString(editDraft.browser),
          script: normalizeOptionalString(editDraft.script)
        }
      };
    });

    syncSmooth(true);
    scheduleSave();
    queueRender();
    closeEditor();
    updateStatus('Node updated');
  }

  function openCtxMenu(event, nodeId) {
    event.preventDefault();
    event.stopPropagation();

    closeEditor();

    const menuWidth = 220;
    const menuHeight = 280;
    const x = clamp(event.clientX, 8, Math.max(8, window.innerWidth - menuWidth - 8));
    const y = clamp(event.clientY, 8, Math.max(8, window.innerHeight - menuHeight - 8));

    contextMenu = { open: true, x, y, nodeId };
    highlightNodeId = nodeId;
  }

  function launchFromContext(action) {
    if (!contextNode) {
      closeCtx();
      return;
    }
    void launchNode(contextNode, action);
    closeCtx();
  }

  function openEditorFromMenu() {
    if (!contextNode || isLockedNode(contextNode)) {
      closeCtx();
      return;
    }
    openEditor(contextNode.id);
    closeCtx();
  }

  function addConnected() {
    const source = contextNode;
    if (!source) {
      closeCtx();
      return;
    }

    const next = createEmptyNode(source.x + 132, source.y + 24);
    nodes = [
      ...nodes.map((node) => {
        if (node.id !== source.id) return node;
        return {
          ...node,
          links: [...new Set([...(node.links ?? []), next.id])]
        };
      }),
      next
    ];

    syncSmooth(true);
    scheduleSave();
    void tick().then(() => {
      clampAllNodesToCanvas(true);
      queueRender();
    });
    closeCtx();
    updateStatus('Connected node added');
  }

  function connectNearest() {
    const source = contextNode;
    if (!source) {
      closeCtx();
      return;
    }

    const candidates = nodes.filter((node) => node.id !== source.id);
    if (!candidates.length) {
      closeCtx();
      return;
    }

    const nearest = candidates.reduce((best, node) => {
      const nodeDist = (node.x - source.x) ** 2 + (node.y - source.y) ** 2;
      const bestDist = (best.x - source.x) ** 2 + (best.y - source.y) ** 2;
      return nodeDist < bestDist ? node : best;
    });

    nodes = nodes.map((node) => {
      if (node.id !== source.id) return node;
      return {
        ...node,
        links: [...new Set([...(node.links ?? []), nearest.id])]
      };
    });

    scheduleSave();
    queueRender();
    closeCtx();
    updateStatus('Connected nearest node');
  }

  function clearLinks() {
    const source = contextNode;
    if (!source) {
      closeCtx();
      return;
    }

    nodes = nodes.map((node) => {
      if (node.id !== source.id) return node;
      return {
        ...node,
        links: []
      };
    });

    scheduleSave();
    queueRender();
    closeCtx();
    updateStatus('Links cleared');
  }

  function openLauncher() {
    showLauncher = true;
    launcherQuery = '';
    launcherIndex = 0;

    void tick().then(() => {
      const input = document.getElementById('launcher-input');
      if (input) input.focus();
    });
  }

  function closeLauncher() {
    showLauncher = false;
    launcherQuery = '';
  }

  function launcherKey(event) {
    if (event.key === 'Escape') {
      closeLauncher();
      return;
    }

    if (event.key === 'ArrowDown') {
      launcherIndex = Math.min(launcherIndex + 1, Math.max(0, launcherResults.length - 1));
      return;
    }

    if (event.key === 'ArrowUp') {
      launcherIndex = Math.max(launcherIndex - 1, 0);
      return;
    }

    if (event.key === 'Enter') {
      const selected = launcherResults[launcherIndex];
      if (selected) {
        void launchNode(selected, 'open-path');
        closeLauncher();
      }
    }
  }

  function hasLaunchTarget(node, action) {
    if (action === 'open-path') return Boolean(node?.targets?.path);
    if (action === 'open-browser') return Boolean(node?.targets?.browser);
    if (action === 'run-script') return Boolean(node?.targets?.script);
    if (action === 'open-editor') return Boolean(node?.targets?.editor || node?.targets?.path);
    return false;
  }

  function onNodeEnter(nodeId) {
    hoveredId = nodeId;
    highlightNodeId = nodeId;
  }

  function onNodeLeave(nodeId) {
    if (hoveredId === nodeId) hoveredId = null;
    if (!contextMenu.open && highlightNodeId === nodeId) {
      highlightNodeId = null;
    }
  }

  async function bootstrap() {
    const unlistenLayout = await listen('layout-updated', () => {
      void loadWorkspaces();
    });

    const unlistenLauncher = await listen('toggle-quick-launcher', () => {
      if (showLauncher) closeLauncher();
      else openLauncher();
    });

    await loadWorkspaces();

    const onResize = () => {
      queueRender();
      void tick().then(() => clampAllNodesToCanvas(false));
    };

    const onMove = (event) => onPointerMove(event);
    const onUp = () => onPointerUp();

    const onDown = (event) => {
      if (!(event.target instanceof Element)) return;
      if (!event.target.closest('.context-menu')) closeCtx();
      if (!event.target.closest('.editor-modal') && !event.target.closest('.node__edit')) {
        closeEditor();
      }
      if (!event.target.closest('.node') && !event.ctrlKey && !event.metaKey) {
        selectedIds = new Set();
        expandedNodeId = null;
      }
    };

    const onKey = (event) => {
      if (event.key === 'Escape') {
        closeCtx();
        closeEditor();
        closeLauncher();
        return;
      }

      if ((event.key.toLowerCase() === 'k' && (event.ctrlKey || event.metaKey)) || (event.code === 'Space' && event.altKey)) {
        event.preventDefault();
        if (showLauncher) closeLauncher();
        else openLauncher();
      }
    };

    window.addEventListener('resize', onResize);
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
    window.addEventListener('pointerdown', onDown);
    window.addEventListener('keydown', onKey);

    return () => {
      unlistenLayout();
      unlistenLauncher();

      window.removeEventListener('resize', onResize);
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
      window.removeEventListener('pointerdown', onDown);
      window.removeEventListener('keydown', onKey);

      if (saveTimer !== null) clearTimeout(saveTimer);
      if (dragFrame !== null) cancelAnimationFrame(dragFrame);
      if (springFrame !== null) cancelAnimationFrame(springFrame);
    };
  }

  $: smoothLookup = new Map(smoothNodes.map((item) => [item.id, item]));

  $: renderNodes = nodes.map((node) => {
    const smooth = smoothLookup.get(node.id);
    const useRaw = draggingId === node.id;
    return {
      ...node,
      renderX: useRaw ? node.x : smooth ? smooth.x : node.x,
      renderY: useRaw ? node.y : smooth ? smooth.y : node.y
    };
  });

  $: contextNode = contextMenu.nodeId ? nodes.find((node) => node.id === contextMenu.nodeId) ?? null : null;
  $: editNode = editPopup.nodeId ? nodes.find((node) => node.id === editPopup.nodeId) ?? null : null;

  $: launcherResults = launcherQuery.trim()
    ? nodes.filter((node) => node.name.toLowerCase().includes(launcherQuery.toLowerCase())).slice(0, 8)
    : nodes.slice(0, 8);
  $: isNodeBoardWindow = currentWindowLabel === 'desktop';

  $: {
    const validIds = new Set(nodes.map((node) => node.id));
    const next = [...selectedIds].filter((id) => validIds.has(id));
    if (next.length !== selectedIds.size) {
      selectedIds = new Set(next);
    }
    if (hoveredId && !validIds.has(hoveredId)) hoveredId = null;
    if (highlightNodeId && !validIds.has(highlightNodeId)) highlightNodeId = null;
    if (expandedNodeId && !validIds.has(expandedNodeId)) expandedNodeId = null;
  }

  onMount(() => {
    currentWindowLabel = appWindow.label ?? 'main';

    let disposed = false;
    let cleanup = () => {};

    void bootstrap()
      .then((fn) => {
        if (disposed) {
          fn();
          return;
        }
        cleanup = fn;
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
{:else}
  {#if isNodeBoardWindow}
    <main class="nodeboard-shell">
      <div class="canvas nodeboard-canvas" bind:this={nodeLayer}>
        <svg class="links" {viewBox}>
          {#each links as link}
            <path class="link" class:link--highlight={highlightNodeId && (link.from === highlightNodeId || link.to === highlightNodeId)} d={link.d}></path>
          {/each}
        </svg>

        <div class="node-layer">
          {#each renderNodes as node (node.id)}
            <div
              class="node"
              class:node--selected={selectedIds.has(node.id)}
              class:node--dragging={draggingId === node.id}
              class:node--expanded={expandedNodeId === node.id}
              role="button"
              tabindex="0"
              use:nodeRef={node.id}
              style="left:{node.renderX}px;top:{node.renderY}px;--node-color:{nodeColor(node)}"
              on:pointerdown={(event) => beginDrag(event, node.id)}
              on:click={(event) => onNodeClick(event, node.id)}
              on:keydown={(event) => onNodeKeydown(event, node.id)}
              on:contextmenu={(event) => openCtxMenu(event, node.id)}
              on:dblclick={() => void launchNode(node, 'open-path')}
              on:mouseenter={() => onNodeEnter(node.id)}
              on:mouseleave={() => onNodeLeave(node.id)}
            >
              {#if expandedNodeId === node.id}
                <div class="node__run-actions" role="group" aria-label={`Run options for ${node.name || 'node'}`}>
                  {#if hasLaunchTarget(node, 'open-path')}
                    <button on:pointerdown|stopPropagation on:click|stopPropagation={() => void launchNode(node, 'open-path')}>Path</button>
                  {/if}
                  {#if hasLaunchTarget(node, 'open-editor')}
                    <button on:pointerdown|stopPropagation on:click|stopPropagation={() => void launchNode(node, 'open-editor')}>Edit</button>
                  {/if}
                  {#if hasLaunchTarget(node, 'open-browser')}
                    <button on:pointerdown|stopPropagation on:click|stopPropagation={() => void launchNode(node, 'open-browser')}>Web</button>
                  {/if}
                  {#if hasLaunchTarget(node, 'run-script')}
                    <button on:pointerdown|stopPropagation on:click|stopPropagation={() => void launchNode(node, 'run-script')}>Run</button>
                  {/if}
                </div>
              {/if}

              <div class="node__top">
                <span class="node__dot" class:node__dot--active={Boolean(node.last_launched)}></span>
                <button class="node__edit" disabled={isLockedNode(node)} on:click|stopPropagation={() => openEditor(node.id)}>Edit</button>
              </div>

              {#if isImageIcon(node.icon)}
                <img class="node__icon-image" src={node.icon} alt={node.name || 'Node icon'} />
              {:else if isLogoIcon(node.icon)}
                <img class="node__logo" src={appLogo} alt="FinNode" />
              {:else}
                <div class="node__icon">{node.icon || 'N'}</div>
              {/if}

              <div class="node__name">{node.name || 'Untitled'}</div>
              {#if expandedNodeId === node.id}
                <div class="node__hint">{node.description || 'Node expanded'}</div>
              {/if}
            </div>
          {/each}
        </div>
      </div>
    </main>
  {:else}
    <main class="settings-shell">
      <aside class="sidebar settings-layer">
      <header class="brand panel">
        <div class="brand__identity">
          <img class="brand__logo" src={appLogo} alt="FinNode" />
          <div>
            <h1>FinNode</h1>
            <p>Settings and controls</p>
          </div>
        </div>
        <div class="settings-window-controls">
          <button on:click={minimizeToTray}>Minimize</button>
          <button class="danger" on:click={shutdownApp}>Shutdown</button>
        </div>
      </header>

      <section class="panel">
        <div class="panel__head">
          <h2>Workspaces</h2>
        </div>
        <select value={activeWorkspaceId} on:change={(event) => switchWorkspace(event.currentTarget.value)}>
          {#each workspaces as workspace}
            <option value={workspace.id}>{workspace.name}</option>
          {/each}
        </select>

        <div class="row">
          <input bind:value={workspaceName} placeholder="New workspace" />
          <button on:click={createWorkspace}>Create</button>
        </div>

        <button class="danger" disabled={workspaces.length <= 1} on:click={() => deleteWorkspace(activeWorkspaceId)}>
          Delete current workspace
        </button>
      </section>

      <section class="panel">
        <div class="panel__head">
          <h2>Nodes</h2>
          <span>{nodes.length}</span>
        </div>

        <div class="row row--tight">
          <button on:click={addNode}>Add node</button>
          <button on:click={layoutGrid}>Auto layout</button>
          <button on:click={openLauncher}>Launcher</button>
        </div>

        {#if selectedIds.size > 0}
          <div class="selection-bar">
            <span>{selectedIds.size} selected</span>
            <button on:click={batchLaunch}>Open</button>
            <button class="danger" on:click={batchDelete}>Delete</button>
            <button on:click={() => (selectedIds = new Set())}>Clear</button>
          </div>
        {/if}

        <div class="node-list">
          {#each nodes as node (node.id)}
            {@const locked = isLockedNode(node)}
            <div class="node-row" class:node-row--selected={selectedIds.has(node.id)}>
              <div class="node-row__title" title={node.name || 'Untitled node'}>{node.name || 'Untitled node'}</div>
              <div class="node-row__actions">
                <button disabled={!hasLaunchTarget(node, 'open-path')} on:click={() => void launchNode(node, 'open-path')}>Open</button>
                <button disabled={locked} on:click={() => openEditor(node.id)}>Edit</button>
                <button disabled={locked} on:click={() => cloneNode(node.id)}>Clone</button>
                <button class="danger" disabled={locked} on:click={() => deleteNode(node.id)}>Delete</button>
              </div>
            </div>
          {/each}
        </div>
      </section>

      <section class="panel status-panel">
        <div class="panel__head">
          <h2>Status</h2>
        </div>
        <p class="status-text">{statusText}</p>
        <div class="activity">
          {#if activityLog.length === 0}
            <p class="muted">No activity yet.</p>
          {:else}
            {#each activityLog as item (item.id)}
              <div class="activity__item">{item.text}</div>
            {/each}
          {/if}
        </div>
      </section>
      </aside>
    </main>
  {/if}

  {#if editPopup.open && editNode}
    <!-- svelte-ignore a11y-click-events-have-key-events -->
    <div class="modal" role="presentation" on:click|self={closeEditor}>
      <section class="editor-modal">
        <header>
          <h3>Edit Node</h3>
          <button class="ghost" on:click={closeEditor}>Close</button>
        </header>

        <label>
          <span>Name</span>
          <input bind:value={editDraft.name} />
        </label>

        <label>
          <span>Icon</span>
          <input bind:value={editDraft.icon} placeholder="N" />
        </label>

        <label>
          <span>Upload icon image</span>
          <input type="file" accept="image/*" on:change={handleNodeIconUpload} />
        </label>

        {#if isImageIcon(editDraft.icon)}
          <div class="icon-upload-preview">
            <img class="icon-upload-preview__img" src={editDraft.icon} alt="Icon preview" />
            <button type="button" class="ghost" on:click={clearUploadedNodeIcon}>Remove uploaded image</button>
          </div>
        {/if}

        <label>
          <span>Description</span>
          <textarea rows="2" bind:value={editDraft.description}></textarea>
        </label>

        <label>
          <span>Folder path</span>
          <input bind:value={editDraft.path} placeholder="/path/to/project" />
        </label>

        <label>
          <span>Editor path</span>
          <input bind:value={editDraft.editor} placeholder="/path/to/project" />
        </label>

        <label>
          <span>Browser URL</span>
          <input bind:value={editDraft.browser} placeholder="https://example.com" />
        </label>

        <label>
          <span>Script command</span>
          <input bind:value={editDraft.script} placeholder="npm run dev" />
        </label>

        <label>
          <span>Color</span>
          <select bind:value={editDraft.color}>
            {#each NODE_COLORS as color}
              <option value={color}>{color}</option>
            {/each}
          </select>
        </label>

        <section class="links-section">
          <h4>Links</h4>
          <div class="links-list">
            {#each nodes.filter((node) => node.id !== editNode.id) as candidate (candidate.id)}
              <label class="link-row">
                <span>{candidate.name || 'Untitled node'}</span>
                <input
                  type="checkbox"
                  checked={editSelectedLinks.includes(candidate.id)}
                  on:change={(event) => toggleEditLink(candidate.id, event.currentTarget.checked)}
                />
              </label>
            {/each}
          </div>
        </section>

        <footer>
          <button on:click={saveEditor}>Save</button>
          <button class="ghost" on:click={closeEditor}>Cancel</button>
        </footer>
      </section>
    </div>
  {/if}

  {#if contextMenu.open && contextNode}
    <div class="context-menu" style="left:{contextMenu.x}px;top:{contextMenu.y}px;" on:pointerdown|stopPropagation>
      <div class="context-menu__title">{contextNode.name || 'Untitled node'}</div>
      <button disabled={!hasLaunchTarget(contextNode, 'open-path')} on:click={() => launchFromContext('open-path')}>Open path</button>
      <button disabled={!hasLaunchTarget(contextNode, 'open-editor')} on:click={() => launchFromContext('open-editor')}>Open editor</button>
      <button disabled={!hasLaunchTarget(contextNode, 'open-browser')} on:click={() => launchFromContext('open-browser')}>Open browser</button>
      <button disabled={!hasLaunchTarget(contextNode, 'run-script')} on:click={() => launchFromContext('run-script')}>Run script</button>
      <button disabled={isLockedNode(contextNode)} on:click={openEditorFromMenu}>Edit</button>
      <button on:click={addConnected}>Add connected node</button>
      <button on:click={connectNearest}>Connect nearest</button>
      <button on:click={clearLinks}>Clear links</button>
      <button disabled={isLockedNode(contextNode)} on:click={() => { cloneNode(contextNode.id); closeCtx(); }}>Clone</button>
      <button class="danger" disabled={isLockedNode(contextNode)} on:click={() => { deleteNode(contextNode.id); closeCtx(); }}>Delete</button>
    </div>
  {/if}

  {#if showLauncher}
    <!-- svelte-ignore a11y-click-events-have-key-events -->
    <div class="launcher-overlay" role="presentation" on:click|self={closeLauncher}>
      <section class="launcher">
        <input
          id="launcher-input"
          class="launcher__input"
          placeholder="Search nodes"
          bind:value={launcherQuery}
          on:keydown={launcherKey}
        />
        <div class="launcher__list">
          {#each launcherResults as node, index (node.id)}
            <button
              class="launcher__item"
              class:launcher__item--active={index === launcherIndex}
              on:click={() => {
                void launchNode(node, 'open-path');
                closeLauncher();
              }}
            >
              <span>{node.name || 'Untitled node'}</span>
            </button>
          {/each}
          {#if launcherResults.length === 0}
            <div class="muted">No matching nodes</div>
          {/if}
        </div>
      </section>
    </div>
  {/if}
{/if}

<style>
  :global(:root) {
    --bg: #0e1722;
    --panel: #14212f;
    --panel-soft: #18283a;
    --canvas: #0f1b28;
    --text: #e7eef6;
    --muted: #9db0c2;
    --line: rgba(118, 180, 224, 0.4);
    --line-strong: rgba(77, 208, 225, 0.78);
    --accent: #4dd0e1;
    --border: rgba(138, 165, 189, 0.24);
    --danger: #f27f8f;
    --settings-width: 332px;
  }

  * {
    box-sizing: border-box;
  }

  :global(html),
  :global(body),
  :global(#app) {
    width: 100%;
    height: 100%;
    margin: 0;
  }

  :global(body) {
    font-family: 'Space Grotesk', 'Segoe UI', sans-serif;
    background: linear-gradient(165deg, #0c141f 0%, #102236 100%);
    color: var(--text);
  }

  .settings-shell {
    width: 100%;
    height: 100%;
    padding: 14px;
    display: flex;
    align-items: flex-start;
    overflow: auto;
  }

  .nodeboard-shell {
    width: 100%;
    height: 100%;
    display: grid;
    place-items: center;
    padding: 18px;
    background:
      radial-gradient(circle at 18% 18%, rgba(77, 208, 225, 0.1), transparent 30%),
      radial-gradient(circle at 82% 82%, rgba(123, 208, 137, 0.08), transparent 28%),
      linear-gradient(165deg, #0b1320 0%, #0d1e31 100%);
  }

  .nodeboard-canvas {
    border: 1px solid rgba(129, 153, 176, 0.26);
    border-radius: 20px;
    box-shadow: 0 24px 48px rgba(0, 0, 0, 0.35);
  }

  .sidebar {
    width: var(--settings-width);
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 12px;
    overflow: auto;
    padding-right: 2px;
    position: relative;
    z-index: 20;
  }

  .settings-layer {
    pointer-events: auto;
  }

  .panel {
    border: 1px solid var(--border);
    border-radius: 14px;
    background: linear-gradient(180deg, var(--panel) 0%, var(--panel-soft) 100%);
    padding: 12px;
  }

  .brand {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 10px;
  }

  .brand__identity {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .brand h1 {
    margin: 0;
    font-size: 1.15rem;
    letter-spacing: 0.02em;
  }

  .brand p {
    margin: 3px 0 0;
    color: var(--muted);
    font-size: 0.82rem;
  }

  .brand__logo {
    width: 36px;
    height: 36px;
    object-fit: contain;
    border-radius: 10px;
    background: rgba(255, 255, 255, 0.03);
    padding: 4px;
  }

  .settings-window-controls {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-left: auto;
  }

  .settings-window-controls button {
    width: auto;
    white-space: nowrap;
    padding: 7px 10px;
    font-size: 0.74rem;
  }

  .panel__head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 8px;
  }

  .panel__head h2 {
    margin: 0;
    font-size: 0.82rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--muted);
  }

  .panel select,
  .panel input,
  .panel button {
    width: 100%;
    font: inherit;
    color: var(--text);
    border-radius: 10px;
    border: 1px solid var(--border);
    background: rgba(11, 19, 28, 0.7);
    padding: 8px 10px;
  }

  .panel button {
    cursor: pointer;
  }

  .panel button:hover {
    border-color: rgba(77, 208, 225, 0.55);
  }

  .panel button:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .panel .danger {
    border-color: rgba(242, 127, 143, 0.45);
    color: #ffd5dc;
  }

  .row {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 8px;
    margin-top: 8px;
  }

  .row--tight {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }

  .selection-bar {
    margin-top: 10px;
    display: grid;
    grid-template-columns: 1fr repeat(3, auto);
    gap: 6px;
    align-items: center;
    background: rgba(8, 14, 22, 0.62);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 8px;
    font-size: 0.8rem;
  }

  .node-list {
    margin-top: 10px;
    display: flex;
    flex-direction: column;
    gap: 7px;
    max-height: 230px;
    overflow: auto;
  }

  .node-row {
    border: 1px solid rgba(126, 151, 175, 0.22);
    border-radius: 10px;
    background: rgba(12, 20, 30, 0.66);
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .node-row--selected {
    border-color: rgba(77, 208, 225, 0.65);
  }

  .node-row__title {
    font-size: 0.82rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .node-row__actions {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 5px;
  }

  .node-row__actions button {
    padding: 6px;
    font-size: 0.72rem;
  }

  .status-panel {
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .status-text {
    margin: 0;
    font-size: 0.86rem;
  }

  .activity {
    display: flex;
    flex-direction: column;
    gap: 6px;
    overflow: auto;
    max-height: 120px;
  }

  .activity__item {
    font-size: 0.74rem;
    color: var(--muted);
    border-bottom: 1px dashed rgba(138, 165, 189, 0.2);
    padding-bottom: 4px;
  }

  .muted {
    margin: 0;
    color: var(--muted);
    font-size: 0.78rem;
  }

  .canvas {
    position: relative;
    width: 100%;
    height: 100%;
    min-height: 0;
    border-radius: 12px;
    border: 1px solid rgba(129, 153, 176, 0.2);
    background: linear-gradient(180deg, rgba(10, 19, 28, 0.75) 0%, rgba(9, 16, 24, 0.95) 100%);
    overflow: hidden;
    z-index: 0;
  }

  .canvas.nodeboard-canvas {
    width: min(860px, calc(100vw - 48px));
    height: min(620px, calc(100vh - 48px));
    min-height: 380px;
    background:
      radial-gradient(circle at 20% 20%, rgba(77, 208, 225, 0.08), transparent 34%),
      linear-gradient(180deg, rgba(10, 19, 28, 0.82) 0%, rgba(8, 14, 22, 0.97) 100%);
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
  }

  .link--highlight {
    stroke: var(--line-strong);
    stroke-width: 2.6;
  }

  .node-layer {
    pointer-events: none;
  }

  .node {
    position: absolute;
    width: 104px;
    height: 104px;
    border-radius: 999px;
    border: 1px solid rgba(143, 163, 181, 0.42);
    background:
      radial-gradient(circle at 26% 24%, rgba(255, 255, 255, 0.18), rgba(255, 255, 255, 0.02) 48%),
      linear-gradient(180deg, rgba(11, 20, 30, 0.95), rgba(7, 13, 20, 0.92));
    color: var(--text);
    padding: 11px 10px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    pointer-events: auto;
    user-select: none;
    cursor: grab;
    box-shadow: 0 10px 24px rgba(0, 0, 0, 0.3), inset 0 0 0 1px rgba(255, 255, 255, 0.03);
    transform-origin: center;
    transition: border-color 120ms ease, transform 180ms ease, box-shadow 180ms ease;
  }

  .node--selected {
    border-color: rgba(77, 208, 225, 0.78);
  }

  .node--dragging {
    cursor: grabbing;
    transform: scale(1.02);
  }

  .node--expanded {
    transform: scale(1.24);
    z-index: 8;
    box-shadow: 0 16px 30px rgba(0, 0, 0, 0.38), 0 0 0 1px rgba(77, 208, 225, 0.45);
  }

  .node--expanded.node--dragging {
    transform: scale(1.18);
  }

  .node__run-actions {
    position: absolute;
    top: -36px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 6px;
    border-radius: 999px;
    border: 1px solid rgba(133, 157, 180, 0.35);
    background: rgba(9, 16, 24, 0.92);
    box-shadow: 0 8px 16px rgba(0, 0, 0, 0.28);
    z-index: 12;
  }

  .node__run-actions button {
    width: auto;
    min-width: 0;
    padding: 3px 7px;
    border-radius: 999px;
    border: 1px solid rgba(133, 157, 180, 0.35);
    background: rgba(12, 22, 34, 0.9);
    color: var(--text);
    font-size: 0.6rem;
    line-height: 1;
    cursor: pointer;
    white-space: nowrap;
  }

  .node__run-actions button:hover {
    border-color: rgba(77, 208, 225, 0.62);
    color: #d7f9ff;
  }

  .node__top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    gap: 6px;
  }

  .node__dot {
    width: 8px;
    height: 8px;
    border-radius: 999px;
    background: rgba(157, 176, 194, 0.6);
  }

  .node__dot--active {
    background: #79d089;
  }

  .node__edit {
    width: auto;
    padding: 3px 6px;
    font-size: 0.66rem;
    border-radius: 7px;
  }

  .node__icon,
  .node__logo {
    width: 36px;
    height: 36px;
    border-radius: 999px;
    display: grid;
    place-items: center;
    margin: 0 auto;
    color: var(--node-color);
    font-weight: 700;
    border: 1px solid rgba(143, 163, 181, 0.28);
    background: rgba(7, 14, 22, 0.72);
  }

  .node__logo {
    object-fit: contain;
    padding: 5px;
  }

  .node__icon-image {
    width: 40px;
    height: 40px;
    border-radius: 999px;
    object-fit: cover;
    border: 1px solid rgba(143, 163, 181, 0.3);
    background: rgba(7, 14, 22, 0.72);
  }

  .node__name {
    margin-top: auto;
    font-size: 0.74rem;
    text-align: center;
    max-width: 90px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .node__hint {
    width: 94px;
    margin-top: 2px;
    font-size: 0.58rem;
    line-height: 1.2;
    color: var(--muted);
    text-align: center;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .modal,
  .launcher-overlay {
    position: fixed;
    inset: 0;
    background: rgba(4, 8, 14, 0.65);
    display: grid;
    place-items: center;
    z-index: 30;
    padding: 20px;
  }

  .editor-modal,
  .launcher {
    width: min(460px, 94vw);
    max-height: 88vh;
    overflow: auto;
    border-radius: 14px;
    border: 1px solid rgba(140, 165, 188, 0.35);
    background: linear-gradient(180deg, #122130 0%, #101b2a 100%);
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .editor-modal header,
  .editor-modal footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .editor-modal h3 {
    margin: 0;
    font-size: 1rem;
  }

  .editor-modal label {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .editor-modal input[type='file'] {
    padding: 6px;
  }

  .icon-upload-preview {
    border: 1px dashed rgba(133, 157, 180, 0.35);
    border-radius: 10px;
    padding: 8px;
    display: flex;
    align-items: center;
    gap: 10px;
    background: rgba(9, 15, 23, 0.52);
  }

  .icon-upload-preview__img {
    width: 46px;
    height: 46px;
    border-radius: 10px;
    object-fit: cover;
    border: 1px solid rgba(133, 157, 180, 0.28);
  }

  .icon-upload-preview button {
    width: auto;
  }

  .editor-modal label span,
  .links-section h4 {
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: var(--muted);
  }

  .links-section {
    border-top: 1px solid rgba(133, 157, 180, 0.2);
    padding-top: 8px;
  }

  .links-section h4 {
    margin: 0 0 8px;
  }

  .links-list {
    display: flex;
    flex-direction: column;
    gap: 5px;
    max-height: 150px;
    overflow: auto;
  }

  .link-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font-size: 0.8rem;
    border: 1px solid rgba(129, 153, 176, 0.2);
    border-radius: 8px;
    padding: 6px 8px;
    background: rgba(9, 15, 23, 0.58);
  }

  .ghost {
    background: rgba(9, 15, 23, 0.58);
  }

  .context-menu {
    position: fixed;
    z-index: 40;
    width: 220px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    border: 1px solid rgba(133, 157, 180, 0.35);
    border-radius: 12px;
    padding: 8px;
    background: linear-gradient(180deg, #122130 0%, #101b2a 100%);
  }

  .context-menu__title {
    font-size: 0.8rem;
    color: var(--muted);
    border-bottom: 1px solid rgba(133, 157, 180, 0.2);
    padding-bottom: 7px;
    margin-bottom: 4px;
  }

  .context-menu button {
    text-align: left;
    font-size: 0.78rem;
    padding: 7px 8px;
    border-radius: 8px;
    border: 1px solid rgba(133, 157, 180, 0.2);
    background: rgba(8, 14, 22, 0.7);
    color: var(--text);
    cursor: pointer;
  }

  .context-menu button:hover {
    border-color: rgba(77, 208, 225, 0.58);
  }

  .context-menu button:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .context-menu .danger {
    border-color: rgba(242, 127, 143, 0.45);
    color: #ffd5dc;
  }

  .launcher__input {
    width: 100%;
    border: 1px solid rgba(133, 157, 180, 0.3);
    border-radius: 10px;
    padding: 10px 12px;
    font: inherit;
    color: var(--text);
    background: rgba(8, 14, 22, 0.8);
  }

  .launcher__list {
    margin-top: 8px;
    display: flex;
    flex-direction: column;
    gap: 5px;
    max-height: 280px;
    overflow: auto;
  }

  .launcher__item {
    width: 100%;
    text-align: left;
    border: 1px solid rgba(133, 157, 180, 0.2);
    border-radius: 9px;
    background: rgba(8, 14, 22, 0.64);
    color: var(--text);
    padding: 9px 10px;
    cursor: pointer;
  }

  .launcher__item--active,
  .launcher__item:hover {
    border-color: rgba(77, 208, 225, 0.62);
  }

  .fatal {
    margin: 0;
    padding: 18px;
    color: #ffd0d8;
    white-space: pre-wrap;
  }

  @media (max-width: 980px) {
    .settings-shell {
      padding: 10px;
    }

    .settings-layer.sidebar {
      width: 100%;
      max-height: 100%;
      padding-right: 0;
    }

    .nodeboard-shell {
      padding: 10px;
    }

    .canvas.nodeboard-canvas {
      width: calc(100vw - 20px);
      height: calc(100vh - 20px);
      min-height: 340px;
    }

    .settings-window-controls {
      flex-wrap: wrap;
    }

    .row--tight {
      grid-template-columns: 1fr;
    }

    .selection-bar {
      grid-template-columns: 1fr 1fr;
    }
  }
</style>
