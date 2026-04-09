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
    cyan: '#5ee7f7',
    green: '#6ee89a',
    amber: '#fdd87a',
    rose: '#ff8fa8',
    violet: '#c4a8ff'
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
  let nodeLayerResizeObserver = null;
  let nodeLayerResizeFrame = null;
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

  let zoomLevel = 1.0;
  const ZOOM_MIN = 0.4;
  const ZOOM_MAX = 2.5;
  const ZOOM_STEP = 0.15;

  function zoomIn()  { zoomLevel = Math.min(ZOOM_MAX, parseFloat((zoomLevel + ZOOM_STEP).toFixed(2))); }
  function zoomOut() { zoomLevel = Math.max(ZOOM_MIN, parseFloat((zoomLevel - ZOOM_STEP).toFixed(2))); }

  async function minimizeWindow() { try { await appWindow.minimize(); } catch(e) {} }
  async function closeWindow()    { try { await appWindow.close();    } catch(e) {} }

  function createEditDraft() {
    return { name: '', icon: '', description: '', path: '', editor: '', browser: '', script: '', color: 'slate' };
  }

  function uid(prefix) { return `${prefix}-${Math.random().toString(36).slice(2, 8)}`; }
  function clamp(value, min, max) { return Math.max(min, Math.min(value, max)); }

  function normalizeOptionalString(value) {
    if (typeof value !== 'string') return null;
    const trimmed = value.trim();
    return trimmed ? trimmed : null;
  }

  function isLockedNode(nodeOrId) {
    const id = typeof nodeOrId === 'string' ? nodeOrId : nodeOrId?.id;
    return id === MAIN_NODE_ID || Boolean(nodeOrId?.locked);
  }

  function isLogoIcon(icon) { return icon === LOGO_ICON || icon === 'fin'; }
  function isImageIcon(icon) { return typeof icon === 'string' && icon.startsWith('data:image/'); }
  function nodeColor(node) { return NODE_COLOR_MAP[node?.color] || NODE_COLOR_MAP.slate; }

  function createMainNode(anchor) {
    let x = 30, y = 30;
    if (anchor) { x = Math.max(20, Number(anchor.x) - 130); y = Math.max(20, Number(anchor.y) - 30); }
    return { id: MAIN_NODE_ID, name: MAIN_NODE_NAME, icon: LOGO_ICON, description: 'Core entry node', x, y, links: [], targets: { path: null, editor: null, browser: null, script: null }, color: 'cyan', locked: true, last_launched: null };
  }

  function createEmptyNode(x, y) {
    return { id: uid('node'), name: '', icon: '', description: '', x, y, links: [], targets: { path: null, editor: null, browser: null, script: null }, color: 'slate', locked: false, last_launched: null };
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
      targets: { path: normalizeOptionalString(targets.path), editor: normalizeOptionalString(targets.editor), browser: normalizeOptionalString(targets.browser), script: normalizeOptionalString(targets.script) },
      color: NODE_COLORS.includes(raw?.color) ? raw.color : 'slate',
      locked: Boolean(raw?.locked),
      last_launched: typeof raw?.last_launched === 'string' ? raw.last_launched : null
    };
  }

  function ensureMainNode(list) {
    let changed = false, hasMain = false;
    const normalized = list.map((node) => {
      if (node.id !== MAIN_NODE_ID) return node;
      hasMain = true;
      const forced = { ...node, name: MAIN_NODE_NAME, icon: LOGO_ICON, locked: true };
      if (forced.name !== node.name || forced.icon !== node.icon || !node.locked) changed = true;
      return forced;
    });
    if (hasMain) return { nodes: normalized, changed };
    const anchor = normalized[0];
    return { nodes: [createMainNode(anchor), ...normalized], changed: true };
  }

  function createDefaultWorkspace() {
    return { id: 'default', name: 'Default', nodes: [createMainNode(null)], zoom: 1, pan_x: 0, pan_y: 0 };
  }

  function normalizeWorkspace(raw, index) {
    const id = typeof raw?.id === 'string' && raw.id ? raw.id : `ws-${index + 1}`;
    const name = typeof raw?.name === 'string' && raw.name.trim() ? raw.name.trim() : `Workspace ${index + 1}`;
    const nodeList = Array.isArray(raw?.nodes) ? raw.nodes.map((node, i) => normalizeNode(node, i)) : [];
    const ensured = ensureMainNode(nodeList);
    return { id, name, nodes: ensured.nodes, zoom: 1, pan_x: 0, pan_y: 0 };
  }

  function normalizeWorkspaces(rawList) {
    if (!Array.isArray(rawList) || rawList.length === 0) return [createDefaultWorkspace()];
    const ids = new Set();
    const normalized = rawList.map((raw, index) => {
      const ws = normalizeWorkspace(raw, index);
      if (ids.has(ws.id)) ws.id = uid('ws');
      ids.add(ws.id);
      return ws;
    });
    return normalized.length ? normalized : [createDefaultWorkspace()];
  }

  function recordActivity(message) {
    const stamp = new Date().toLocaleTimeString();
    activityLog = [{ id: `${Date.now()}-${Math.random().toString(36).slice(2, 7)}`, text: `${stamp} ${message}` }, ...activityLog].slice(0, 8);
  }

  function updateStatus(message) { statusText = message; recordActivity(message); }

  function updateActiveWorkspaceNodes(nextNodes) {
    workspaces = workspaces.map((workspace) => {
      if (workspace.id !== activeWorkspaceId) return workspace;
      return { ...workspace, nodes: nextNodes, zoom: 1, pan_x: 0, pan_y: 0 };
    });
  }

  async function persistLayout() {
    updateActiveWorkspaceNodes(nodes);
    await invoke('save_layout', { layout: { active_workspace: activeWorkspaceId, workspaces, command_history: commandHistoryCache } });
  }

  async function flushPendingSave() {
    if (saveTimer === null) return;
    clearTimeout(saveTimer);
    saveTimer = null;
    await persistLayout();
  }

  function scheduleSave() {
    if (saveTimer !== null) clearTimeout(saveTimer);
    saveTimer = setTimeout(async () => {
      saveTimer = null;
      try { await persistLayout(); } catch (error) { updateStatus(String(error)); }
    }, SAVE_DEBOUNCE_MS);
  }

  function applyLayout(layout) {
    commandHistoryCache = Array.isArray(layout?.command_history) ? layout.command_history : [];
    workspaces = normalizeWorkspaces(layout?.workspaces ?? []);
    let nextActive = typeof layout?.active_workspace === 'string' ? layout.active_workspace : workspaces[0].id;
    if (!workspaces.some((workspace) => workspace.id === nextActive)) nextActive = workspaces[0].id;
    activeWorkspaceId = nextActive;
    const workspace = workspaces.find((item) => item.id === activeWorkspaceId) ?? workspaces[0];
    nodes = workspace.nodes;
    syncSmooth(true);
    void tick().then(() => { clampAllNodesToCanvas(false); queueRender(); });
  }

  async function loadWorkspaces() {
    try { const layout = await invoke('load_layout'); applyLayout(layout); updateStatus(`Loaded ${nodes.length} nodes`); }
    catch (error) { updateStatus(String(error)); }
  }

  async function switchWorkspace(workspaceId) {
    if (!workspaceId || workspaceId === activeWorkspaceId) return;
    try {
      await flushPendingSave();
      const layout = await invoke('switch_workspace', { workspaceId });
      applyLayout(layout);
      const workspace = workspaces.find((item) => item.id === activeWorkspaceId);
      updateStatus(`Switched to ${workspace?.name ?? 'workspace'}`);
    } catch (error) { updateStatus(String(error)); }
  }

  async function createWorkspace() {
    const name = workspaceName.trim();
    if (!name) return;
    try {
      await flushPendingSave();
      const created = await invoke('create_workspace', { name });
      workspaceName = '';
      await loadWorkspaces();
      if (created && typeof created.id === 'string') await switchWorkspace(created.id);
      updateStatus('Workspace created');
    } catch (error) { updateStatus(String(error)); }
  }

  async function deleteWorkspace(workspaceId) {
    if (!workspaceId || workspaces.length <= 1) return;
    try {
      await flushPendingSave();
      await invoke('delete_workspace', { workspaceId });
      await loadWorkspaces();
      updateStatus('Workspace deleted');
    } catch (error) { updateStatus(String(error)); }
  }

  async function launchNode(node, action) {
    try {
      await invoke('launch_node', { node, action });
      nodes = nodes.map((item) => { if (item.id !== node.id) return item; return { ...item, last_launched: new Date().toISOString() }; });
      queueRender();
      updateStatus(`Ran ${action} on ${node.name || 'node'}`);
    } catch (error) { updateStatus(String(error)); }
  }

  async function openSettingsFromMainNode() {
    try { await invoke('show_settings_view'); updateStatus('Opened settings'); }
    catch (error) { updateStatus(`Unable to open settings: ${String(error)}`); }
  }

  function addNode() {
    const offset = nodes.length * 14;
    const next = createEmptyNode(50 + offset, 70 + offset);
    nodes = [...nodes, next];
    syncSmooth(true);
    scheduleSave();
    void tick().then(() => { clampAllNodesToCanvas(true); queueRender(); });
    updateStatus('Added node');
  }

  function cloneNode(nodeId) {
    if (isLockedNode(nodeId)) return;
    const source = nodes.find((node) => node.id === nodeId);
    if (!source) return;
    const clone = { ...source, id: uid('node'), name: source.name ? `${source.name} copy` : 'Node copy', x: source.x + 24, y: source.y + 24, links: [...(source.links ?? [])], targets: { ...(source.targets ?? {}) }, locked: false };
    nodes = [...nodes, clone];
    syncSmooth(true);
    scheduleSave();
    void tick().then(() => { clampAllNodesToCanvas(true); queueRender(); });
    updateStatus('Node cloned');
  }

  function deleteNode(nodeId) {
    if (isLockedNode(nodeId)) { updateStatus('Main node is locked'); return; }
    if (contextMenu.nodeId === nodeId) closeCtx();
    if (editPopup.nodeId === nodeId) closeEditor();
    nodes = nodes.filter((node) => node.id !== nodeId).map((node) => ({ ...node, links: (node.links ?? []).filter((id) => id !== nodeId) }));
    syncSmooth(true);
    scheduleSave();
    updateStatus('Node deleted');
  }

  function layoutGrid() {
    const cols = Math.max(1, Math.ceil(Math.sqrt(nodes.length)));
    const gapX = 140, gapY = 128;
    nodes = nodes.map((node, index) => ({ ...node, x: 36 + (index % cols) * gapX, y: 36 + Math.floor(index / cols) * gapY }));
    syncSmooth(true);
    scheduleSave();
    void tick().then(() => { clampAllNodesToCanvas(true); queueRender(); });
    updateStatus('Auto layout applied');
  }

  function toggleSelect(nodeId, event) {
    if (!event.ctrlKey && !event.metaKey) return false;
    const next = new Set(selectedIds);
    if (next.has(nodeId)) next.delete(nodeId); else next.add(nodeId);
    selectedIds = next;
    return true;
  }

  function batchLaunch() {
    for (const id of selectedIds) { const node = nodes.find((item) => item.id === id); if (node) void launchNode(node, 'open-path'); }
    selectedIds = new Set();
  }

  function batchDelete() {
    const toDelete = [...selectedIds];
    for (const id of toDelete) deleteNode(id);
    selectedIds = new Set();
  }

  function clampNodePosition(node, x, y) {
    if (!nodeLayer) return { x, y };
    const layerRect = nodeLayer.getBoundingClientRect();
    const element = nodeElements.get(node.id);
    const width = element ? element.offsetWidth : NODE_SIZE;
    const height = element ? element.offsetHeight : NODE_SIZE;
    return { x: clamp(x, 0, Math.max(0, layerRect.width - width)), y: clamp(y, 0, Math.max(0, layerRect.height - height)) };
  }

  function clampAllNodesToCanvas(save = false) {
    if (!nodeLayer || nodes.length === 0) return;
    let changed = false;
    const next = nodes.map((node) => {
      const clamped = clampNodePosition(node, node.x, node.y);
      if (clamped.x === node.x && clamped.y === node.y) return node;
      changed = true;
      return { ...node, x: clamped.x, y: clamped.y };
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
    dragOffset = { x: event.clientX - rect.left, y: event.clientY - rect.top };
    event.preventDefault();
  }

  function onPointerMove(event) {
    if (!draggingId || !nodeLayer) return;
    if (!dragMoved) dragMoved = Math.abs(event.clientX - dragStart.x) > 4 || Math.abs(event.clientY - dragStart.y) > 4;
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
    if (dragFrame !== null) { cancelAnimationFrame(dragFrame); dragFrame = null; }
    pendingPointer = null;
    const moved = dragMoved;
    draggingId = null;
    dragMoved = false;
    if (moved) {
      syncSmooth(true);
      scheduleSave();
      suppressExpandNodeId = releasedId;
      setTimeout(() => { if (suppressExpandNodeId === releasedId) suppressExpandNodeId = null; }, 0);
    }
  }

  function onNodeClick(event, nodeId) {
    if (event.button !== 0) return;
    if (suppressExpandNodeId === nodeId) { suppressExpandNodeId = null; return; }
    if (nodeId === MAIN_NODE_ID) { expandedNodeId = null; void openSettingsFromMainNode(); return; }
    expandedNodeId = expandedNodeId === nodeId ? null : nodeId;
  }

  function onNodeKeydown(event, nodeId) {
    if (event.key !== 'Enter' && event.key !== ' ') return;
    event.preventDefault();
    if (nodeId === MAIN_NODE_ID) { expandedNodeId = null; void openSettingsFromMainNode(); return; }
    expandedNodeId = expandedNodeId === nodeId ? null : nodeId;
  }

  function nodeRef(element, id) {
    nodeElements.set(id, element);
    queueRender();
    return { destroy() { nodeElements.delete(id); queueRender(); } };
  }

  function queueRender() {
    if (renderQueued) return;
    renderQueued = true;
    void tick().then(() => { renderQueued = false; renderConnections(); });
  }

  function centerOf(element) {
    const rect = element.getBoundingClientRect();
    const layerRect = nodeLayer.getBoundingClientRect();
    return { x: rect.left - layerRect.left + rect.width / 2, y: rect.top - layerRect.top + rect.height / 2, radius: Math.max(20, Math.min(rect.width, rect.height) / 2 - 6) };
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
        const ux = dx / distance, uy = dy / distance;
        const start = { x: startCenter.x + ux * startCenter.radius, y: startCenter.y + uy * startCenter.radius };
        const end = { x: endCenter.x - ux * endCenter.radius, y: endCenter.y - uy * endCenter.radius };
        const bend = Math.min(120, distance * 0.34);
        const c1 = { x: start.x + ux * bend, y: start.y + uy * bend };
        const c2 = { x: end.x - ux * bend, y: end.y - uy * bend };
        next.push({ from: node.id, to: targetId, d: `M ${start.x} ${start.y} C ${c1.x} ${c1.y}, ${c2.x} ${c2.y}, ${end.x} ${end.y}` });
      }
    }
    links = next;
  }

  function syncSmooth(immediate = false) {
    const previous = new Map(smoothNodes.map((item) => [item.id, item]));
    smoothNodes = nodes.map((node) => {
      const existing = previous.get(node.id);
      if (!existing || immediate) return { id: node.id, x: node.x, y: node.y, vx: 0, vy: 0 };
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
      if (draggingId === item.id) return { ...item, x: target.x, y: target.y, vx: 0, vy: 0 };
      const dx = target.x - item.x, dy = target.y - item.y;
      const vx = (item.vx + dx * SPRING_STIFFNESS) * SPRING_DAMPING;
      const vy = (item.vy + dy * SPRING_STIFFNESS) * SPRING_DAMPING;
      if (Math.abs(dx) > 0.15 || Math.abs(dy) > 0.15 || Math.abs(vx) > 0.15 || Math.abs(vy) > 0.15) active = true;
      return { ...item, x: item.x + vx, y: item.y + vy, vx, vy };
    });
    queueRender();
    if (active) springFrame = requestAnimationFrame(stepSpring);
  }

  function closeCtx() { if (!contextMenu.open) return; contextMenu = { open: false, x: 0, y: 0, nodeId: null }; highlightNodeId = hoveredId; }
  function closeEditor() { if (!editPopup.open) return; editPopup = { open: false, nodeId: null }; editDraft = createEditDraft(); editSelectedLinks = []; }

  function openEditor(nodeId) {
    if (isLockedNode(nodeId)) return;
    const node = nodes.find((item) => item.id === nodeId);
    if (!node) return;
    editDraft = { name: node.name ?? '', icon: node.icon ?? '', description: node.description ?? '', path: node.targets?.path ?? '', editor: node.targets?.editor ?? '', browser: node.targets?.browser ?? '', script: node.targets?.script ?? '', color: node.color ?? 'slate' };
    editSelectedLinks = [...(node.links ?? [])];
    editPopup = { open: true, nodeId };
  }

  function clearUploadedNodeIcon() { editDraft = { ...editDraft, icon: '' }; }

  function handleNodeIconUpload(event) {
    const input = event.currentTarget;
    const file = input.files?.[0];
    if (!file) return;
    if (!file.type.startsWith('image/')) { updateStatus('Please choose an image file'); input.value = ''; return; }
    if (file.size > MAX_NODE_ICON_BYTES) { updateStatus('Icon image must be 512KB or smaller'); input.value = ''; return; }
    const reader = new FileReader();
    reader.onload = () => { if (typeof reader.result !== 'string') return; editDraft = { ...editDraft, icon: reader.result }; updateStatus('Node image uploaded'); };
    reader.readAsDataURL(file);
    input.value = '';
  }

  function toggleEditLink(targetId, enabled) {
    if (enabled) editSelectedLinks = [...new Set([...editSelectedLinks, targetId])];
    else editSelectedLinks = editSelectedLinks.filter((id) => id !== targetId);
  }

  function saveEditor() {
    if (!editPopup.nodeId || isLockedNode(editPopup.nodeId)) return;
    const nodeId = editPopup.nodeId;
    const normalizedIcon = isImageIcon(editDraft.icon) ? editDraft.icon : editDraft.icon.trim();
    nodes = nodes.map((node) => {
      if (node.id !== nodeId) return node;
      const nextName = editDraft.name.trim();
      return { ...node, name: nextName || node.name, icon: normalizedIcon, description: editDraft.description.trim(), color: NODE_COLORS.includes(editDraft.color) ? editDraft.color : 'slate', links: [...new Set(editSelectedLinks.filter((id) => id !== nodeId))], targets: { path: normalizeOptionalString(editDraft.path), editor: normalizeOptionalString(editDraft.editor), browser: normalizeOptionalString(editDraft.browser), script: normalizeOptionalString(editDraft.script) } };
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
    const menuWidth = 220, menuHeight = 280;
    const x = clamp(event.clientX, 8, Math.max(8, window.innerWidth - menuWidth - 8));
    const y = clamp(event.clientY, 8, Math.max(8, window.innerHeight - menuHeight - 8));
    contextMenu = { open: true, x, y, nodeId };
    highlightNodeId = nodeId;
  }

  function launchFromContext(action) { if (!contextNode) { closeCtx(); return; } void launchNode(contextNode, action); closeCtx(); }

  function openEditorFromMenu() {
    if (!contextNode || isLockedNode(contextNode)) { closeCtx(); return; }
    openEditor(contextNode.id);
    closeCtx();
  }

  function addConnected() {
    const source = contextNode;
    if (!source) { closeCtx(); return; }
    const next = createEmptyNode(source.x + 132, source.y + 24);
    nodes = [...nodes.map((node) => { if (node.id !== source.id) return node; return { ...node, links: [...new Set([...(node.links ?? []), next.id])] }; }), next];
    syncSmooth(true);
    scheduleSave();
    void tick().then(() => { clampAllNodesToCanvas(true); queueRender(); });
    closeCtx();
    updateStatus('Connected node added');
  }

  function connectNearest() {
    const source = contextNode;
    if (!source) { closeCtx(); return; }
    const candidates = nodes.filter((node) => node.id !== source.id);
    if (!candidates.length) { closeCtx(); return; }
    const nearest = candidates.reduce((best, node) => { const nodeDist = (node.x - source.x) ** 2 + (node.y - source.y) ** 2; const bestDist = (best.x - source.x) ** 2 + (best.y - source.y) ** 2; return nodeDist < bestDist ? node : best; });
    nodes = nodes.map((node) => { if (node.id !== source.id) return node; return { ...node, links: [...new Set([...(node.links ?? []), nearest.id])] }; });
    scheduleSave();
    queueRender();
    closeCtx();
    updateStatus('Connected nearest node');
  }

  function clearLinks() {
    const source = contextNode;
    if (!source) { closeCtx(); return; }
    nodes = nodes.map((node) => { if (node.id !== source.id) return node; return { ...node, links: [] }; });
    scheduleSave();
    queueRender();
    closeCtx();
    updateStatus('Links cleared');
  }

  function openLauncher() {
    showLauncher = true;
    launcherQuery = '';
    launcherIndex = 0;
    void tick().then(() => { const input = document.getElementById('launcher-input'); if (input) input.focus(); });
  }

  function closeLauncher() { showLauncher = false; launcherQuery = ''; }

  function launcherKey(event) {
    if (event.key === 'Escape') { closeLauncher(); return; }
    if (event.key === 'ArrowDown') { launcherIndex = Math.min(launcherIndex + 1, Math.max(0, launcherResults.length - 1)); return; }
    if (event.key === 'ArrowUp') { launcherIndex = Math.max(launcherIndex - 1, 0); return; }
    if (event.key === 'Enter') { const selected = launcherResults[launcherIndex]; if (selected) { void launchNode(selected, 'open-path'); closeLauncher(); } }
  }

  function hasLaunchTarget(node, action) {
    if (action === 'open-path') return Boolean(node?.targets?.path);
    if (action === 'open-browser') return Boolean(node?.targets?.browser);
    if (action === 'run-script') return Boolean(node?.targets?.script);
    if (action === 'open-editor') return Boolean(node?.targets?.editor || node?.targets?.path);
    return false;
  }

  function onNodeEnter(nodeId) { hoveredId = nodeId; highlightNodeId = nodeId; }
  function onNodeLeave(nodeId) { if (hoveredId === nodeId) hoveredId = null; if (!contextMenu.open && highlightNodeId === nodeId) highlightNodeId = null; }

  function scheduleNodeLayerRelayout() {
    if (nodeLayerResizeFrame !== null) return;
    nodeLayerResizeFrame = requestAnimationFrame(() => { nodeLayerResizeFrame = null; queueRender(); clampAllNodesToCanvas(false); });
  }

  function setupNodeLayerResizeObserver() {
    if (typeof ResizeObserver === 'undefined' || !nodeLayer) return;
    if (nodeLayerResizeObserver) nodeLayerResizeObserver.disconnect();
    nodeLayerResizeObserver = new ResizeObserver(() => { scheduleNodeLayerRelayout(); });
    nodeLayerResizeObserver.observe(nodeLayer);
  }

  async function bootstrap() {
    const unlistenLayout = await listen('layout-updated', () => { void loadWorkspaces(); });
    const unlistenLauncher = await listen('toggle-quick-launcher', () => { if (showLauncher) closeLauncher(); else openLauncher(); });
    await loadWorkspaces();
    await tick();
    setupNodeLayerResizeObserver();
    const onResize = () => { scheduleNodeLayerRelayout(); };
    const onMove = (event) => onPointerMove(event);
    const onUp = () => onPointerUp();
    const onDown = (event) => {
      if (!(event.target instanceof Element)) return;
      if (!event.target.closest('.context-menu')) closeCtx();
      if (!event.target.closest('.editor-modal') && !event.target.closest('.node__edit')) closeEditor();
      if (!event.target.closest('.node') && !event.ctrlKey && !event.metaKey) { selectedIds = new Set(); expandedNodeId = null; }
    };
    const onKey = (event) => {
      if (event.key === 'Escape') { closeCtx(); closeEditor(); closeLauncher(); return; }
      if ((event.key.toLowerCase() === 'k' && (event.ctrlKey || event.metaKey)) || (event.code === 'Space' && event.altKey)) { event.preventDefault(); if (showLauncher) closeLauncher(); else openLauncher(); }
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
      if (nodeLayerResizeObserver) nodeLayerResizeObserver.disconnect();
      if (nodeLayerResizeFrame !== null) cancelAnimationFrame(nodeLayerResizeFrame);
    };
  }

  $: smoothLookup = new Map(smoothNodes.map((item) => [item.id, item]));
  $: renderNodes = nodes.map((node) => { const smooth = smoothLookup.get(node.id); const useRaw = draggingId === node.id; return { ...node, renderX: useRaw ? node.x : smooth ? smooth.x : node.x, renderY: useRaw ? node.y : smooth ? smooth.y : node.y }; });
  $: contextNode = contextMenu.nodeId ? nodes.find((node) => node.id === contextMenu.nodeId) ?? null : null;
  $: editNode = editPopup.nodeId ? nodes.find((node) => node.id === editPopup.nodeId) ?? null : null;
  $: launcherResults = launcherQuery.trim() ? nodes.filter((node) => node.name.toLowerCase().includes(launcherQuery.toLowerCase())).slice(0, 8) : nodes.slice(0, 8);
  $: isNodeBoardWindow = currentWindowLabel === 'desktop';
  $: if (typeof document !== 'undefined') { document.documentElement.classList.toggle('desktop-overlay-window', isNodeBoardWindow); document.body.classList.toggle('desktop-mode', isNodeBoardWindow); }
  $: { const validIds = new Set(nodes.map((node) => node.id)); const next = [...selectedIds].filter((id) => validIds.has(id)); if (next.length !== selectedIds.size) selectedIds = new Set(next); if (hoveredId && !validIds.has(hoveredId)) hoveredId = null; if (highlightNodeId && !validIds.has(highlightNodeId)) highlightNodeId = null; if (expandedNodeId && !validIds.has(expandedNodeId)) expandedNodeId = null; }

  onMount(() => {
    currentWindowLabel = appWindow.label ?? 'main';
    let disposed = false, cleanup = () => {};
    void bootstrap().then((fn) => { if (disposed) { fn(); return; } cleanup = fn; }).catch((error) => { fatalError = error?.stack ?? error?.message ?? String(error); });
    return () => {
      disposed = true;
      cleanup();
      if (typeof document !== 'undefined') { document.documentElement.classList.remove('desktop-overlay-window'); document.body.classList.remove('desktop-mode'); }
    };
  });
</script>

{#if fatalError}
  <pre class="fatal">{fatalError}</pre>
{:else}
  {#if isNodeBoardWindow}
    <main class="nodeboard-shell">
      <div class="nodeboard-frame">
        <!-- Drag handle bar -->
        <div class="nodeboard-titlebar" data-tauri-drag-region>
          <div class="nodeboard-titlebar__dots">
            <span class="tb-dot tb-dot--red"></span>
            <span class="tb-dot tb-dot--amber"></span>
            <span class="tb-dot tb-dot--green"></span>
          </div>
          <span class="nodeboard-titlebar__label" data-tauri-drag-region>FinNode</span>
          <div class="nodeboard-titlebar__spacer" data-tauri-drag-region></div>
        </div>
        <div class="canvas nodeboard-canvas" bind:this={nodeLayer}>
          <svg class="links" {viewBox}>
            <defs>
              <marker id="arrow" markerWidth="6" markerHeight="6" refX="5" refY="3" orient="auto">
                <path d="M0,0 L0,6 L6,3 z" fill="rgba(94,231,247,0.45)" />
              </marker>
            </defs>
            {#each links as link}
              <path
                class="link"
                class:link--highlight={highlightNodeId && (link.from === highlightNodeId || link.to === highlightNodeId)}
                d={link.d}
              ></path>
            {/each}
          </svg>

          <div class="node-layer" style="transform: scale({zoomLevel}); transform-origin: 0 0;">
            {#each renderNodes as node (node.id)}
              <div
                class="node"
                class:node--selected={selectedIds.has(node.id)}
                class:node--dragging={draggingId === node.id}
                class:node--expanded={expandedNodeId === node.id}
                class:node--main={node.id === MAIN_NODE_ID}
                class:node--hovered={hoveredId === node.id}
                role="button"
                tabindex="0"
                use:nodeRef={node.id}
                style="left:{node.renderX}px;top:{node.renderY}px;--node-color:{nodeColor(node)};--node-color-rgb:{nodeColor(node)}"
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
                      <button on:pointerdown|stopPropagation on:click|stopPropagation={() => void launchNode(node, 'open-path')}>
                        <span class="action-icon">📁</span> Path
                      </button>
                    {/if}
                    {#if hasLaunchTarget(node, 'open-editor')}
                      <button on:pointerdown|stopPropagation on:click|stopPropagation={() => void launchNode(node, 'open-editor')}>
                        <span class="action-icon">✏️</span> Edit
                      </button>
                    {/if}
                    {#if hasLaunchTarget(node, 'open-browser')}
                      <button on:pointerdown|stopPropagation on:click|stopPropagation={() => void launchNode(node, 'open-browser')}>
                        <span class="action-icon">🌐</span> Web
                      </button>
                    {/if}
                    {#if hasLaunchTarget(node, 'run-script')}
                      <button on:pointerdown|stopPropagation on:click|stopPropagation={() => void launchNode(node, 'run-script')}>
                        <span class="action-icon">▶</span> Run
                      </button>
                    {/if}
                  </div>
                {/if}

                <div class="node__ring"></div>
                <div class="node__inner">
                  <div class="node__top">
                    <span class="node__dot" class:node__dot--active={Boolean(node.last_launched)} class:node__dot--core={node.id === MAIN_NODE_ID}></span>
                    {#if !isLockedNode(node)}
                      <button class="node__edit" on:click|stopPropagation={() => openEditor(node.id)} title="Edit node">⋯</button>
                    {/if}
                  </div>

                  {#if isImageIcon(node.icon)}
                    <img class="node__icon-image" src={node.icon} alt={node.name || 'Node icon'} />
                  {:else if isLogoIcon(node.icon)}
                    <img class="node__logo" src={appLogo} alt="FinNode" />
                  {:else}
                    <div class="node__icon">{node.icon || node.name?.charAt(0)?.toUpperCase() || 'N'}</div>
                  {/if}

                  <div class="node__name">{node.name || 'Untitled'}</div>
                </div>

                {#if expandedNodeId === node.id && node.description}
                  <div class="node__hint">{node.description}</div>
                {/if}
              </div>
            {/each}
          </div>
        </div>
        <!-- Zoom controls -->
        <div class="zoom-controls">
          <button class="zoom-btn" on:click={zoomOut} title="Zoom out" disabled={zoomLevel <= ZOOM_MIN}>−</button>
          <span class="zoom-label">{Math.round(zoomLevel * 100)}%</span>
          <button class="zoom-btn" on:click={zoomIn}  title="Zoom in"  disabled={zoomLevel >= ZOOM_MAX}>+</button>
        </div>
      </div>
    </main>
  {:else}
    <main class="settings-shell">
      <aside class="sidebar settings-layer">

        <header class="panel brand-panel">
          <div class="brand__identity">
            <div class="brand__logo-wrap">
              <img class="brand__logo" src={appLogo} alt="FinNode" />
            </div>
            <div>
              <h1>FinNode</h1>
              <p>Project navigator</p>
            </div>
          </div>
          <div class="wm-buttons">
            <button class="wm-btn wm-btn--min" on:click={minimizeWindow} title="Minimize">−</button>
            <button class="wm-btn wm-btn--close" on:click={closeWindow}   title="Close">✕</button>
          </div>
        </header>

        <section class="panel">
          <div class="panel__head">
            <h2>Workspaces</h2>
            <span class="badge">{workspaces.length}</span>
          </div>
          <select value={activeWorkspaceId} on:change={(event) => switchWorkspace(event.currentTarget.value)}>
            {#each workspaces as workspace}
              <option value={workspace.id}>{workspace.name}</option>
            {/each}
          </select>

          <div class="row">
            <input bind:value={workspaceName} placeholder="New workspace name…" />
            <button class="btn-accent" on:click={createWorkspace}>+</button>
          </div>

          <button class="btn-danger" disabled={workspaces.length <= 1} on:click={() => deleteWorkspace(activeWorkspaceId)}>
            Delete workspace
          </button>
        </section>

        <section class="panel">
          <div class="panel__head">
            <h2>Nodes</h2>
            <span class="badge">{nodes.length}</span>
          </div>

          <div class="row row--tight">
            <button class="btn-primary" on:click={addNode}>＋ Add</button>
            <button class="btn-ghost" on:click={layoutGrid}>⊞ Layout</button>
            <button class="btn-ghost" on:click={openLauncher}>⌘ Search</button>
          </div>

          {#if selectedIds.size > 0}
            <div class="selection-bar">
              <span>{selectedIds.size} selected</span>
              <button on:click={batchLaunch}>Open</button>
              <button class="danger" on:click={batchDelete}>Delete</button>
              <button on:click={() => (selectedIds = new Set())}>✕</button>
            </div>
          {/if}

          <div class="node-list">
            {#each nodes as node (node.id)}
              {@const locked = isLockedNode(node)}
              <div class="node-row" class:node-row--selected={selectedIds.has(node.id)} style="--node-color:{nodeColor(node)}">
                <div class="node-row__pill" style="background:{nodeColor(node)}"></div>
                <div class="node-row__title" title={node.name || 'Untitled node'}>{node.name || 'Untitled node'}</div>
                <div class="node-row__actions">
                  <button disabled={!hasLaunchTarget(node, 'open-path')} on:click={() => void launchNode(node, 'open-path')}>▶</button>
                  {#if !locked}
                    <button on:click={() => openEditor(node.id)}>✏</button>
                  {/if}
                  <button disabled={locked} on:click={() => cloneNode(node.id)}>⧉</button>
                  <button class="btn-danger-sm" disabled={locked} on:click={() => deleteNode(node.id)}>✕</button>
                </div>
              </div>
            {/each}
          </div>
        </section>

        <section class="panel status-panel">
          <div class="panel__head">
            <h2>Status</h2>
            <span class="status-dot" class:status-dot--ok={!statusText.toLowerCase().includes('error')}></span>
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
          <button class="btn-ghost icon-btn" on:click={closeEditor}>✕</button>
        </header>

        <div class="editor-grid">
          <label class="field">
            <span>Name</span>
            <input bind:value={editDraft.name} placeholder="Node name" />
          </label>

          <label class="field">
            <span>Icon character</span>
            <input bind:value={editDraft.icon} placeholder="N" maxlength="4" />
          </label>
        </div>

        <label class="field">
          <span>Upload icon image</span>
          <input type="file" accept="image/*" on:change={handleNodeIconUpload} />
        </label>

        {#if isImageIcon(editDraft.icon)}
          <div class="icon-upload-preview">
            <img class="icon-upload-preview__img" src={editDraft.icon} alt="Icon preview" />
            <button type="button" class="btn-ghost" on:click={clearUploadedNodeIcon}>Remove image</button>
          </div>
        {/if}

        <label class="field">
          <span>Description</span>
          <textarea rows="2" bind:value={editDraft.description} placeholder="Short description…"></textarea>
        </label>

        <div class="field-group">
          <label class="field">
            <span>Folder path</span>
            <input bind:value={editDraft.path} placeholder="/path/to/project" />
          </label>
          <label class="field">
            <span>Editor path</span>
            <input bind:value={editDraft.editor} placeholder="/usr/bin/code" />
          </label>
          <label class="field">
            <span>Browser URL</span>
            <input bind:value={editDraft.browser} placeholder="https://example.com" />
          </label>
          <label class="field">
            <span>Script command</span>
            <input bind:value={editDraft.script} placeholder="npm run dev" />
          </label>
        </div>

        <label class="field">
          <span>Color</span>
          <div class="color-picker">
            {#each NODE_COLORS as color}
              <button
                type="button"
                class="color-swatch"
                class:color-swatch--active={editDraft.color === color}
                style="--swatch:{NODE_COLOR_MAP[color]}"
                on:click={() => (editDraft = { ...editDraft, color })}
                title={color}
              ></button>
            {/each}
          </div>
        </label>

        <section class="links-section">
          <h4>Links to other nodes</h4>
          <div class="links-list">
            {#each nodes.filter((node) => node.id !== editNode.id) as candidate (candidate.id)}
              <label class="link-row">
                <div class="link-row__dot" style="background:{nodeColor(candidate)}"></div>
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
          <button class="btn-accent" on:click={saveEditor}>Save changes</button>
          <button class="btn-ghost" on:click={closeEditor}>Cancel</button>
        </footer>
      </section>
    </div>
  {/if}

  {#if contextMenu.open && contextNode}
    <div class="context-menu" style="left:{contextMenu.x}px;top:{contextMenu.y}px;" on:pointerdown|stopPropagation>
      <div class="context-menu__title">
        <div class="ctx-dot" style="background:{nodeColor(contextNode)}"></div>
        {contextNode.name || 'Untitled node'}
      </div>
      <div class="ctx-group">
        <button disabled={!hasLaunchTarget(contextNode, 'open-path')} on:click={() => launchFromContext('open-path')}>📁 Open path</button>
        <button disabled={!hasLaunchTarget(contextNode, 'open-editor')} on:click={() => launchFromContext('open-editor')}>✏️ Open editor</button>
        <button disabled={!hasLaunchTarget(contextNode, 'open-browser')} on:click={() => launchFromContext('open-browser')}>🌐 Open browser</button>
        <button disabled={!hasLaunchTarget(contextNode, 'run-script')} on:click={() => launchFromContext('run-script')}>▶ Run script</button>
      </div>
      <div class="ctx-divider"></div>
      <div class="ctx-group">
        {#if !isLockedNode(contextNode)}<button on:click={openEditorFromMenu}>⚙ Edit node</button>{/if}
        <button on:click={addConnected}>⊕ Add connected</button>
        <button on:click={connectNearest}>⟶ Connect nearest</button>
        <button on:click={clearLinks}>⊘ Clear links</button>
        <button disabled={isLockedNode(contextNode)} on:click={() => { cloneNode(contextNode.id); closeCtx(); }}>⧉ Clone</button>
      </div>
      <div class="ctx-divider"></div>
      <button class="ctx-danger" disabled={isLockedNode(contextNode)} on:click={() => { deleteNode(contextNode.id); closeCtx(); }}>✕ Delete node</button>
    </div>
  {/if}

  {#if showLauncher}
    <!-- svelte-ignore a11y-click-events-have-key-events -->
    <div class="launcher-overlay" role="presentation" on:click|self={closeLauncher}>
      <section class="launcher">
        <div class="launcher__search-wrap">
          <span class="launcher__search-icon">⌘</span>
          <input
            id="launcher-input"
            class="launcher__input"
            placeholder="Search nodes…"
            bind:value={launcherQuery}
            on:keydown={launcherKey}
          />
          {#if launcherQuery}
            <button class="launcher__clear" on:click={() => (launcherQuery = '')}>✕</button>
          {/if}
        </div>
        <div class="launcher__list">
          {#each launcherResults as node, index (node.id)}
            <button
              class="launcher__item"
              class:launcher__item--active={index === launcherIndex}
              on:click={() => { void launchNode(node, 'open-path'); closeLauncher(); }}
            >
              <span class="launcher__item-dot" style="background:{nodeColor(node)}"></span>
              <span class="launcher__item-name">{node.name || 'Untitled node'}</span>
              {#if node.targets?.path}
                <span class="launcher__item-path">{node.targets.path}</span>
              {/if}
            </button>
          {/each}
          {#if launcherResults.length === 0}
            <div class="launcher__empty">No matching nodes</div>
          {/if}
        </div>
        <div class="launcher__footer">
          <span>↑↓ navigate</span>
          <span>↵ open</span>
          <span>esc close</span>
        </div>
      </section>
    </div>
  {/if}
{/if}

<style>
  /* ─── Tokens ───────────────────────────────────────────────────── */
  :global(:root) {
    --bg:          #080d14;
    --surface-0:   #0b1220;
    --surface-1:   #0f1928;
    --surface-2:   #162030;
    --surface-3:   #1c2a3e;
    --text:        #dde8f2;
    --text-soft:   #7a99b8;
    --text-dim:    #4a6278;
    --accent:      #5ee7f7;
    --accent-glow: rgba(94, 231, 247, 0.22);
    --accent-2:    #b8a4ff;
    --gold:        #fdd87a;
    --gold-glow:   rgba(253, 216, 122, 0.28);
    --danger:      #ff6b82;
    --success:     #6ee89a;
    --border:      rgba(130, 165, 200, 0.14);
    --border-soft: rgba(130, 165, 200, 0.08);
    --radius-sm:   8px;
    --radius-md:   12px;
    --radius-lg:   18px;
    --radius-xl:   24px;
    --settings-w:  320px;
    --node-sz:     108px;
  }

  /* ─── Reset ────────────────────────────────────────────────────── */
  * { box-sizing: border-box; }

  :global(html), :global(body), :global(#app) { width: 100%; height: 100%; margin: 0; }

  :global(body) {
    font-family: 'DM Sans', 'Segoe UI', system-ui, sans-serif;
    background: radial-gradient(ellipse at 30% 20%, #0d1f35 0%, #080d14 60%);
    color: var(--text);
    -webkit-font-smoothing: antialiased;
  }

  :global(html.desktop-overlay-window),
  :global(body.desktop-mode) { background: transparent; }

  /* ─── Shells ───────────────────────────────────────────────────── */
  .settings-shell {
    width: 100%;
    height: 100%;
    padding: 16px;
    display: flex;
    align-items: flex-start;
    overflow: auto;
  }

  .nodeboard-shell {
    width: 100%;
    height: 100%;
    display: grid;
    place-items: center;
    background: transparent;
  }

  .nodeboard-frame {
    width: 100%;
    height: 100%;
    overflow: hidden;
    border: 1px solid rgba(94, 231, 247, 0.14);
    border-radius: 22px;
    box-shadow:
      0 0 0 1px rgba(94, 231, 247, 0.06),
      0 32px 64px rgba(0, 0, 0, 0.45),
      inset 0 1px 0 rgba(255,255,255,0.06);
    /* Frosted glass */
    background: rgba(8, 13, 22, 0.42);
    backdrop-filter: blur(28px) saturate(1.4);
    -webkit-backdrop-filter: blur(28px) saturate(1.4);
    display: flex;
    flex-direction: column;
    position: relative;
  }

  .nodeboard-canvas {
    flex: 1;
    min-height: 0;
    width: 100%;
    border: 0;
    border-radius: 0;
    box-shadow: none;
    background:
      radial-gradient(ellipse at 20% 15%, rgba(94,231,247,0.05) 0%, transparent 55%),
      radial-gradient(ellipse at 80% 85%, rgba(184,164,255,0.04) 0%, transparent 55%),
      rgba(6, 11, 18, 0.18);
  }

  /* ─── Sidebar ──────────────────────────────────────────────────── */
  .sidebar {
    width: var(--settings-w);
    display: flex;
    flex-direction: column;
    gap: 10px;
    overflow: auto;
    padding-right: 2px;
    position: relative;
    z-index: 20;
  }

  .settings-layer { pointer-events: auto; }

  /* ─── Panels ───────────────────────────────────────────────────── */
  .panel {
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: linear-gradient(160deg, var(--surface-1) 0%, var(--surface-0) 100%);
    padding: 14px;
    box-shadow: 0 4px 16px rgba(0,0,0,0.2), inset 0 1px 0 rgba(255,255,255,0.03);
  }

  .brand-panel {
    padding: 16px;
    background: linear-gradient(160deg, var(--surface-2) 0%, var(--surface-1) 100%);
    border-color: rgba(94,231,247,0.12);
  }

  .brand__identity { display: flex; align-items: center; gap: 12px; }

  .brand__logo-wrap {
    width: 44px;
    height: 44px;
    border-radius: 14px;
    background: linear-gradient(135deg, rgba(94,231,247,0.15), rgba(184,164,255,0.1));
    border: 1px solid rgba(94,231,247,0.2);
    display: grid;
    place-items: center;
    flex-shrink: 0;
    box-shadow: 0 0 20px rgba(94,231,247,0.08);
  }

  .brand__logo { width: 28px; height: 28px; object-fit: contain; border-radius: 6px; }

  .brand__identity h1 {
    margin: 0;
    font-size: 1.05rem;
    font-weight: 700;
    letter-spacing: 0.03em;
    background: linear-gradient(90deg, var(--text) 0%, rgba(221,232,242,0.7) 100%);
    -webkit-background-clip: text;
    -webkit-text-fill-color: transparent;
    background-clip: text;
  }

  .brand__identity p { margin: 3px 0 0; color: var(--text-soft); font-size: 0.76rem; }

  .panel__head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 10px;
  }

  .panel__head h2 {
    margin: 0;
    font-size: 0.68rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.14em;
    color: var(--text-soft);
  }

  .badge {
    font-size: 0.68rem;
    font-weight: 600;
    padding: 2px 7px;
    border-radius: 999px;
    background: rgba(94,231,247,0.1);
    border: 1px solid rgba(94,231,247,0.18);
    color: var(--accent);
  }

  /* ─── Form elements ────────────────────────────────────────────── */
  .panel select,
  .panel input:not([type='file']):not([type='checkbox']) {
    width: 100%;
    font: inherit;
    font-size: 0.82rem;
    color: var(--text);
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: rgba(8,13,20,0.7);
    padding: 8px 10px;
    outline: none;
    transition: border-color 150ms;
  }

  .panel select:focus,
  .panel input:not([type='file']):not([type='checkbox']):focus { border-color: rgba(94,231,247,0.4); box-shadow: 0 0 0 3px rgba(94,231,247,0.07); }

  /* ─── Buttons ──────────────────────────────────────────────────── */
  button {
    font: inherit;
    cursor: pointer;
    border-radius: var(--radius-sm);
    transition: all 120ms ease;
  }

  button:disabled { opacity: 0.38; cursor: not-allowed; }

  .btn-primary {
    padding: 8px 12px;
    background: linear-gradient(135deg, rgba(94,231,247,0.2), rgba(94,231,247,0.1));
    border: 1px solid rgba(94,231,247,0.3);
    color: var(--accent);
    font-size: 0.8rem;
    font-weight: 600;
  }
  .btn-primary:hover:not(:disabled) { background: linear-gradient(135deg, rgba(94,231,247,0.28), rgba(94,231,247,0.15)); border-color: rgba(94,231,247,0.5); }

  .btn-accent {
    padding: 8px 14px;
    background: linear-gradient(135deg, rgba(94,231,247,0.25), rgba(94,231,247,0.12));
    border: 1px solid rgba(94,231,247,0.35);
    color: var(--accent);
    font-weight: 600;
  }
  .btn-accent:hover:not(:disabled) { background: linear-gradient(135deg, rgba(94,231,247,0.35), rgba(94,231,247,0.2)); }

  .btn-ghost {
    padding: 8px 12px;
    background: rgba(8,13,20,0.5);
    border: 1px solid var(--border);
    color: var(--text-soft);
    font-size: 0.8rem;
  }
  .btn-ghost:hover:not(:disabled) { border-color: rgba(130,165,200,0.3); color: var(--text); }

  .btn-danger {
    width: 100%;
    margin-top: 8px;
    padding: 8px 12px;
    background: rgba(255,107,130,0.06);
    border: 1px solid rgba(255,107,130,0.22);
    color: #ff9aaa;
    font-size: 0.8rem;
  }
  .btn-danger:hover:not(:disabled) { background: rgba(255,107,130,0.12); border-color: rgba(255,107,130,0.4); }

  .btn-danger-sm {
    padding: 5px 7px;
    background: rgba(255,107,130,0.06);
    border: 1px solid rgba(255,107,130,0.2);
    color: #ff9aaa;
    font-size: 0.7rem;
    border-radius: 6px;
  }
  .btn-danger-sm:hover:not(:disabled) { background: rgba(255,107,130,0.14); }

  .icon-btn {
    width: 32px;
    height: 32px;
    padding: 0;
    display: grid;
    place-items: center;
    border-radius: var(--radius-sm);
    font-size: 0.75rem;
  }

  /* ─── Layout helpers ───────────────────────────────────────────── */
  .row {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 8px;
    margin-top: 8px;
  }
  .row--tight { grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 6px; margin-top: 8px; }

  /* ─── Selection bar ────────────────────────────────────────────── */
  .selection-bar {
    margin-top: 10px;
    display: grid;
    grid-template-columns: 1fr repeat(3, auto);
    gap: 5px;
    align-items: center;
    background: rgba(94,231,247,0.05);
    border: 1px solid rgba(94,231,247,0.15);
    border-radius: var(--radius-sm);
    padding: 8px;
    font-size: 0.76rem;
    color: var(--accent);
  }
  .selection-bar button {
    padding: 5px 9px;
    font-size: 0.72rem;
    background: rgba(8,13,20,0.5);
    border: 1px solid var(--border);
    color: var(--text-soft);
    border-radius: 6px;
  }
  .selection-bar .danger { border-color: rgba(255,107,130,0.22); color: #ff9aaa; }

  /* ─── Node list (sidebar) ──────────────────────────────────────── */
  .node-list {
    margin-top: 10px;
    display: flex;
    flex-direction: column;
    gap: 5px;
    max-height: 240px;
    overflow: auto;
  }

  .node-row {
    border: 1px solid var(--border-soft);
    border-radius: var(--radius-sm);
    background: rgba(8,13,20,0.5);
    padding: 8px 10px;
    display: flex;
    align-items: center;
    gap: 8px;
    transition: border-color 120ms, background 120ms;
  }
  .node-row:hover { background: rgba(94,231,247,0.03); border-color: var(--border); }
  .node-row--selected { border-color: rgba(94,231,247,0.3); background: rgba(94,231,247,0.04); }

  .node-row__pill {
    width: 4px;
    height: 28px;
    border-radius: 2px;
    flex-shrink: 0;
    opacity: 0.8;
  }

  .node-row__title {
    flex: 1;
    font-size: 0.8rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text);
  }

  .node-row__actions {
    display: flex;
    gap: 4px;
    flex-shrink: 0;
  }
  .node-row__actions button {
    width: 26px;
    height: 26px;
    padding: 0;
    display: grid;
    place-items: center;
    font-size: 0.72rem;
    background: rgba(8,13,20,0.6);
    border: 1px solid var(--border);
    color: var(--text-soft);
    border-radius: 6px;
  }
  .node-row__actions button:hover:not(:disabled) { border-color: rgba(94,231,247,0.3); color: var(--accent); }

  /* ─── Status ───────────────────────────────────────────────────── */
  .status-panel { display: flex; flex-direction: column; gap: 8px; }

  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--text-dim);
    flex-shrink: 0;
  }
  .status-dot--ok { background: var(--success); box-shadow: 0 0 6px rgba(110,232,154,0.5); }

  .status-text { margin: 0; font-size: 0.82rem; color: var(--text-soft); }

  .activity { display: flex; flex-direction: column; gap: 5px; overflow: auto; max-height: 110px; }

  .activity__item {
    font-size: 0.72rem;
    color: var(--text-dim);
    padding: 4px 0;
    border-bottom: 1px solid var(--border-soft);
    line-height: 1.3;
  }

  .muted { margin: 0; color: var(--text-dim); font-size: 0.76rem; }

  /* ─── Canvas ───────────────────────────────────────────────────── */
  .canvas {
    position: relative;
    width: 100%;
    height: 100%;
    border-radius: var(--radius-md);
    overflow: hidden;
    z-index: 0;
  }

  .links, .node-layer {
    position: absolute;
    inset: 0;
  }

  .links { pointer-events: none; }

  .link {
    fill: none;
    stroke: rgba(94,231,247,0.18);
    stroke-width: 1.5;
    stroke-dasharray: none;
    transition: stroke 200ms;
  }
  .link--highlight {
    stroke: rgba(94,231,247,0.65);
    stroke-width: 2;
    filter: drop-shadow(0 0 4px rgba(94,231,247,0.35));
  }

  .node-layer { pointer-events: none; }

  /* ─── NODES ────────────────────────────────────────────────────── */
  .node {
    position: absolute;
    /* Force a strict square so border-radius:50% gives a circle */
    width: var(--node-sz);
    height: var(--node-sz);
    min-width: var(--node-sz);
    min-height: var(--node-sz);
    max-width: var(--node-sz);
    max-height: var(--node-sz);
    flex-shrink: 0;
    aspect-ratio: 1 / 1;
    border-radius: 50% !important;

    pointer-events: auto;
    user-select: none;
    cursor: grab;
    transform-origin: center center;
    transition:
      box-shadow 200ms ease,
      transform 180ms cubic-bezier(0.34, 1.56, 0.64, 1);

    /* Layered depth */
    background:
      radial-gradient(circle at 30% 25%, rgba(255,255,255,0.09) 0%, transparent 52%),
      linear-gradient(180deg, var(--surface-2) 0%, var(--surface-1) 100%);
    border: 1.5px solid rgba(130,165,200,0.22);
    box-shadow:
      0 8px 24px rgba(0,0,0,0.35),
      0 2px 8px rgba(0,0,0,0.2),
      inset 0 1px 0 rgba(255,255,255,0.05),
      inset 0 -1px 0 rgba(0,0,0,0.15);

    /* Inner layout — clip content to the circle */
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 3px;
    padding: 10px;
    overflow: hidden;
  }

  /* The subtle outer ring that lights up */
  .node__ring {
    position: absolute;
    inset: -3px;
    border-radius: 50%;
    border: 1.5px solid transparent;
    background: transparent;
    transition: all 250ms ease;
    pointer-events: none;
  }

  .node__inner {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 3px;
    width: 100%;
    height: 100%;
  }

  /* Hover state */
  .node--hovered .node__ring {
    border-color: var(--node-color);
    box-shadow: 0 0 16px color-mix(in srgb, var(--node-color) 50%, transparent);
    opacity: 0.5;
  }

  .node--hovered {
    transform: scale(1.05);
    box-shadow:
      0 12px 32px rgba(0,0,0,0.45),
      0 0 0 1px var(--node-color),
      inset 0 1px 0 rgba(255,255,255,0.07);
  }

  /* Selected */
  .node--selected {
    border-color: var(--accent);
    box-shadow:
      0 10px 28px rgba(0,0,0,0.4),
      0 0 0 2px rgba(94,231,247,0.5),
      0 0 20px rgba(94,231,247,0.12),
      inset 0 1px 0 rgba(255,255,255,0.06);
  }

  /* Main / core node */
  .node--main {
    border-color: rgba(253,216,122,0.5);
    background:
      radial-gradient(circle at 28% 22%, rgba(253,216,122,0.18) 0%, transparent 55%),
      linear-gradient(180deg, rgba(38,28,12,0.95) 0%, rgba(22,16,8,0.92) 100%);
    box-shadow:
      0 10px 28px rgba(0,0,0,0.4),
      0 0 0 1px rgba(253,216,122,0.25),
      0 0 24px rgba(253,216,122,0.1),
      inset 0 1px 0 rgba(255,255,255,0.06);
  }

  .node--main .node__name { color: var(--gold); font-weight: 700; }
  .node--main .node__dot--core { background: var(--gold); box-shadow: 0 0 8px rgba(253,216,122,0.7); }

  /* Dragging */
  .node--dragging {
    cursor: grabbing;
    transform: scale(1.06) rotate(1deg);
    box-shadow:
      0 20px 48px rgba(0,0,0,0.55),
      0 0 0 1.5px rgba(94,231,247,0.35),
      inset 0 1px 0 rgba(255,255,255,0.06);
    z-index: 10;
  }

  /* Expanded */
  .node--expanded {
    transform: scale(1.18);
    z-index: 8;
    border-color: rgba(94,231,247,0.5);
    box-shadow:
      0 16px 40px rgba(0,0,0,0.5),
      0 0 0 1.5px rgba(94,231,247,0.4),
      0 0 32px rgba(94,231,247,0.1),
      inset 0 1px 0 rgba(255,255,255,0.06);
  }

  /* Node top row */
  .node__top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 0 2px;
  }

  .node__dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: rgba(130,165,200,0.3);
    flex-shrink: 0;
    transition: background 200ms, box-shadow 200ms;
  }
  .node__dot--active { background: var(--success); box-shadow: 0 0 6px rgba(110,232,154,0.5); }
  .node__dot--core   { background: var(--gold);    box-shadow: 0 0 8px rgba(253,216,122,0.7); }

  .node__edit {
    width: 18px;
    height: 18px;
    padding: 0;
    display: grid;
    place-items: center;
    font-size: 0.8rem;
    line-height: 1;
    background: rgba(8,13,20,0.6);
    border: 1px solid rgba(130,165,200,0.2);
    color: var(--text-soft);
    border-radius: 5px;
    opacity: 0;
    transition: opacity 150ms;
  }

  .node:hover .node__edit { opacity: 1; }
  .node__edit:hover { border-color: rgba(94,231,247,0.4); color: var(--accent); }

  /* Icon */
  .node__icon {
    width: 38px;
    height: 38px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    font-size: 1rem;
    font-weight: 700;
    color: var(--node-color);
    background: radial-gradient(circle, rgba(8,13,20,0.9) 0%, rgba(12,20,32,0.7) 100%);
    border: 1px solid rgba(130,165,200,0.15);
    box-shadow: 0 0 12px color-mix(in srgb, var(--node-color) 20%, transparent);
    text-shadow: 0 0 8px var(--node-color);
    flex-shrink: 0;
  }

  .node__logo {
    width: 38px;
    height: 38px;
    border-radius: 50%;
    object-fit: contain;
    padding: 6px;
    background: rgba(8,13,20,0.7);
    border: 1px solid rgba(130,165,200,0.15);
    flex-shrink: 0;
  }

  .node__icon-image {
    width: 40px;
    height: 40px;
    border-radius: 50%;
    object-fit: cover;
    border: 1.5px solid rgba(130,165,200,0.2);
    flex-shrink: 0;
  }

  .node__name {
    font-size: 0.66rem;
    font-weight: 500;
    letter-spacing: 0.02em;
    text-align: center;
    max-width: 88px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text-soft);
    line-height: 1;
  }

  .node__hint {
    position: absolute;
    bottom: -30px;
    left: 50%;
    transform: translateX(-50%);
    white-space: nowrap;
    font-size: 0.6rem;
    color: var(--text-dim);
    background: rgba(8,13,20,0.88);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 3px 7px;
    pointer-events: none;
    z-index: 5;
  }

  /* Run actions bubble */
  .node__run-actions {
    position: absolute;
    top: -44px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 3px;
    padding: 5px 7px;
    border-radius: 999px;
    border: 1px solid rgba(94,231,247,0.18);
    background: rgba(8,13,20,0.94);
    backdrop-filter: blur(8px);
    box-shadow: 0 8px 24px rgba(0,0,0,0.35), 0 0 0 1px rgba(94,231,247,0.05);
    z-index: 12;
    white-space: nowrap;
  }

  .node__run-actions button {
    width: auto;
    min-width: 0;
    padding: 4px 8px;
    border-radius: 999px;
    border: 1px solid rgba(94,231,247,0.15);
    background: rgba(14,24,38,0.9);
    color: var(--text-soft);
    font-size: 0.58rem;
    font-weight: 500;
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 3px;
    transition: all 120ms;
  }
  .node__run-actions button:hover { border-color: rgba(94,231,247,0.45); color: var(--accent); background: rgba(94,231,247,0.06); }

  .action-icon { font-size: 0.65rem; }

  /* ─── Modals ───────────────────────────────────────────────────── */
  .modal, .launcher-overlay {
    position: fixed;
    inset: 0;
    background: rgba(4,8,14,0.72);
    backdrop-filter: blur(4px);
    display: grid;
    place-items: center;
    z-index: 30;
    padding: 20px;
  }

  /* ─── Editor modal ─────────────────────────────────────────────── */
  .editor-modal {
    width: min(480px, 96vw);
    max-height: 90vh;
    overflow: auto;
    border-radius: var(--radius-xl);
    border: 1px solid rgba(130,165,200,0.2);
    background: linear-gradient(160deg, var(--surface-2) 0%, var(--surface-1) 50%, var(--surface-0) 100%);
    padding: 20px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    box-shadow: 0 32px 64px rgba(0,0,0,0.5), 0 0 0 1px rgba(255,255,255,0.03);
  }

  .editor-modal header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 2px;
  }
  .editor-modal h3 { margin: 0; font-size: 0.95rem; font-weight: 700; letter-spacing: 0.02em; }

  .editor-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }

  .field { display: flex; flex-direction: column; gap: 5px; }

  .field span, .links-section h4 {
    font-size: 0.65rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: var(--text-dim);
  }

  .field input:not([type='file']):not([type='checkbox']),
  .field textarea {
    width: 100%;
    font: inherit;
    font-size: 0.82rem;
    color: var(--text);
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: rgba(8,13,20,0.7);
    padding: 8px 10px;
    outline: none;
    transition: border-color 150ms;
  }
  .field input:focus, .field textarea:focus {
    border-color: rgba(94,231,247,0.35);
    box-shadow: 0 0 0 3px rgba(94,231,247,0.06);
  }

  .field input[type='file'] {
    font: inherit;
    font-size: 0.78rem;
    color: var(--text-soft);
    background: rgba(8,13,20,0.5);
    border: 1px dashed var(--border);
    border-radius: var(--radius-sm);
    padding: 8px;
    width: 100%;
  }

  .field-group { display: flex; flex-direction: column; gap: 8px; }

  .icon-upload-preview {
    border: 1px dashed rgba(94,231,247,0.2);
    border-radius: var(--radius-md);
    padding: 10px;
    display: flex;
    align-items: center;
    gap: 12px;
    background: rgba(94,231,247,0.03);
  }

  .icon-upload-preview__img {
    width: 48px;
    height: 48px;
    border-radius: 50%;
    object-fit: cover;
    border: 1.5px solid rgba(130,165,200,0.25);
  }

  /* Color picker */
  .color-picker { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }

  .color-swatch {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    background: var(--swatch);
    border: 2px solid transparent;
    cursor: pointer;
    transition: transform 120ms, border-color 120ms;
    box-shadow: 0 0 8px color-mix(in srgb, var(--swatch) 40%, transparent);
    padding: 0;
  }
  .color-swatch:hover { transform: scale(1.15); }
  .color-swatch--active { border-color: rgba(255,255,255,0.7); transform: scale(1.12); }

  /* Links section */
  .links-section {
    border-top: 1px solid var(--border-soft);
    padding-top: 12px;
  }
  .links-section h4 { margin: 0 0 8px; }

  .links-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 140px;
    overflow: auto;
  }

  .link-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 0.8rem;
    border: 1px solid var(--border-soft);
    border-radius: var(--radius-sm);
    padding: 7px 10px;
    background: rgba(8,13,20,0.5);
    cursor: pointer;
    transition: border-color 120ms;
  }
  .link-row:hover { border-color: var(--border); }

  .link-row__dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
  .link-row span { flex: 1; }
  .link-row input[type='checkbox'] { width: 14px; height: 14px; accent-color: var(--accent); }

  .editor-modal footer {
    display: flex;
    align-items: center;
    gap: 8px;
    justify-content: flex-end;
    border-top: 1px solid var(--border-soft);
    padding-top: 12px;
    margin-top: 2px;
  }

  /* ─── Context menu ─────────────────────────────────────────────── */
  .context-menu {
    position: fixed;
    z-index: 40;
    width: 210px;
    border: 1px solid rgba(130,165,200,0.18);
    border-radius: var(--radius-lg);
    padding: 8px;
    background: linear-gradient(160deg, var(--surface-2) 0%, var(--surface-1) 100%);
    box-shadow: 0 20px 48px rgba(0,0,0,0.5), 0 0 0 1px rgba(255,255,255,0.03);
    backdrop-filter: blur(16px);
  }

  .context-menu__title {
    font-size: 0.76rem;
    font-weight: 600;
    color: var(--text-soft);
    padding: 4px 6px 8px;
    display: flex;
    align-items: center;
    gap: 7px;
    border-bottom: 1px solid var(--border-soft);
    margin-bottom: 5px;
  }

  .ctx-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }

  .ctx-group { display: flex; flex-direction: column; gap: 2px; }
  .ctx-divider { height: 1px; background: var(--border-soft); margin: 5px 0; }

  .context-menu button {
    width: 100%;
    text-align: left;
    font-size: 0.76rem;
    padding: 7px 8px;
    border-radius: var(--radius-sm);
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-soft);
    transition: all 100ms;
  }
  .context-menu button:hover:not(:disabled) { background: rgba(94,231,247,0.06); border-color: rgba(94,231,247,0.12); color: var(--text); }
  .context-menu button:disabled { opacity: 0.35; }

  .ctx-danger { color: #ff8fa8 !important; }
  .ctx-danger:hover:not(:disabled) { background: rgba(255,107,130,0.08) !important; border-color: rgba(255,107,130,0.2) !important; }

  /* ─── Launcher ─────────────────────────────────────────────────── */
  .launcher {
    width: min(520px, 94vw);
    border-radius: var(--radius-xl);
    border: 1px solid rgba(130,165,200,0.18);
    background: linear-gradient(160deg, var(--surface-2) 0%, var(--surface-1) 100%);
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    box-shadow: 0 32px 64px rgba(0,0,0,0.55), 0 0 0 1px rgba(255,255,255,0.03);
    backdrop-filter: blur(20px);
  }

  .launcher__search-wrap {
    display: flex;
    align-items: center;
    gap: 10px;
    background: rgba(8,13,20,0.7);
    border: 1px solid rgba(94,231,247,0.2);
    border-radius: var(--radius-md);
    padding: 10px 12px;
    box-shadow: 0 0 0 3px rgba(94,231,247,0.05);
  }

  .launcher__search-icon { font-size: 0.9rem; color: var(--accent); opacity: 0.6; }

  .launcher__input {
    flex: 1;
    border: none;
    background: transparent;
    font: inherit;
    font-size: 0.9rem;
    color: var(--text);
    outline: none;
  }
  .launcher__input::placeholder { color: var(--text-dim); }

  .launcher__clear {
    background: none;
    border: none;
    color: var(--text-dim);
    font-size: 0.75rem;
    padding: 2px;
    cursor: pointer;
    border-radius: 4px;
  }
  .launcher__clear:hover { color: var(--text); }

  .launcher__list {
    display: flex;
    flex-direction: column;
    gap: 3px;
    max-height: 300px;
    overflow: auto;
  }

  .launcher__item {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    text-align: left;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: rgba(8,13,20,0.4);
    color: var(--text);
    padding: 10px 12px;
    cursor: pointer;
    transition: all 100ms;
  }
  .launcher__item:hover,
  .launcher__item--active { background: rgba(94,231,247,0.06); border-color: rgba(94,231,247,0.15); }

  .launcher__item-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
  .launcher__item-name { flex: 1; font-size: 0.84rem; font-weight: 500; }
  .launcher__item-path { font-size: 0.7rem; color: var(--text-dim); max-width: 180px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .launcher__empty { padding: 16px 12px; font-size: 0.8rem; color: var(--text-dim); text-align: center; }

  .launcher__footer {
    display: flex;
    gap: 12px;
    justify-content: center;
    font-size: 0.66rem;
    color: var(--text-dim);
    border-top: 1px solid var(--border-soft);
    padding-top: 8px;
  }
  .launcher__footer span {
    padding: 2px 6px;
    border: 1px solid var(--border-soft);
    border-radius: 4px;
    background: rgba(8,13,20,0.4);
  }

  /* ─── Fatal error ──────────────────────────────────────────────── */
  .fatal { margin: 0; padding: 20px; color: #ff9aaa; white-space: pre-wrap; font-size: 0.82rem; }

  /* ─── Scrollbars ───────────────────────────────────────────────── */
  :global(*::-webkit-scrollbar) { width: 4px; height: 4px; }
  :global(*::-webkit-scrollbar-track) { background: transparent; }
  :global(*::-webkit-scrollbar-thumb) { background: rgba(130,165,200,0.2); border-radius: 2px; }
  :global(*::-webkit-scrollbar-thumb:hover) { background: rgba(130,165,200,0.35); }

  /* ─── Nodeboard title bar ──────────────────────────────────────── */
  .nodeboard-titlebar {
    flex-shrink: 0;
    height: 32px;
    display: flex;
    align-items: center;
    gap: 0;
    padding: 0 12px;
    background: rgba(8, 13, 22, 0.55);
    border-bottom: 1px solid rgba(94, 231, 247, 0.07);
    border-radius: 22px 22px 0 0;
    cursor: grab;
    user-select: none;
  }
  .nodeboard-titlebar:active { cursor: grabbing; }

  .nodeboard-titlebar__dots { display: flex; align-items: center; gap: 5px; flex-shrink: 0; }

  .tb-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    opacity: 0.75;
    transition: opacity 150ms;
  }
  .nodeboard-titlebar:hover .tb-dot { opacity: 1; }
  .tb-dot--red   { background: #ff5f57; }
  .tb-dot--amber { background: #febc2e; }
  .tb-dot--green { background: #28c840; }

  .nodeboard-titlebar__label {
    flex: 1;
    text-align: center;
    font-size: 0.7rem;
    font-weight: 600;
    letter-spacing: 0.12em;
    color: rgba(200, 220, 240, 0.35);
    pointer-events: none;
    text-transform: uppercase;
  }

  .nodeboard-titlebar__spacer { width: 46px; flex-shrink: 0; }

  /* ─── Zoom controls ─────────────────────────────────────────────── */
  .zoom-controls {
    position: absolute;
    bottom: 14px;
    right: 14px;
    display: flex;
    align-items: center;
    gap: 4px;
    background: rgba(8, 13, 22, 0.7);
    backdrop-filter: blur(8px);
    border: 1px solid rgba(94, 231, 247, 0.14);
    border-radius: 999px;
    padding: 4px 8px;
    z-index: 20;
    pointer-events: auto;
    box-shadow: 0 4px 16px rgba(0,0,0,0.3);
  }

  .zoom-btn {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    border: 1px solid rgba(130, 165, 200, 0.2);
    background: rgba(20, 32, 48, 0.8);
    color: var(--text-soft);
    font-size: 1rem;
    line-height: 1;
    display: grid;
    place-items: center;
    cursor: pointer;
    transition: all 120ms;
    padding: 0;
  }
  .zoom-btn:hover:not(:disabled) { border-color: rgba(94, 231, 247, 0.4); color: var(--accent); background: rgba(94, 231, 247, 0.08); }
  .zoom-btn:disabled { opacity: 0.3; cursor: not-allowed; }

  .zoom-label {
    font-size: 0.66rem;
    font-weight: 600;
    color: var(--text-dim);
    min-width: 34px;
    text-align: center;
    letter-spacing: 0.04em;
  }

  /* ─── Window manager buttons (settings) ────────────────────────── */
  .wm-buttons {
    display: flex;
    gap: 5px;
    margin-left: auto;
    flex-shrink: 0;
  }

  .wm-btn {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    border: none;
    font-size: 0.72rem;
    font-weight: 700;
    display: grid;
    place-items: center;
    cursor: pointer;
    transition: all 120ms;
    padding: 0;
  }

  .wm-btn--min {
    background: rgba(254, 188, 46, 0.15);
    color: #febc2e;
    border: 1px solid rgba(254, 188, 46, 0.3);
  }
  .wm-btn--min:hover { background: rgba(254, 188, 46, 0.28); }

  .wm-btn--close {
    background: rgba(255, 95, 87, 0.15);
    color: #ff5f57;
    border: 1px solid rgba(255, 95, 87, 0.3);
  }
  .wm-btn--close:hover { background: rgba(255, 95, 87, 0.28); }

  /* ─── Responsive ───────────────────────────────────────────────── */
  @media (max-width: 980px) {
    .settings-shell { padding: 10px; }
    .settings-layer.sidebar { width: 100%; max-height: 100%; padding-right: 0; }
    .nodeboard-shell { padding: 0; }
    .nodeboard-frame { width: 100%; height: 100%; }
    .row--tight { grid-template-columns: 1fr 1fr; }
    .selection-bar { grid-template-columns: 1fr 1fr; }
    .editor-grid { grid-template-columns: 1fr; }
  }
</style>