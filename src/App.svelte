<script>
  import { onMount, tick } from 'svelte';
  import { fade, fly, scale } from 'svelte/transition';
  import { invoke as tauriInvoke } from '@tauri-apps/api/tauri';
  import { listen as tauriListen } from '@tauri-apps/api/event';
  import { appWindow } from '@tauri-apps/api/window';
  import { readText as readClipboardText } from '@tauri-apps/api/clipboard';
  import { HAS_TAURI, previewInvoke, previewListen } from './preview-mock.js';
  import appLogo from '../src-tauri/icons/icon.png';

  // Use the real Tauri IPC inside the desktop shell; fall back to an in-memory
  // bridge when the frontend is served as a plain web page (preview/dev).
  const invoke = HAS_TAURI ? tauriInvoke : previewInvoke;
  const listen = HAS_TAURI ? tauriListen : previewListen;

  const MAIN_NODE_ID   = 'main-node';
  const MAIN_NODE_NAME = 'main';
  const LOGO_ICON      = 'logo';
  const NODE_SIZE      = 108;
  const SAVE_DEBOUNCE_MS  = 220;
  const SPRING_STIFFNESS  = 0.22;
  const SPRING_DAMPING    = 0.80;
  const MAX_NODE_ICON_BYTES = 512 * 1024;
  const MAX_SCRIPT_UPLOAD_BYTES = 1024 * 1024;

  const NODE_COLORS    = ['slate', 'cyan', 'green', 'amber', 'rose', 'violet'];
  const NODE_TYPES     = ['default', 'script'];
  const MACRO_ACTIONS  = ['run-script', 'run-uploaded-script', 'type-text', 'keyboard-shortcut', 'prompt-input', 'open-path', 'open-editor', 'open-browser', 'delay', 'open-application'];

  /* Appearance: runtime themes recolor the cyan accent system. Each theme maps
     to the CSS custom properties consumed across the board + settings UI. */
  const THEMES = {
    cyan:    { label: 'Cyan',    accent: '#5ee7f7', accent2: '#b09eff', glow: 'rgba(94,231,247,.22)' },
    violet:  { label: 'Violet',  accent: '#c4a8ff', accent2: '#6ee8ff', glow: 'rgba(196,168,255,.24)' },
    emerald: { label: 'Emerald', accent: '#6ee89a', accent2: '#5ee7f7', glow: 'rgba(110,232,154,.22)' },
    amber:   { label: 'Amber',   accent: '#fdd87a', accent2: '#ff9f7a', glow: 'rgba(253,216,122,.22)' },
    rose:    { label: 'Rose',    accent: '#ff8fa8', accent2: '#c4a8ff', glow: 'rgba(255,143,168,.24)' },
    mono:    { label: 'Mono',    accent: '#aebfce', accent2: '#8fa3b5', glow: 'rgba(174,191,206,.20)' }
  };
  const THEME_IDS = Object.keys(THEMES);
  const BOARD_BACKGROUNDS = [
    { id: 'dots',   label: 'Dots'   },
    { id: 'grid',   label: 'Grid'   },
    { id: 'aurora', label: 'Aurora' },
    { id: 'void',   label: 'Void'   }
  ];
  const BOARD_BACKGROUND_IDS = BOARD_BACKGROUNDS.map(b => b.id);
  const GROUP_NONE = '';
  const SHORTCUT_MODIFIER_ORDER = ['Ctrl', 'Alt', 'Shift', 'Meta'];
  const SHORTCUT_RECORD_IDLE_MS = 1400;
  const EMPTY_SET = new Set();
  const NODE_COLOR_MAP = {
    slate:  '#8fa3b5',
    cyan:   '#5ee7f7',
    green:  '#6ee89a',
    amber:  '#fdd87a',
    rose:   '#ff8fa8',
    violet: '#c4a8ff'
  };

  /* ─── State ──────────────────────────────────────────────────────── */
  let nodes       = [];
  let renderNodes = [];
  let links       = [];
  let viewBox     = '0 0 1 1';

  let workspaces        = [];
  let activeWorkspaceId = 'default';
  let workspaceName     = '';
  let commandHistoryCache = [];

  let statusText  = 'Loading…';
  let activityLog = [];
  let fatalError  = '';
  let currentWindowLabel = 'main';
  let isNodeBoardWindow  = false;

  // Two separate canvas refs: outer (logical size) and zoom-root (scaled)
  let canvasEl  = null;  // outer canvas div – used for viewBox + clamp
  let nodeLayer = null;  // zoom-root div inside canvas – used for drag math
  let nodeLayerResizeObserver = null;
  let nodeLayerResizeFrame    = null;
  const nodeElements = new Map();
  let renderQueued = false;

  let smoothNodes  = [];
  let smoothLookup = new Map();
  let springFrame  = null;

  let selectedIds       = new Set();
  let hoveredId         = null;
  let highlightNodeId   = null;
  let expandedNodeId    = null;
  let suppressExpandNodeId = null;

  let draggingId = null;
  let dragOffset = { x: 0, y: 0 };
  let dragStart  = { x: 0, y: 0 };
  let dragMoved  = false;
  let isPanning  = false;
  let panStartPointer = { x: 0, y: 0 };
  let panStartOffset  = { x: 0, y: 0 };
  let pendingPointer = null;
  let dragFrame      = null;

  let saveTimer = null;

  let contextMenu = { open: false, x: 0, y: 0, nodeId: null };
  let contextNode = null;

  let editPopup  = { open: false, nodeId: null };
  let editNode   = null;
  let editDraft  = createEditDraft();
  let editSelectedLinks = [];
  let editMacroSteps = [];
  let editScriptUpload = null;
  let recordingShortcutIndex = null;
  let shortcutRecordStopTimer = null;

  let showLauncher    = false;
  let launcherQuery   = '';
  let launcherIndex   = 0;
  let launcherResults = [];

  let zoomLevel = 1.0;
  let boardPan = { x: 0, y: 0 };
  const ZOOM_MIN  = 0.3;
  const ZOOM_MAX  = 3.0;
  const ZOOM_STEP = 0.12;
  const BOARD_OPACITY_MIN = 20;
  const BOARD_OPACITY_MAX = 100;
  let boardOpacity = BOARD_OPACITY_MAX;
  let appSettings = { start_on_boot: true, theme: 'cyan', board_background: 'dots' };
  let savingStartOnBoot = false;

  // Groups (per-workspace clusters). `groups` mirrors the active workspace's
  // group metadata the same way `nodes` mirrors its node list.
  let groups = [];

  // Drag-drop-to-create state.
  let isDropActive = false;
  let dropDepth = 0;

  // Macro prompt-input dialog state. `promptResolver` resolves the awaiting macro.
  let promptDialog = { open: false, label: '', value: '' };
  let promptResolver = null;
  let prefersReducedMotion = false;
  let reducedMotionQuery = null;
  let reducedMotionListener = null;
  let showcaseMode = false;
  let statusFlash = false;
  let statusVersion = 0;
  let statusFlashTimer = null;
  let workspaceSwitchTimer = null;
  let isSwitchingWorkspace = false;

  /* ─── Helpers ────────────────────────────────────────────────────── */
  function zoomIn()  { zoomLevel = Math.min(ZOOM_MAX, parseFloat((zoomLevel + ZOOM_STEP).toFixed(2))); queueRender(); }
  function zoomOut() { zoomLevel = Math.max(ZOOM_MIN, parseFloat((zoomLevel - ZOOM_STEP).toFixed(2))); queueRender(); }
  function setBoardOpacity(value) {
    boardOpacity = clamp(Math.round(Number(value) || BOARD_OPACITY_MAX), BOARD_OPACITY_MIN, BOARD_OPACITY_MAX);
  }

  async function loadAppSettings() {
    try {
      const settings = await invoke('get_app_settings');
      appSettings = {
        start_on_boot: Boolean(settings?.start_on_boot ?? true),
        theme: THEME_IDS.includes(settings?.theme) ? settings.theme : 'cyan',
        board_background: BOARD_BACKGROUND_IDS.includes(settings?.board_background) ? settings.board_background : 'dots'
      };
      applyAppearance();
    } catch (e) {
      updateStatus(`Failed to load app settings: ${String(e)}`);
    }
  }

  /* ─── Appearance / themes ────────────────────────────────────────── */
  function applyAppearance() {
    if (typeof document === 'undefined') return;
    const theme = THEMES[appSettings.theme] || THEMES.cyan;
    const root = document.documentElement.style;
    root.setProperty('--accent', theme.accent);
    root.setProperty('--accent2', theme.accent2);
    root.setProperty('--accent-glow', theme.glow);
  }
  async function persistUiPreferences() {
    try {
      await invoke('set_ui_preferences', {
        theme: appSettings.theme,
        boardBackground: appSettings.board_background
      });
    } catch (e) {
      updateStatus(`Appearance save failed: ${String(e)}`);
    }
  }
  function setTheme(themeId) {
    if (!THEME_IDS.includes(themeId)) return;
    appSettings = {...appSettings, theme:themeId};
    applyAppearance();
    void persistUiPreferences();
    updateStatus(`Theme: ${THEMES[themeId].label}`);
  }
  function cycleTheme() {
    const idx = THEME_IDS.indexOf(appSettings.theme);
    setTheme(THEME_IDS[(idx + 1) % THEME_IDS.length]);
  }
  function setBoardBackground(bgId) {
    if (!BOARD_BACKGROUND_IDS.includes(bgId)) return;
    appSettings = {...appSettings, board_background:bgId};
    void persistUiPreferences();
    updateStatus(`Board background: ${bgId}`);
  }

  async function updateStartOnBoot(enabled) {
    const previous = appSettings.start_on_boot;
    appSettings = {...appSettings, start_on_boot: enabled};
    savingStartOnBoot = true;
    try {
      await invoke('set_start_on_boot', { enabled });
      updateStatus(enabled ? 'Start on boot enabled' : 'Start on boot disabled');
    } catch (e) {
      appSettings = {...appSettings, start_on_boot: previous};
      updateStatus(`Startup setting failed: ${String(e)}`);
    } finally {
      savingStartOnBoot = false;
    }
  }

  async function minimizeWindow() {
    try {
      await invoke('hide_main_window');
      updateStatus('Settings hidden to tray');
    } catch (e) {
      updateStatus(String(e));
    }
  }

  async function exitApplication() {
    try {
      await invoke('exit_app');
    } catch (e) {
      updateStatus(String(e));
    }
  }

  function createEditDraft() {
    return {
      name:'', icon:'', description:'',
      path:'', editor:'', browser:'', script:'',
      color:'slate', node_type:'default', group:'',
      uploaded_script_path:'', uploaded_script_name:'',
      run_macro_on_system_start:false
    };
  }

  function uid(prefix) { return `${prefix}-${Math.random().toString(36).slice(2,8)}`; }
  function clamp(v, lo, hi) { return Math.max(lo, Math.min(hi, v)); }
  function normalizeOptionalString(v) {
    if (typeof v !== 'string') return null;
    const t = v.trim(); return t || null;
  }
  function normalizeNodeType(v) {
    return NODE_TYPES.includes(v) ? v : 'default';
  }
  function createMacroStep(action='run-script', value='') {
    return {action, value};
  }

  function sleep(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  function motionDuration(ms) {
    return prefersReducedMotion ? 1 : ms;
  }

  function scheduleStatusFlash() {
    statusFlash = false;

    if (statusFlashTimer !== null) {
      clearTimeout(statusFlashTimer);
      statusFlashTimer = null;
    }

    if (prefersReducedMotion) return;

    void tick().then(() => {
      statusFlash = true;
      statusFlashTimer = setTimeout(() => {
        statusFlash = false;
        statusFlashTimer = null;
      }, motionDuration(420));
    });
  }

  function normalizeShortcutKeyToken(rawToken) {
    if (typeof rawToken !== 'string') return null;
    const token = rawToken.trim();
    if (!token) return null;
    const lower = token.toLowerCase();

    if (lower === 'ctrl' || lower === 'control') return 'Ctrl';
    if (lower === 'alt' || lower === 'option') return 'Alt';
    if (lower === 'shift') return 'Shift';
    if (lower === 'meta' || lower === 'cmd' || lower === 'command' || lower === 'win' || lower === 'super' || lower === 'os') return 'Meta';
    if (lower === 'esc' || lower === 'escape') return 'Esc';
    if (lower === 'space' || lower === 'spacebar') return 'Space';
    if (lower === 'enter' || lower === 'return') return 'Enter';
    if (lower === 'tab') return 'Tab';
    if (lower === 'delete' || lower === 'del') return 'Delete';
    if (lower === 'backspace') return 'Backspace';
    if (lower === 'up' || lower === 'arrowup') return 'Up';
    if (lower === 'down' || lower === 'arrowdown') return 'Down';
    if (lower === 'left' || lower === 'arrowleft') return 'Left';
    if (lower === 'right' || lower === 'arrowright') return 'Right';
    if (/^f\d{1,2}$/i.test(token)) return token.toUpperCase();
    if (token.length === 1) return token.toUpperCase();
    return token.charAt(0).toUpperCase() + token.slice(1).toLowerCase();
  }

  function normalizeShortcutChord(rawChord) {
    if (typeof rawChord !== 'string') return '';
    const tokens = rawChord
      .split('+')
      .map(normalizeShortcutKeyToken)
      .filter(Boolean);

    if (!tokens.length) return '';

    const seen = new Set();
    const ordered = [];

    for (const modifier of SHORTCUT_MODIFIER_ORDER) {
      if (tokens.includes(modifier) && !seen.has(modifier)) {
        ordered.push(modifier);
        seen.add(modifier);
      }
    }

    for (const token of tokens) {
      if (SHORTCUT_MODIFIER_ORDER.includes(token)) continue;
      if (!seen.has(token)) {
        ordered.push(token);
        seen.add(token);
      }
    }

    const hasNonModifier = ordered.some(token => !SHORTCUT_MODIFIER_ORDER.includes(token));
    if (!hasNonModifier) return '';
    return ordered.join('+');
  }

  function normalizeShortcutSequence(rawValue) {
    if (typeof rawValue !== 'string') return '';
    return rawValue
      .split(',')
      .map(chunk => normalizeShortcutChord(chunk.trim()))
      .filter(Boolean)
      .join(', ');
  }

  function formatShortcutSequence(rawValue) {
    const normalized = normalizeShortcutSequence(rawValue);
    if (!normalized) return '';
    return normalized
      .split(',')
      .map(part => part.trim().split('+').join(' + '))
      .join(', ');
  }

  function shortcutChordFromEvent(event) {
    const tokens = [];
    if (event.ctrlKey) tokens.push('Ctrl');
    if (event.altKey) tokens.push('Alt');
    if (event.shiftKey) tokens.push('Shift');
    if (event.metaKey) tokens.push('Meta');

    const keyToken = normalizeShortcutKeyToken(event.key);
    if (keyToken && !SHORTCUT_MODIFIER_ORDER.includes(keyToken)) {
      tokens.push(keyToken);
    }

    return normalizeShortcutChord(tokens.join('+'));
  }

  function stopShortcutRecording() {
    recordingShortcutIndex = null;
    if (shortcutRecordStopTimer!==null) {
      clearTimeout(shortcutRecordStopTimer);
      shortcutRecordStopTimer = null;
    }
  }

  function queueShortcutRecordingStop() {
    if (shortcutRecordStopTimer!==null) clearTimeout(shortcutRecordStopTimer);
    shortcutRecordStopTimer = setTimeout(() => {
      shortcutRecordStopTimer = null;
      recordingShortcutIndex = null;
    }, SHORTCUT_RECORD_IDLE_MS);
  }

  function toggleShortcutRecording(index) {
    if (recordingShortcutIndex === index) {
      stopShortcutRecording();
      return;
    }
    recordingShortcutIndex = index;
    editMacroSteps = editMacroSteps.map((step, i) => i===index ? {...step, value:''} : step);
    queueShortcutRecordingStop();
  }

  function handleShortcutRecordingKeydown(event) {
    if (recordingShortcutIndex === null) return false;

    if (event.key === 'Escape') {
      event.preventDefault();
      stopShortcutRecording();
      return true;
    }

    const chord = shortcutChordFromEvent(event);
    if (!chord) return false;

    event.preventDefault();
    event.stopPropagation();

    const index = recordingShortcutIndex;
    editMacroSteps = editMacroSteps.map((step, i) => {
      if (i !== index) return step;
      const existing = normalizeShortcutSequence(step.value);
      const parts = existing
        ? existing.split(',').map(part => part.trim()).filter(Boolean)
        : [];
      parts.push(chord);
      return {...step, value: parts.join(', ')};
    });

    queueShortcutRecordingStop();
    return true;
  }

  function normalizeMacroStep(raw) {
    const action = MACRO_ACTIONS.includes(raw?.action) ? raw.action : 'run-script';
    const rawValue = typeof raw?.value === 'string' ? raw.value : '';
    const value = action === 'keyboard-shortcut' ? normalizeShortcutSequence(rawValue) : rawValue;
    return {action, value};
  }
  function macroActionNeedsValue(action) {
    return action !== 'delay' && action !== 'run-uploaded-script' && action !== 'prompt-input';
  }
  function normalizeMacroSteps(steps) {
    return (Array.isArray(steps) ? steps : [])
      .map(normalizeMacroStep)
      .filter(step => macroActionNeedsValue(step.action) ? step.value.trim().length > 0 : true);
  }
  function macroActionLabel(action) {
    if (action === 'run-script') return 'Run script';
    if (action === 'run-uploaded-script') return 'Run uploaded script';
    if (action === 'type-text') return 'Type text';
    if (action === 'keyboard-shortcut') return 'Keyboard shortcut';
    if (action === 'prompt-input') return 'Prompt for input';
    if (action === 'open-path') return 'Open path';
    if (action === 'open-editor') return 'Open editor';
    if (action === 'open-browser') return 'Open browser';
    if (action === 'delay') return 'Delay (ms)';
    if (action === 'open-application') return 'Open application';
    return action;
  }
  function macroValuePlaceholder(action) {
    if (action === 'run-script') return 'npm run dev';
    if (action === 'run-uploaded-script') return 'Uses this node uploaded script file';
    if (action === 'type-text') return 'hello world';
    if (action === 'keyboard-shortcut') return 'Ctrl+Shift+P, Ctrl+K';
    if (action === 'prompt-input') return 'Label, e.g. Branch name';
    if (action === 'open-path') return '/path/to/project';
    if (action === 'open-editor') return 'code /path/to/project';
    if (action === 'open-browser') return 'https://example.com';
    if (action === 'delay') return '1000';
    if (action === 'open-application') return 'obsidian';
    return 'Value';
  }
  function isScriptNode(node) {
    return normalizeNodeType(node?.node_type) === 'script';
  }
  function nodeScriptTarget(node) {
    return normalizeOptionalString(node?.uploaded_script_path)
      || normalizeOptionalString(node?.targets?.script);
  }
  function isLockedNode(n) {
    const id = typeof n === 'string' ? n : n?.id;
    return id === MAIN_NODE_ID || Boolean(n?.locked);
  }
  function isLogoIcon(icon)  { return icon === LOGO_ICON || icon === 'fin'; }
  function isImageIcon(icon) { return typeof icon === 'string' && icon.startsWith('data:image/'); }
  function nodeColor(node)   { return NODE_COLOR_MAP[node?.color] || NODE_COLOR_MAP.slate; }

  /* ─── Node factories / normalizers ──────────────────────────────── */
  function createMainNode(anchor) {
    let x = 30, y = 30;
    if (anchor) { x = Math.max(20, Number(anchor.x)-130); y = Math.max(20, Number(anchor.y)-30); }
    return { id:MAIN_NODE_ID, name:MAIN_NODE_NAME, icon:LOGO_ICON, description:'Core entry node', x, y,
             links:[], targets:{path:null,editor:null,browser:null,script:null},
             node_type:'default', uploaded_script_path:null, uploaded_script_name:null,
             run_macro_on_system_start:false, group:GROUP_NONE,
             color:'cyan', macros:[], locked:true, last_launched:null, launch_count:0 };
  }
  function createEmptyNode(x, y) {
    return { id:uid('node'), name:'', icon:'', description:'', x, y,
             links:[], targets:{path:null,editor:null,browser:null,script:null},
             node_type:'default', uploaded_script_path:null, uploaded_script_name:null,
             run_macro_on_system_start:false, group:GROUP_NONE,
             color:'slate', macros:[], locked:false, last_launched:null, launch_count:0 };
  }
  function normalizeNode(raw, index=0) {
    const t = raw?.targets ?? {};
    const nodeId = typeof raw?.id==='string' && raw.id ? raw.id : uid('node');
    return {
      id:          nodeId,
      name:        typeof raw?.name==='string' ? raw.name : '',
      icon:        typeof raw?.icon==='string' ? raw.icon : '',
      description: typeof raw?.description==='string' ? raw.description : '',
      x:           Number.isFinite(Number(raw?.x)) ? Number(raw.x) : 40+(index%4)*140,
      y:           Number.isFinite(Number(raw?.y)) ? Number(raw.y) : 40+Math.floor(index/4)*140,
      links:       Array.isArray(raw?.links) ? raw.links.filter(id=>typeof id==='string') : [],
      targets:     { path:normalizeOptionalString(t.path), editor:normalizeOptionalString(t.editor),
                     browser:normalizeOptionalString(t.browser), script:normalizeOptionalString(t.script) },
      node_type:   normalizeNodeType(raw?.node_type),
      uploaded_script_path: normalizeOptionalString(raw?.uploaded_script_path),
      uploaded_script_name: normalizeOptionalString(raw?.uploaded_script_name),
      run_macro_on_system_start: nodeId===MAIN_NODE_ID ? false : Boolean(raw?.run_macro_on_system_start),
      group:       typeof raw?.group==='string' ? raw.group : GROUP_NONE,
      color:       NODE_COLORS.includes(raw?.color) ? raw.color : 'slate',
      macros:      normalizeMacroSteps(raw?.macros),
      locked:      Boolean(raw?.locked),
      last_launched: typeof raw?.last_launched==='string' ? raw.last_launched : null,
      launch_count: Number.isFinite(Number(raw?.launch_count)) ? Math.max(0, Math.floor(Number(raw.launch_count))) : 0
    };
  }
  function ensureMainNode(list) {
    let changed=false, hasMain=false;
    const normalized = list.map(node => {
      if (node.id !== MAIN_NODE_ID) return node;
      hasMain = true;
      const forced = {...node, name:MAIN_NODE_NAME, icon:LOGO_ICON, locked:true, run_macro_on_system_start:false};
      if (forced.name!==node.name||forced.icon!==node.icon||!node.locked||Boolean(node.run_macro_on_system_start)) changed=true;
      return forced;
    });
    if (hasMain) return {nodes:normalized, changed};
    return {nodes:[createMainNode(normalized[0]),...normalized], changed:true};
  }
  function createDefaultWorkspace() {
    return { id:'default', name:'Default', nodes:[createMainNode(null)], zoom:1, pan_x:0, pan_y:0, groups:[] };
  }
  function normalizeGroup(raw) {
    const name = typeof raw?.name==='string' ? raw.name.trim() : '';
    const id   = typeof raw?.id==='string' && raw.id ? raw.id : (name || uid('grp'));
    return {
      id,
      name: name || id,
      color: NODE_COLORS.includes(raw?.color) ? raw.color : 'cyan',
      collapsed: Boolean(raw?.collapsed)
    };
  }
  function normalizeGroups(rawGroups, nodeList) {
    const used = new Set((nodeList||[]).map(n=>n.group).filter(Boolean));
    const seen = new Set();
    const list = (Array.isArray(rawGroups) ? rawGroups : [])
      .map(normalizeGroup)
      .filter(g => { if (seen.has(g.id)) return false; seen.add(g.id); return true; });
    // Auto-create group metadata for any group referenced by a node but missing.
    for (const id of used) {
      if (!seen.has(id)) { list.push(normalizeGroup({id, name:id})); seen.add(id); }
    }
    // Drop empty groups that no node references.
    return list.filter(g => used.has(g.id));
  }
  function normalizeWorkspace(raw, index) {
    const id   = typeof raw?.id==='string' && raw.id ? raw.id : `ws-${index+1}`;
    const name = typeof raw?.name==='string' && raw.name.trim() ? raw.name.trim() : `Workspace ${index+1}`;
    const nodeList = Array.isArray(raw?.nodes) ? raw.nodes.map((n,i)=>normalizeNode(n,i)) : [];
    const ensured  = ensureMainNode(nodeList);
    return { id, name, nodes:ensured.nodes, zoom:1, pan_x:0, pan_y:0, groups:normalizeGroups(raw?.groups, ensured.nodes) };
  }
  function normalizeWorkspaces(rawList) {
    if (!Array.isArray(rawList)||rawList.length===0) return [createDefaultWorkspace()];
    const ids = new Set();
    const normalized = rawList.map((raw,i) => {
      const ws = normalizeWorkspace(raw,i);
      if (ids.has(ws.id)) ws.id = uid('ws');
      ids.add(ws.id);
      return ws;
    });
    return normalized.length ? normalized : [createDefaultWorkspace()];
  }

  /* ─── Activity / status ──────────────────────────────────────────── */
  function recordActivity(message) {
    const stamp = new Date().toLocaleTimeString();
    activityLog = [{id:`${Date.now()}-${Math.random().toString(36).slice(2,7)}`, text:`${stamp} ${message}`}, ...activityLog].slice(0,8);
  }
  function updateStatus(message) {
    statusText = message;
    statusVersion += 1;
    scheduleStatusFlash();
    recordActivity(message);
  }

  function setupReducedMotionListener() {
    if (typeof window==='undefined' || typeof window.matchMedia!=='function') return () => {};

    reducedMotionQuery = window.matchMedia('(prefers-reduced-motion: reduce)');
    prefersReducedMotion = reducedMotionQuery.matches;
    reducedMotionListener = event => {
      prefersReducedMotion = Boolean(event?.matches);
    };

    if (typeof reducedMotionQuery.addEventListener === 'function') {
      reducedMotionQuery.addEventListener('change', reducedMotionListener);
    } else if (typeof reducedMotionQuery.addListener === 'function') {
      reducedMotionQuery.addListener(reducedMotionListener);
    }

    return () => {
      if (!reducedMotionQuery || !reducedMotionListener) return;
      if (typeof reducedMotionQuery.removeEventListener === 'function') {
        reducedMotionQuery.removeEventListener('change', reducedMotionListener);
      } else if (typeof reducedMotionQuery.removeListener === 'function') {
        reducedMotionQuery.removeListener(reducedMotionListener);
      }
      reducedMotionQuery = null;
      reducedMotionListener = null;
    };
  }

  /* ─── Workspace CRUD ─────────────────────────────────────────────── */
  function updateActiveWorkspaceNodes(nextNodes) {
    workspaces = workspaces.map(ws => ws.id!==activeWorkspaceId ? ws : {...ws, nodes:nextNodes, groups});
  }
  async function persistLayout() {
    groups = normalizeGroups(groups, nodes);
    updateActiveWorkspaceNodes(nodes);
    await invoke('save_layout', {layout:{active_workspace:activeWorkspaceId, workspaces, command_history:commandHistoryCache}});
  }
  async function flushPendingSave() {
    if (saveTimer===null) return;
    clearTimeout(saveTimer); saveTimer=null;
    await persistLayout();
  }
  function scheduleSave() {
    if (saveTimer!==null) clearTimeout(saveTimer);
    saveTimer = setTimeout(async () => {
      saveTimer = null;
      try { await persistLayout(); } catch(e) { updateStatus(String(e)); }
    }, SAVE_DEBOUNCE_MS);
  }
  function applyLayout(layout) {
    commandHistoryCache = Array.isArray(layout?.command_history) ? layout.command_history : [];
    workspaces = normalizeWorkspaces(layout?.workspaces ?? []);
    let nextActive = typeof layout?.active_workspace==='string' ? layout.active_workspace : workspaces[0].id;
    if (!workspaces.some(ws=>ws.id===nextActive)) nextActive = workspaces[0].id;
    activeWorkspaceId = nextActive;
    const ws = workspaces.find(ws=>ws.id===activeWorkspaceId) ?? workspaces[0];
    nodes = ws.nodes;
    groups = normalizeGroups(ws.groups, ws.nodes);
    syncSmooth(true);
    void tick().then(()=>{ clampAllNodesToCanvas(false); queueRender(); });
  }
  async function loadWorkspaces() {
    try { const layout = await invoke('load_layout'); applyLayout(layout); updateStatus(`${nodes.length} nodes loaded`); }
    catch(e) { updateStatus(String(e)); }
  }
  async function switchWorkspace(wsId) {
    if (!wsId||wsId===activeWorkspaceId) return;

    isSwitchingWorkspace = true;
    if (workspaceSwitchTimer!==null) {
      clearTimeout(workspaceSwitchTimer);
      workspaceSwitchTimer = null;
    }

    try {
      await flushPendingSave();
      const layout = await invoke('switch_workspace', {workspaceId:wsId});
      applyLayout(layout);
      const ws = workspaces.find(ws=>ws.id===activeWorkspaceId);
      updateStatus(`Switched to ${ws?.name??'workspace'}`);
    } catch(e) { updateStatus(String(e)); }
    finally {
      workspaceSwitchTimer = setTimeout(() => {
        isSwitchingWorkspace = false;
        workspaceSwitchTimer = null;
      }, motionDuration(190));
    }
  }
  async function createWorkspace() {
    const name = workspaceName.trim(); if (!name) return;
    try {
      await flushPendingSave();
      const created = await invoke('create_workspace', {name});
      workspaceName = '';
      await loadWorkspaces();
      if (created&&typeof created.id==='string') await switchWorkspace(created.id);
      updateStatus('Workspace created');
    } catch(e) { updateStatus(String(e)); }
  }
  async function deleteWorkspace(wsId) {
    if (!wsId||workspaces.length<=1) return;
    try {
      await flushPendingSave();
      await invoke('delete_workspace', {workspaceId:wsId});
      await loadWorkspaces();
      updateStatus('Workspace deleted');
    } catch(e) { updateStatus(String(e)); }
  }

  /* ─── Import / Export ────────────────────────────────────────────── */
  async function exportWorkspace() {
    try {
      await flushPendingSave();
      const path = await invoke('export_workspace', {workspaceId:activeWorkspaceId});
      updateStatus(`Exported → ${path}`);
    } catch(e) { updateStatus(`Export failed: ${String(e)}`); }
  }
  function triggerImportWorkspace() {
    document.getElementById('workspace-import-input')?.click();
  }
  async function handleWorkspaceImport(event) {
    const input = event.currentTarget;
    const file = input.files?.[0];
    input.value = '';
    if (!file) return;
    try {
      const text = await file.text();
      const parsed = JSON.parse(text);
      // Accept either the export envelope { workspace: {...} } or a bare workspace.
      const rawWs = parsed?.workspace ?? parsed;
      if (!rawWs || !Array.isArray(rawWs.nodes)) {
        updateStatus('Import failed: not a FinNode workspace');
        return;
      }
      await flushPendingSave();
      const baseName = (typeof rawWs.name==='string' && rawWs.name.trim()) ? rawWs.name.trim() : 'Imported';
      const existingNames = new Set(workspaces.map(w=>w.name));
      let name = baseName, n = 2;
      while (existingNames.has(name)) name = `${baseName} (${n++})`;
      const imported = normalizeWorkspace({...rawWs, id:uid('ws'), name}, workspaces.length);
      workspaces = [...workspaces, imported];
      await invoke('save_layout', {layout:{active_workspace:activeWorkspaceId, workspaces, command_history:commandHistoryCache}});
      await switchWorkspace(imported.id);
      updateStatus(`Imported "${name}" (${imported.nodes.length} nodes)`);
    } catch(e) {
      updateStatus(`Import failed: ${String(e)}`);
    }
  }

  /* ─── Macro variables ────────────────────────────────────────────── */
  // Tokens usable in macro step values. Resolved on the frontend (where the
  // clipboard + a prompt dialog are available) before steps are sent to Rust.
  function substituteTokens(value, ctx) {
    if (typeof value !== 'string' || value.indexOf('{{') === -1) return value;
    return value.replace(/\{\{\s*([a-zA-Z]+)(?::([^}]*))?\s*\}\}/g, (match, name, arg) => {
      const key = name.toLowerCase();
      if (key === 'clipboard') return ctx.clipboard;
      if (key === 'date') return ctx.date;
      if (key === 'time') return ctx.time;
      if (key === 'datetime') return ctx.datetime;
      if (key === 'input') {
        if (arg !== undefined) return ctx.inputs[arg.trim().toLowerCase()] ?? '';
        return ctx.lastInput;
      }
      return match;
    });
  }
  function macroUsesToken(steps, token) {
    return steps.some(s => typeof s.value === 'string' && s.value.toLowerCase().includes(token));
  }
  // Resolve prompt-input steps + token substitution. Returns the executable
  // step list, or null if the user cancelled a prompt (aborting the macro).
  async function resolveMacroSteps(steps) {
    const ctx = { date:'', time:'', datetime:'', clipboard:'', inputs:{}, lastInput:'' };
    const now = new Date();
    ctx.date = now.toLocaleDateString();
    ctx.time = now.toLocaleTimeString();
    ctx.datetime = now.toLocaleString();
    if (macroUsesToken(steps, '{{clipboard}}')) {
      try { ctx.clipboard = (await readClipboardText()) ?? ''; }
      catch (_) { ctx.clipboard = ''; }
    }
    const resolved = [];
    for (const step of steps) {
      if (step.action === 'prompt-input') {
        const label = (step.value || '').trim() || 'Enter value';
        const answer = await openPromptDialog(label);
        if (answer === null) return null; // cancelled → abort macro
        ctx.lastInput = answer;
        ctx.inputs[label.toLowerCase()] = answer;
        continue; // prompt steps are frontend-only
      }
      resolved.push({ ...step, value: substituteTokens(step.value, ctx) });
    }
    return resolved;
  }
  function openPromptDialog(label) {
    return new Promise(resolve => {
      promptResolver = resolve;
      promptDialog = { open:true, label, value:'' };
      void tick().then(() => document.getElementById('macro-prompt-input')?.focus());
    });
  }
  function resolvePromptDialog(value) {
    const resolver = promptResolver;
    promptResolver = null;
    promptDialog = { open:false, label:'', value:'' };
    if (resolver) resolver(value);
  }
  function confirmPromptDialog() { resolvePromptDialog(promptDialog.value); }
  function cancelPromptDialog()  { resolvePromptDialog(null); }
  function promptDialogKey(event) {
    if (event.key === 'Enter')  { event.preventDefault(); confirmPromptDialog(); }
    if (event.key === 'Escape') { event.preventDefault(); cancelPromptDialog(); }
  }

  /* ─── Node actions ───────────────────────────────────────────────── */
  function registerLaunch(nodeId) {
    nodes = nodes.map(n => n.id!==nodeId ? n : {
      ...n,
      last_launched: new Date().toISOString(),
      launch_count: (Number(n.launch_count) || 0) + 1
    });
    queueRender();
    scheduleSave();
  }
  async function launchNode(node, action) {
    try {
      if (action === 'run-macro') {
        const steps = normalizeMacroSteps(node?.macros);
        if (!steps.length) {
          updateStatus('No macro steps configured');
          return;
        }

        // Collect any prompt input + resolve tokens while the window is visible.
        const resolvedSteps = await resolveMacroSteps(steps);
        if (resolvedSteps === null) { updateStatus('Macro cancelled'); return; }
        if (!resolvedSteps.length)  { updateStatus('Macro had no runnable steps'); return; }

        if (!isNodeBoardWindow && resolvedSteps.some(step => step.action==='type-text' || step.action==='keyboard-shortcut')) {
          try {
            await invoke('hide_main_window');
            await sleep(140);
          } catch (_) {}
        }

        await invoke('run_node_macro', {
          steps: resolvedSteps,
          uploadedScriptPath: normalizeOptionalString(node?.uploaded_script_path),
          workingDir: normalizeOptionalString(node?.targets?.path)
        });
      } else {
        await invoke('launch_node', {node, action});
      }
      registerLaunch(node.id);
      updateStatus(`${action} → ${node.name||'node'}`);
    } catch(e) { updateStatus(String(e)); }
  }
  async function openSettingsFromMainNode() {
    try { await invoke('show_settings_view'); updateStatus('Opened settings'); }
    catch(e) { updateStatus(`Settings unavailable`); }
  }

  function addNode() {
    const offset = nodes.length * 14;
    const next   = createEmptyNode(50+offset, 70+offset);
    nodes = [...nodes, next];
    syncSmooth(true);
    scheduleSave();
    void tick().then(()=>{ clampAllNodesToCanvas(true); queueRender(); });
    updateStatus('Node added');
  }
  function cloneNode(nodeId) {
    if (isLockedNode(nodeId)) return;
    const src = nodes.find(n=>n.id===nodeId); if (!src) return;
    const clone = {...src, id:uid('node'), name:src.name?`${src.name} copy`:'Node copy', x:src.x+28, y:src.y+28,
                   links:[...(src.links??[])], targets:{...(src.targets??{})},
                   macros:normalizeMacroSteps(src.macros), run_macro_on_system_start:false, locked:false};
    nodes = [...nodes, clone];
    syncSmooth(true); scheduleSave();
    void tick().then(()=>{ clampAllNodesToCanvas(true); queueRender(); });
    updateStatus('Node cloned');
  }
  function deleteNode(nodeId) {
    if (isLockedNode(nodeId)) { updateStatus('Main node is locked'); return; }
    if (contextMenu.nodeId===nodeId) closeCtx();
    if (editPopup.nodeId===nodeId) closeEditor();
    nodes = nodes.filter(n=>n.id!==nodeId).map(n=>({...n, links:(n.links??[]).filter(id=>id!==nodeId)}));
    syncSmooth(true); scheduleSave();
    updateStatus('Node deleted');
  }
  function layoutGrid() {
    const cols = Math.max(1, Math.ceil(Math.sqrt(nodes.length)));
    const gapX=148, gapY=140;
    nodes = nodes.map((n,i)=>({...n, x:40+(i%cols)*gapX, y:40+Math.floor(i/cols)*gapY}));
    syncSmooth(true); scheduleSave();
    void tick().then(()=>{ clampAllNodesToCanvas(true); queueRender(); });
    updateStatus('Grid layout applied');
  }

  /* ─── Selection ──────────────────────────────────────────────────── */
  function toggleSelect(nodeId, event) {
    if (!event.ctrlKey&&!event.metaKey) return false;
    const next = new Set(selectedIds);
    next.has(nodeId) ? next.delete(nodeId) : next.add(nodeId);
    selectedIds = next; return true;
  }
  function batchLaunch() {
    for (const id of selectedIds) { const n=nodes.find(n=>n.id===id); if(n) void launchNode(n,'open-path'); }
    selectedIds = new Set();
  }
  function batchDelete() {
    [...selectedIds].forEach(deleteNode); selectedIds = new Set();
  }

  /* ─── Quick grouping ─────────────────────────────────────────────── */
  // Assign a set of node ids to a group (creating the group if new). Passing an
  // empty name ungroups them. Main node is never grouped.
  function assignNodesToGroup(ids, rawName) {
    const idSet = ids instanceof Set ? ids : new Set(ids);
    const name = (rawName || '').trim();
    let count = 0;
    nodes = nodes.map(n => {
      if (!idSet.has(n.id) || n.id === MAIN_NODE_ID) return n;
      count++;
      return {...n, group: name};
    });
    if (name && !groups.some(g => g.id === name)) {
      const palette = NODE_COLORS.filter(c => c !== 'slate');
      const color = palette[groups.length % palette.length] || 'cyan';
      groups = [...groups, normalizeGroup({id: name, name, color})];
    }
    groups = normalizeGroups(groups, nodes);
    syncSmooth(true); scheduleSave(); queueRender();
    return { count, name };
  }
  async function groupSelectedNodes() {
    if (selectedIds.size === 0) return;
    const ids = new Set(selectedIds);
    const name = await openPromptDialog('Name this group');
    if (name === null) return;
    const { count } = assignNodesToGroup(ids, name);
    selectedIds = new Set();
    updateStatus(name.trim() ? `Grouped ${count} node${count===1?'':'s'} into "${name.trim()}"` : 'Removed from group');
  }
  async function groupSingleNode(nodeId) {
    if (!nodeId || nodeId === MAIN_NODE_ID) return;
    const node = nodes.find(n => n.id === nodeId);
    const current = node?.group || '';
    const name = await openPromptDialog(current ? `Move "${node?.name||'node'}" to group` : `Add "${node?.name||'node'}" to a group`);
    if (name === null) return;
    assignNodesToGroup([nodeId], name);
    updateStatus(name.trim() ? `Added to "${name.trim()}"` : 'Removed from group');
  }

  /* ─── Drag ───────────────────────────────────────────────────────── */
  // Positions are in LOGICAL (pre-zoom) space.
  // nodeLayer is the zoom-root (scaled), canvasEl is unscaled outer container.
  function clampNodePosition(node, x, y) {
    if (!canvasEl) return {x, y};
    const el = nodeElements.get(node.id);
    const w  = el ? el.offsetWidth  : NODE_SIZE;
    const h  = el ? el.offsetHeight : NODE_SIZE;

    // When zoomed out, expand logical bounds and add travel padding so
    // nodes can move beyond the original 100% viewport area.
    const zoomForBounds = zoomLevel < 1 ? zoomLevel : 1;
    const logicalWidth = canvasEl.offsetWidth / zoomForBounds;
    const logicalHeight = canvasEl.offsetHeight / zoomForBounds;
    const extraTravelX = zoomLevel < 1 ? canvasEl.offsetWidth * ((1 / zoomLevel) - 1) : 0;
    const extraTravelY = zoomLevel < 1 ? canvasEl.offsetHeight * ((1 / zoomLevel) - 1) : 0;
    const minX = -extraTravelX;
    const minY = -extraTravelY;
    const maxX = Math.max(minX, logicalWidth + extraTravelX - w);
    const maxY = Math.max(minY, logicalHeight + extraTravelY - h);

    return {
      x: clamp(x, minX, maxX),
      y: clamp(y, minY, maxY)
    };
  }
  function clampAllNodesToCanvas(save=false) {
    if (!canvasEl||nodes.length===0) return;
    let changed=false;
    const next = nodes.map(n=>{
      const c = clampNodePosition(n, n.x, n.y);
      if (c.x===n.x&&c.y===n.y) return n;
      changed=true; return {...n, x:c.x, y:c.y};
    });
    if (!changed) return;
    nodes=next; syncSmooth(true); queueRender();
    if (save) scheduleSave();
  }
  function beginDrag(event, nodeId) {
    if (event.button!==0) return;
    if (event.target instanceof Element && event.target.closest('button,input,textarea,select,label,a')) return;
    if (toggleSelect(nodeId, event)) return;
    const node = nodes.find(n=>n.id===nodeId); if (!node) return;
    closeCtx();
    draggingId=nodeId; dragMoved=false;
    dragStart = {x:event.clientX, y:event.clientY};
    // dragOffset in logical space
    const rect = event.currentTarget.getBoundingClientRect();
    dragOffset = {
      x: (event.clientX - rect.left) / zoomLevel,
      y: (event.clientY - rect.top)  / zoomLevel
    };
    event.preventDefault();
  }
  function onPointerMove(event) {
    if (isPanning) {
      const dx = event.clientX - panStartPointer.x;
      const dy = event.clientY - panStartPointer.y;
      boardPan = {x: panStartOffset.x + dx, y: panStartOffset.y + dy};
      queueRender();
      return;
    }

    if (!draggingId||!nodeLayer) return;
    if (!dragMoved) dragMoved = Math.abs(event.clientX-dragStart.x)>4||Math.abs(event.clientY-dragStart.y)>4;
    pendingPointer = {x:event.clientX, y:event.clientY};
    if (dragFrame!==null) return;
    dragFrame = requestAnimationFrame(()=>{
      dragFrame=null;
      if (!pendingPointer||!draggingId||!nodeLayer) return;
      const ptr=pendingPointer; pendingPointer=null;
      const node=nodes.find(n=>n.id===draggingId); if (!node) return;
      const lr = nodeLayer.getBoundingClientRect(); // visual (scaled) rect
      const lx = (ptr.x - lr.left) / zoomLevel - dragOffset.x;
      const ly = (ptr.y - lr.top)  / zoomLevel - dragOffset.y;
      const next = clampNodePosition(node, lx, ly);
      node.x=next.x; node.y=next.y; nodes=[...nodes];
      queueRender();
    });
  }

  function beginCanvasPan(event) {
    if (event.button!==1) return;
    if (event.target instanceof Element && event.target.closest('button,input,textarea,select,label,a')) return;
    isPanning = true;
    panStartPointer = {x:event.clientX, y:event.clientY};
    panStartOffset  = {x:boardPan.x, y:boardPan.y};
    event.preventDefault();
  }

  function preventMiddleAuxClick(event) {
    if (event.button===1) event.preventDefault();
  }

  function onPointerUp() {
    if (isPanning) {
      isPanning = false;
      return;
    }

    if (!draggingId) return;
    const releasedId=draggingId;
    if (dragFrame!==null) { cancelAnimationFrame(dragFrame); dragFrame=null; }
    pendingPointer=null;
    const moved=dragMoved; draggingId=null; dragMoved=false;
    if (moved) {
      syncSmooth(true); scheduleSave();
      suppressExpandNodeId=releasedId;
      setTimeout(()=>{ if(suppressExpandNodeId===releasedId) suppressExpandNodeId=null; },0);
    }
  }

  /* ─── Click / expand ─────────────────────────────────────────────── */
  function onNodeClick(event, nodeId) {
    if (event.button!==0) return;
    if (suppressExpandNodeId===nodeId) { suppressExpandNodeId=null; return; }
    if (nodeId===MAIN_NODE_ID) { expandedNodeId=null; return; }
    expandedNodeId = expandedNodeId===nodeId ? null : nodeId;
  }
  function onNodeKeydown(event, nodeId) {
    if (event.key!=='Enter'&&event.key!==' ') return;
    event.preventDefault();
    if (nodeId===MAIN_NODE_ID) { expandedNodeId=null; void openSettingsFromMainNode(); return; }
    expandedNodeId = expandedNodeId===nodeId ? null : nodeId;
  }
  function onNodeDoubleClick(node) {
    if (node.id===MAIN_NODE_ID) { expandedNodeId=null; void openSettingsFromMainNode(); return; }
    if (isScriptNode(node) && hasLaunchTarget(node,'run-script')) {
      void launchNode(node,'run-script');
    }
  }

  /* ─── Node element registry ──────────────────────────────────────── */
  function nodeRef(element, id) {
    nodeElements.set(id, element);
    queueRender();
    return { destroy() { nodeElements.delete(id); queueRender(); } };
  }

  /* ─── Link rendering (logical coordinate space) ──────────────────── */
  function queueRender() {
    if (renderQueued) return;
    renderQueued=true;
    void tick().then(()=>{ renderQueued=false; renderConnections(); });
  }
  function renderConnections() {
    if (!canvasEl) return;
    const cw = canvasEl.offsetWidth;
    const ch = canvasEl.offsetHeight;
    if (!cw||!ch) return;
    viewBox = `0 0 ${cw} ${ch}`;
    const next=[];
    const hidden = hiddenNodeIds || EMPTY_SET;
    for (const node of renderNodes) {
      if (hidden.has(node.id)) continue;
      for (const targetId of (node.links??[])) {
        if (hidden.has(targetId)) continue;
        const toNode = renderNodes.find(n=>n.id===targetId);
        if (!toNode) continue;
        const fromEl = nodeElements.get(node.id);
        const toEl   = nodeElements.get(targetId);
        const fw = fromEl ? fromEl.offsetWidth  : NODE_SIZE;
        const fh = fromEl ? fromEl.offsetHeight : NODE_SIZE;
        const tw = toEl   ? toEl.offsetWidth    : NODE_SIZE;
        const th = toEl   ? toEl.offsetHeight   : NODE_SIZE;
        const fx = node.renderX + fw/2;
        const fy = node.renderY + fh/2;
        const tx = toNode.renderX + tw/2;
        const ty = toNode.renderY + th/2;
        const dx=tx-fx, dy=ty-fy;
        const dist=Math.hypot(dx,dy)||1;
        const ux=dx/dist, uy=dy/dist;
        const fr=Math.min(fw,fh)/2-6;
        const tr=Math.min(tw,th)/2-6;
        const sx=fx+ux*fr, sy=fy+uy*fr;
        const ex=tx-ux*tr, ey=ty-uy*tr;
        const bend=Math.min(100,dist*0.3);
        next.push({from:node.id, to:targetId,
          d:`M ${sx} ${sy} C ${sx+ux*bend} ${sy+uy*bend}, ${ex-ux*bend} ${ey-uy*bend}, ${ex} ${ey}`});
      }
    }
    links=next;
  }

  /* ─── Spring animation ───────────────────────────────────────────── */
  function syncSmooth(immediate=false) {
    const prev = new Map(smoothNodes.map(n=>[n.id,n]));
    smoothNodes = nodes.map(n=>{
      const e=prev.get(n.id);
      if (!e||immediate) return {id:n.id, x:n.x, y:n.y, vx:0, vy:0};
      return e;
    });
    startSpring();
  }
  function startSpring() {
    if (springFrame!==null) return;
    springFrame=requestAnimationFrame(stepSpring);
  }
  function stepSpring() {
    springFrame=null;
    if (!smoothNodes.length) return;
    const targetById=new Map(nodes.map(n=>[n.id,n]));
    let active=false;
    smoothNodes = smoothNodes.map(item=>{
      const target=targetById.get(item.id); if (!target) return item;
      if (draggingId===item.id) return {...item, x:target.x, y:target.y, vx:0, vy:0};
      const dx=target.x-item.x, dy=target.y-item.y;
      const vx=(item.vx+dx*SPRING_STIFFNESS)*SPRING_DAMPING;
      const vy=(item.vy+dy*SPRING_STIFFNESS)*SPRING_DAMPING;
      if (Math.abs(dx)>0.1||Math.abs(dy)>0.1||Math.abs(vx)>0.1||Math.abs(vy)>0.1) active=true;
      return {...item, x:item.x+vx, y:item.y+vy, vx, vy};
    });
    queueRender();
    if (active) springFrame=requestAnimationFrame(stepSpring);
  }

  /* ─── Editor modal ───────────────────────────────────────────────── */
  function closeCtx()    { if (!contextMenu.open) return; contextMenu={open:false,x:0,y:0,nodeId:null}; highlightNodeId=hoveredId; }
  function closeEditor() {
    if (!editPopup.open) return;
    stopShortcutRecording();
    editPopup={open:false,nodeId:null};
    editDraft=createEditDraft();
    editSelectedLinks=[];
    editMacroSteps=[];
    editScriptUpload=null;
  }

  function openEditor(nodeId) {
    if (isLockedNode(nodeId)) return;
    const node=nodes.find(n=>n.id===nodeId); if (!node) return;
    editDraft = {name:node.name??'', icon:node.icon??'', description:node.description??'',
                 path:node.targets?.path??'', editor:node.targets?.editor??'',
                 browser:node.targets?.browser??'', script:node.targets?.script??'', color:node.color??'slate',
                 node_type:normalizeNodeType(node.node_type),
                 group:node.group??'',
                 uploaded_script_path:node.uploaded_script_path??'',
                 uploaded_script_name:node.uploaded_script_name??'',
                 run_macro_on_system_start:Boolean(node.run_macro_on_system_start)};
    editSelectedLinks = [...(node.links??[])];
    editMacroSteps = normalizeMacroSteps(node.macros);
    editScriptUpload = null;
    editPopup = {open:true, nodeId};
  }
  function clearUploadedNodeIcon()  { editDraft={...editDraft, icon:''}; }
  function handleNodeIconUpload(event) {
    const input=event.currentTarget;
    const file=input.files?.[0]; if (!file) return;
    if (!file.type.startsWith('image/')) { updateStatus('Choose an image file'); input.value=''; return; }
    if (file.size>MAX_NODE_ICON_BYTES) { updateStatus('Icon must be ≤ 512 KB'); input.value=''; return; }
    const reader=new FileReader();
    reader.onload=()=>{ if (typeof reader.result==='string') editDraft={...editDraft, icon:reader.result}; updateStatus('Icon uploaded'); };
    reader.readAsDataURL(file); input.value='';
  }
  function handleNodeScriptUpload(event) {
    const input = event.currentTarget;
    const file = input.files?.[0];
    if (!file) return;
    if (file.size > MAX_SCRIPT_UPLOAD_BYTES) {
      updateStatus('Script must be ≤ 1 MB');
      input.value = '';
      return;
    }

    const reader = new FileReader();
    reader.onload = () => {
      if (typeof reader.result !== 'string') return;
      editScriptUpload = {name: file.name, content: reader.result};
      editDraft = {
        ...editDraft,
        uploaded_script_name: file.name,
        uploaded_script_path: ''
      };
      updateStatus('Script file selected');
    };
    reader.onerror = () => updateStatus('Failed to read script file');
    reader.readAsText(file);
    input.value = '';
  }
  function clearUploadedScriptSelection() {
    editScriptUpload = null;
    editDraft = {...editDraft, uploaded_script_path:'', uploaded_script_name:''};
  }
  function addMacroStep() {
    editMacroSteps = [...editMacroSteps, createMacroStep()];
  }
  function removeMacroStep(index) {
    editMacroSteps = editMacroSteps.filter((_, i) => i !== index);
    if (recordingShortcutIndex === index) {
      stopShortcutRecording();
    } else if (recordingShortcutIndex !== null && recordingShortcutIndex > index) {
      recordingShortcutIndex -= 1;
    }
  }
  function updateMacroAction(index, action) {
    editMacroSteps = editMacroSteps.map((step, i) => {
      if (i !== index) return step;
      const nextAction = MACRO_ACTIONS.includes(action) ? action : 'run-script';
      const normalizedExistingValue = nextAction === 'keyboard-shortcut'
        ? normalizeShortcutSequence(step.value)
        : step.value;
      const nextValue = nextAction === 'delay' && !step.value.trim()
        ? '1000'
        : (nextAction === 'run-uploaded-script' ? '' : normalizedExistingValue);
      return {...step, action: nextAction, value: nextValue};
    });
    if (recordingShortcutIndex === index && action !== 'keyboard-shortcut') {
      stopShortcutRecording();
    }
  }
  function updateMacroValue(index, value) {
    editMacroSteps = editMacroSteps.map((step, i) => {
      if (i !== index) return step;
      const nextValue = step.action === 'keyboard-shortcut'
        ? normalizeShortcutSequence(value)
        : value;
      return {...step, value: nextValue};
    });
  }
  function toggleEditLink(targetId, enabled) {
    editSelectedLinks = enabled
      ? [...new Set([...editSelectedLinks, targetId])]
      : editSelectedLinks.filter(id=>id!==targetId);
  }
  async function saveEditor() {
    if (!editPopup.nodeId||isLockedNode(editPopup.nodeId)) return;
    const nodeId=editPopup.nodeId;
    const normalizedIcon = isImageIcon(editDraft.icon) ? editDraft.icon : editDraft.icon.trim();
    const nextType = normalizeNodeType(editDraft.node_type);
    const runMacroOnSystemStart = Boolean(editDraft.run_macro_on_system_start);
    let uploadedScriptPath = normalizeOptionalString(editDraft.uploaded_script_path);
    let uploadedScriptName = normalizeOptionalString(editDraft.uploaded_script_name);

    if (nextType === 'script' && editScriptUpload) {
      try {
        const saved = await invoke('save_uploaded_script', {
          fileName: editScriptUpload.name,
          content: editScriptUpload.content
        });
        uploadedScriptPath = normalizeOptionalString(saved?.path);
        uploadedScriptName = normalizeOptionalString(saved?.name) || editScriptUpload.name;
      } catch (e) {
        updateStatus(`Script upload failed: ${String(e)}`);
        return;
      }
    }

    if (nextType !== 'script') {
      uploadedScriptPath = null;
      uploadedScriptName = null;
    }

    const normalizedMacros = normalizeMacroSteps(editMacroSteps);

    nodes = nodes.map(n=>{
      if (n.id===MAIN_NODE_ID) {
        if (Boolean(n.run_macro_on_system_start)) {
          return {...n, run_macro_on_system_start:false};
        }
        return n;
      }

      if (n.id!==nodeId) {
        if (runMacroOnSystemStart && Boolean(n.run_macro_on_system_start)) {
          return {...n, run_macro_on_system_start:false};
        }
        return n;
      }

      return {...n,
        name:        editDraft.name.trim() || n.name,
        icon:        normalizedIcon,
        description: editDraft.description.trim(),
        node_type:   nextType,
        group:       (editDraft.group||'').trim(),
        uploaded_script_path: uploadedScriptPath,
        uploaded_script_name: uploadedScriptName,
        run_macro_on_system_start: runMacroOnSystemStart,
        color:       NODE_COLORS.includes(editDraft.color) ? editDraft.color : 'slate',
        links:       [...new Set(editSelectedLinks.filter(id=>id!==nodeId))],
        macros:      normalizedMacros,
        targets:     { path:normalizeOptionalString(editDraft.path), editor:normalizeOptionalString(editDraft.editor),
                       browser:normalizeOptionalString(editDraft.browser), script:normalizeOptionalString(editDraft.script) }
      };
    });
    // Ensure group metadata exists for any newly-referenced group.
    const newGroup = (editDraft.group||'').trim();
    if (newGroup && !groups.some(g=>g.id===newGroup)) {
      groups = [...groups, normalizeGroup({id:newGroup, name:newGroup, color:editDraft.color})];
    }
    groups = normalizeGroups(groups, nodes);
    syncSmooth(true); scheduleSave(); queueRender();
    closeEditor(); updateStatus('Node updated');
  }

  /* ─── Context menu ───────────────────────────────────────────────── */
  function openCtxMenu(event, nodeId) {
    event.preventDefault(); event.stopPropagation();
    const mw=220, mh=300;
    const x=clamp(event.clientX, 8, Math.max(8, window.innerWidth-mw-8));
    const y=clamp(event.clientY, 8, Math.max(8, window.innerHeight-mh-8));
    contextMenu={open:true, x, y, nodeId};
    highlightNodeId=nodeId;
  }
  function launchFromContext(action) { if (!contextNode) { closeCtx(); return; } void launchNode(contextNode,action); closeCtx(); }
  function openEditorFromMenu() {
    if (!contextNode||isLockedNode(contextNode)) { closeCtx(); return; }
    openEditor(contextNode.id); closeCtx();
  }
  function addConnected() {
    const src=contextNode; if (!src) { closeCtx(); return; }
    const next=createEmptyNode(src.x+148, src.y+24);
    nodes=[...nodes.map(n=>n.id!==src.id?n:{...n,links:[...new Set([...(n.links??[]),next.id])]}), next];
    syncSmooth(true); scheduleSave();
    void tick().then(()=>{ clampAllNodesToCanvas(true); queueRender(); });
    closeCtx(); updateStatus('Connected node added');
  }
  function connectNearest() {
    const src=contextNode; if (!src) { closeCtx(); return; }
    const candidates=nodes.filter(n=>n.id!==src.id); if (!candidates.length) { closeCtx(); return; }
    const nearest=candidates.reduce((b,n)=>{
      const nd=(n.x-src.x)**2+(n.y-src.y)**2; const bd=(b.x-src.x)**2+(b.y-src.y)**2;
      return nd<bd?n:b;
    });
    nodes=nodes.map(n=>n.id!==src.id?n:{...n,links:[...new Set([...(n.links??[]),nearest.id])]});
    scheduleSave(); queueRender(); closeCtx(); updateStatus('Connected nearest node');
  }
  function clearLinks() {
    const src=contextNode; if (!src) { closeCtx(); return; }
    nodes=nodes.map(n=>n.id!==src.id?n:{...n,links:[]});
    scheduleSave(); queueRender(); closeCtx(); updateStatus('Links cleared');
  }

  /* ─── Launcher / command palette ─────────────────────────────────── */
  function openLauncher() {
    showLauncher=true; launcherQuery=''; launcherIndex=0;
    void tick().then(()=>{ document.getElementById('launcher-input')?.focus(); });
  }
  function closeLauncher() { showLauncher=false; launcherQuery=''; }
  function launcherKey(event) {
    if (event.key==='Escape') { closeLauncher(); return; }
    if (event.key==='ArrowDown') { event.preventDefault(); launcherIndex=Math.min(launcherIndex+1,Math.max(0,launcherResults.length-1)); return; }
    if (event.key==='ArrowUp')   { event.preventDefault(); launcherIndex=Math.max(launcherIndex-1,0); return; }
    if (event.key==='Enter') { runLauncherResult(launcherResults[launcherIndex]); }
  }
  function runLauncherResult(result) {
    if (!result) return;
    closeLauncher();
    try { result.run(); } catch(e) { updateStatus(String(e)); }
  }
  function hasLaunchTarget(node, action) {
    if (action==='open-path')    return Boolean(node?.targets?.path);
    if (action==='open-browser') return Boolean(node?.targets?.browser);
    if (action==='run-script')   return Boolean(nodeScriptTarget(node));
    if (action==='run-macro')    return Array.isArray(node?.macros) && node.macros.length > 0;
    if (action==='open-editor')  return Boolean(node?.targets?.editor||node?.targets?.path);
    return false;
  }
  function nodePrimaryAction(node) {
    for (const action of ['open-path','open-editor','open-browser','run-script','run-macro']) {
      if (hasLaunchTarget(node, action)) return action;
    }
    return null;
  }
  const ACTION_ICONS = { 'open-path':'📁','open-editor':'✏️','open-browser':'🌐','run-script':'▶','run-macro':'⚡' };
  // Lightweight fuzzy subsequence scorer. Returns -1 when there is no match.
  function fuzzyScore(query, text) {
    if (!query) return 0;
    const q = query.toLowerCase(), t = (text||'').toLowerCase();
    if (!t) return -1;
    if (t === q) return 1000;
    let qi=0, score=0, streak=0;
    for (let ti=0; ti<t.length && qi<q.length; ti++) {
      if (t[ti]===q[qi]) { qi++; streak++; score += 4 + streak*2 + (ti===0?8:0); }
      else streak=0;
    }
    if (qi < q.length) return -1;
    if (t.startsWith(q)) score += 20;
    score -= (t.length - q.length) * 0.15;
    return score;
  }
  function buildCommandResults() {
    const cmds = [
      { id:'cmd-add',     title:'Add node',         subtitle:'Create a blank node',         icon:'＋', run:()=>addNode() },
      { id:'cmd-layout',  title:'Auto layout',      subtitle:'Tidy nodes into a grid',      icon:'⊞', run:()=>layoutGrid() },
      { id:'cmd-theme',   title:'Cycle theme',      subtitle:`Now: ${THEMES[appSettings.theme]?.label||'Cyan'}`, icon:'🎨', run:()=>cycleTheme() },
      { id:'cmd-export',  title:'Export workspace', subtitle:'Save active workspace to file', icon:'⭳', run:()=>void exportWorkspace() },
      { id:'cmd-import',  title:'Import workspace',  subtitle:'Load a .finnode.json file',    icon:'⭱', run:()=>triggerImportWorkspace() }
    ];
    for (const ws of workspaces) {
      if (ws.id === activeWorkspaceId) continue;
      cmds.push({ id:`cmd-ws-${ws.id}`, title:`Switch to ${ws.name}`, subtitle:'Workspace', icon:'❒', run:()=>void switchWorkspace(ws.id) });
    }
    return cmds.map(c => ({ ...c, type:'command' }));
  }
  function nodeToResult(node) {
    const action = nodePrimaryAction(node);
    return {
      id:`node-${node.id}`, type:'node', node, action,
      title: node.name || 'Untitled',
      subtitle: node.group ? `${groupLabel(node.group)} · ${node.targets?.path || node.targets?.browser || (action?macroActionLabel(action):'no target')}`
                           : (node.targets?.path || node.targets?.browser || (action?macroActionLabel(action):'no target')),
      icon: ACTION_ICONS[action] || '◇',
      color: nodeColor(node),
      run: () => { if (action) void launchNode(node, action); else updateStatus('Node has no launch target'); }
    };
  }
  function buildLauncherResults(query) {
    const q = query.trim();
    if (!q) {
      const recents = [...nodes]
        .filter(n => n.last_launched)
        .sort((a,b)=> String(b.last_launched).localeCompare(String(a.last_launched)))
        .slice(0,5)
        .map(nodeToResult);
      const fallback = recents.length ? recents : nodes.slice(0,5).map(nodeToResult);
      const cmds = buildCommandResults().slice(0,3);
      return [...fallback, ...cmds];
    }
    const scored = [];
    for (const node of nodes) {
      const hay = `${node.name} ${node.targets?.path||''} ${node.targets?.browser||''} ${node.group?groupLabel(node.group):''}`;
      const s = Math.max(fuzzyScore(q, node.name), fuzzyScore(q, hay) - 4);
      if (s >= 0) scored.push({ result:nodeToResult(node), score:s + 6 });
    }
    for (const cmd of buildCommandResults()) {
      const s = fuzzyScore(q, cmd.title);
      if (s >= 0) scored.push({ result:{...cmd}, score:s });
    }
    return scored.sort((a,b)=>b.score-a.score).slice(0,8).map(x=>x.result);
  }

  /* ─── Groups ─────────────────────────────────────────────────────── */
  function groupById(id)   { return groups.find(g=>g.id===id) || null; }
  function groupLabel(id)  { const g=groupById(id); return g ? g.name : id; }
  function toggleGroupCollapsed(id) {
    groups = groups.map(g => g.id!==id ? g : {...g, collapsed:!g.collapsed});
    scheduleSave(); queueRender();
  }
  function computeGroupHulls(list, grps) {
    const pad = 26, half = NODE_SIZE/2;
    const hulls = [];
    for (const g of grps) {
      const members = list.filter(n => n.group === g.id);
      if (!members.length) continue;
      let minX=Infinity, minY=Infinity, maxX=-Infinity, maxY=-Infinity;
      for (const m of members) {
        minX=Math.min(minX, m.renderX); minY=Math.min(minY, m.renderY);
        maxX=Math.max(maxX, m.renderX+NODE_SIZE); maxY=Math.max(maxY, m.renderY+NODE_SIZE);
      }
      hulls.push({
        id:g.id, name:g.name, collapsed:g.collapsed, color:NODE_COLOR_MAP[g.color]||NODE_COLOR_MAP.cyan,
        count:members.length,
        x:minX-pad, y:minY-pad-18, w:(maxX-minX)+pad*2, h:(maxY-minY)+pad*2+18,
        cx:(minX+maxX)/2 - half, cy:(minY+maxY)/2 - half
      });
    }
    return hulls;
  }

  /* ─── Drag-drop to create nodes ──────────────────────────────────── */
  const SCRIPT_EXTS = ['sh','bash','zsh','command','bat','cmd','ps1','py','js','ts','rb','pl'];
  function looksLikeUrl(s) { return /^(https?:\/\/|www\.)/i.test((s||'').trim()); }
  function baseName(p) {
    const norm = (p||'').replace(/[\\/]+$/,'');
    const parts = norm.split(/[\\/]/);
    return parts[parts.length-1] || p;
  }
  function extOf(p) { const m=/\.([a-z0-9]+)$/i.exec((p||'').trim()); return m ? m[1].toLowerCase() : ''; }
  function makeNodeFromTarget(target, index) {
    const node = createEmptyNode(70 + index*26, 90 + index*26);
    const t = (target||'').trim();
    if (!t) return null;
    if (looksLikeUrl(t)) {
      const url = t.startsWith('http') ? t : `https://${t}`;
      node.name = baseName(url) || 'Link';
      node.targets = {...node.targets, browser:url};
      node.icon = '🌐'; node.color = 'cyan';
    } else {
      const ext = extOf(t);
      node.name = baseName(t);
      if (SCRIPT_EXTS.includes(ext)) {
        node.node_type = 'script';
        node.targets = {...node.targets, script:t, path:t};
        node.icon = '▶'; node.color = 'green';
      } else {
        node.targets = {...node.targets, path:t, editor:t};
        node.icon = ext ? '📄' : '📁'; node.color = ext ? 'amber' : 'slate';
      }
    }
    return node;
  }
  function addNodesFromTargets(targets) {
    const incoming = (targets||[]).map((t,i)=>makeNodeFromTarget(t,i)).filter(Boolean);
    if (!incoming.length) return;
    nodes = [...nodes, ...incoming];
    syncSmooth(true); scheduleSave();
    void tick().then(()=>{ clampAllNodesToCanvas(true); queueRender(); });
    updateStatus(incoming.length===1 ? `Added "${incoming[0].name}"` : `Added ${incoming.length} nodes`);
  }
  function onBoardDragOver(event) {
    if (event.dataTransfer && Array.from(event.dataTransfer.types||[]).some(t=>t==='text/uri-list'||t==='text/plain')) {
      event.preventDefault();
      isDropActive = true;
    }
  }
  function onBoardDragLeave(event) {
    if (event.relatedTarget && event.currentTarget.contains(event.relatedTarget)) return;
    isDropActive = false;
  }
  function onBoardDrop(event) {
    event.preventDefault();
    isDropActive = false;
    const dt = event.dataTransfer; if (!dt) return;
    const raw = dt.getData('text/uri-list') || dt.getData('text/plain');
    if (!raw) return;
    const targets = raw.split(/\r?\n/).map(s=>s.trim()).filter(s=>s && !s.startsWith('#'));
    if (targets.length) addNodesFromTargets(targets);
  }

  /* ─── Hover / resize ─────────────────────────────────────────────── */
  function onNodeEnter(nodeId) { hoveredId=nodeId; highlightNodeId=nodeId; }
  function onNodeLeave(nodeId) { if(hoveredId===nodeId) hoveredId=null; if(!contextMenu.open&&highlightNodeId===nodeId) highlightNodeId=null; }

  function scheduleNodeLayerRelayout() {
    if (nodeLayerResizeFrame!==null) return;
    nodeLayerResizeFrame=requestAnimationFrame(()=>{ nodeLayerResizeFrame=null; queueRender(); clampAllNodesToCanvas(false); });
  }
  function setupResizeObserver() {
    if (typeof ResizeObserver==='undefined'||!canvasEl) return;
    if (nodeLayerResizeObserver) nodeLayerResizeObserver.disconnect();
    nodeLayerResizeObserver=new ResizeObserver(()=>scheduleNodeLayerRelayout());
    nodeLayerResizeObserver.observe(canvasEl);
  }

  /* ─── Wheel zoom ─────────────────────────────────────────────────── */
  function onWheelZoom(event) {
    if (!event.ctrlKey&&!event.metaKey) return;
    event.preventDefault();
    const delta = event.deltaY>0 ? -ZOOM_STEP : ZOOM_STEP;
    zoomLevel=clamp(parseFloat((zoomLevel+delta).toFixed(2)), ZOOM_MIN, ZOOM_MAX);
    queueRender();
  }

  /* ─── Bootstrap ──────────────────────────────────────────────────── */
  async function bootstrap() {
    const teardownReducedMotion = setupReducedMotionListener();
    const unlistenLayout   = await listen('layout-updated', ()=>void loadWorkspaces());
    const unlistenLauncher = await listen('toggle-quick-launcher', ()=>{
      showLauncher ? closeLauncher() : openLauncher();
    });

    // OS file/folder drops (handled by Tauri's webview, not the DOM drop event).
    let unlistenFileDrop = () => {};
    try {
      unlistenFileDrop = await appWindow.onFileDropEvent(ev => {
        const payload = ev?.payload;
        if (!payload) return;
        if (payload.type === 'hover') { isDropActive = true; }
        else if (payload.type === 'drop') { isDropActive = false; addNodesFromTargets(payload.paths || []); }
        else { isDropActive = false; }
      });
    } catch (_) {}

    // Try to push desktop window to the bottom layer (needs custom Rust command)
    if (currentWindowLabel==='desktop') {
      try { await invoke('set_window_bottom'); } catch(_) {}
    }

    await loadWorkspaces();
    await loadAppSettings();
    await tick();
    setupResizeObserver();

    const onResize = ()=>scheduleNodeLayerRelayout();
    const onMove   = e=>onPointerMove(e);
    const onUp     = ()=>onPointerUp();
    const onDown   = e=>{
      if (!(e.target instanceof Element)) return;
      if (!e.target.closest('.context-menu'))  closeCtx();
      if (!e.target.closest('.node')&&!e.ctrlKey&&!e.metaKey) { selectedIds=new Set(); expandedNodeId=null; }
    };
    const onKey = e=>{
      if (handleShortcutRecordingKeydown(e)) return;
      if (e.key==='Escape') { closeCtx(); closeLauncher(); return; }
      if ((e.key.toLowerCase()==='k'&&(e.ctrlKey||e.metaKey))||(e.code==='Space'&&e.altKey)) {
        e.preventDefault(); showLauncher ? closeLauncher() : openLauncher();
      }
    };
    window.addEventListener('resize', onResize);
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup',   onUp);
    window.addEventListener('pointerdown', onDown);
    window.addEventListener('keydown',     onKey);
    return ()=>{
      unlistenLayout(); unlistenLauncher(); unlistenFileDrop();
      teardownReducedMotion();
      window.removeEventListener('resize', onResize);
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup',   onUp);
      window.removeEventListener('pointerdown', onDown);
      window.removeEventListener('keydown',     onKey);
      stopShortcutRecording();
      if (saveTimer!==null)        clearTimeout(saveTimer);
      if (statusFlashTimer!==null) clearTimeout(statusFlashTimer);
      if (workspaceSwitchTimer!==null) clearTimeout(workspaceSwitchTimer);
      if (dragFrame!==null)        cancelAnimationFrame(dragFrame);
      if (springFrame!==null)      cancelAnimationFrame(springFrame);
      if (nodeLayerResizeObserver) nodeLayerResizeObserver.disconnect();
      if (nodeLayerResizeFrame!==null) cancelAnimationFrame(nodeLayerResizeFrame);
    };
  }

  /* ─── Reactivity ─────────────────────────────────────────────────── */
  $: smoothLookup = new Map(smoothNodes.map(n=>[n.id,n]));
  $: renderNodes  = nodes.map(n=>{
    const s=smoothLookup.get(n.id);
    const useRaw=draggingId===n.id;
    return {...n, renderX:useRaw?n.x:(s?s.x:n.x), renderY:useRaw?n.y:(s?s.y:n.y)};
  });
  $: contextNode = contextMenu.nodeId ? nodes.find(n=>n.id===contextMenu.nodeId)??null : null;
  $: editNode    = editPopup.nodeId   ? nodes.find(n=>n.id===editPopup.nodeId)??null   : null;
  // Command palette results (depends on query, nodes, workspaces, theme).
  $: launcherResults = (showLauncher, buildLauncherResults(launcherQuery));
  $: if (launcherIndex > launcherResults.length-1) launcherIndex = Math.max(0, launcherResults.length-1);

  // Group rendering: hide members of collapsed groups, draw hulls behind nodes.
  $: collapsedGroupIds = new Set(groups.filter(g=>g.collapsed).map(g=>g.id));
  $: hiddenNodeIds = new Set(renderNodes.filter(n=>n.group && collapsedGroupIds.has(n.group)).map(n=>n.id));
  $: boardNodes = renderNodes.filter(n=>!hiddenNodeIds.has(n.id));
  $: groupHulls = computeGroupHulls(renderNodes, groups);

  // Usage analytics (surfaced in the settings Insights panel).
  $: realNodes = nodes.filter(n=>n.id!==MAIN_NODE_ID);
  $: totalLaunches = realNodes.reduce((sum,n)=>sum+(Number(n.launch_count)||0), 0);
  $: mostUsedNodes = [...realNodes]
       .filter(n=>(Number(n.launch_count)||0)>0)
       .sort((a,b)=>(Number(b.launch_count)||0)-(Number(a.launch_count)||0))
       .slice(0,3);

  $: isNodeBoardWindow = currentWindowLabel==='desktop';
  $: if (typeof document!=='undefined') {
    document.documentElement.classList.toggle('desktop-overlay-window', isNodeBoardWindow && !showcaseMode);
    document.body.classList.toggle('desktop-mode', isNodeBoardWindow && !showcaseMode);
    document.body.classList.toggle('showcase-mode', showcaseMode);
  }
  $: {
    const validIds=new Set(nodes.map(n=>n.id));
    const next=[...selectedIds].filter(id=>validIds.has(id));
    if (next.length!==selectedIds.size) selectedIds=new Set(next);
    if (hoveredId&&!validIds.has(hoveredId))           hoveredId=null;
    if (highlightNodeId&&!validIds.has(highlightNodeId)) highlightNodeId=null;
    if (expandedNodeId&&!validIds.has(expandedNodeId))   expandedNodeId=null;
  }

  // Web-only preview affordance (documented in README): `?view=board` renders
  // the node-board surface in a plain browser; `?showcase=1` uses an opaque
  // backdrop. In a real Tauri window the per-window label always wins.
  function resolveWindowLabel() {
    const fallback = appWindow?.label ?? 'main';
    if (typeof window === 'undefined') return fallback;
    try {
      const params = new URLSearchParams(window.location.search);
      const v = (params.get('view') || '').toLowerCase();
      showcaseMode = params.get('showcase') === '1';
      if (v === 'board' || v === 'desktop') return 'desktop';
      if (v === 'settings' || v === 'main') return 'main';
    } catch (_) {}
    return fallback;
  }

  // Web-preview affordances (no-ops inside the Tauri shell): deep-link to UI
  // states so the documented browser preview can show themes, the launcher, or
  // the node editor directly. e.g. ?view=board&theme=violet&bg=aurora&launcher=1
  function applyPreviewAffordances() {
    if (HAS_TAURI || typeof window === 'undefined') return;
    let params;
    try { params = new URLSearchParams(window.location.search); } catch (_) { return; }
    const theme = params.get('theme');
    if (theme && THEME_IDS.includes(theme)) { appSettings = {...appSettings, theme}; applyAppearance(); }
    const bg = params.get('bg');
    if (bg && BOARD_BACKGROUND_IDS.includes(bg)) appSettings = {...appSettings, board_background: bg};
    const collapse = params.get('collapse');
    if (collapse) groups = groups.map(g => g.id===collapse ? {...g, collapsed:true} : g);
    const edit = params.get('edit');
    if (edit) void tick().then(()=>openEditor(edit));
    if (params.get('launcher') === '1') {
      openLauncher();
      const q = params.get('q');
      if (q) void tick().then(()=>{ launcherQuery = q; });
    }
  }

  onMount(()=>{
    currentWindowLabel=resolveWindowLabel();
    let disposed=false, cleanup=()=>{};
    void bootstrap().then(fn=>{ if(disposed){fn();return;} cleanup=fn; applyPreviewAffordances(); })
      .catch(e=>{ fatalError=e?.stack??e?.message??String(e); });
    return ()=>{
      disposed=true; cleanup();
      if (typeof document!=='undefined') {
        document.documentElement.classList.remove('desktop-overlay-window');
        document.body.classList.remove('desktop-mode');
      }
    };
  });
</script>

<!-- ─────────────────────────────── TEMPLATE ──────────────────────────── -->

{#if fatalError}
  <pre class="fatal">{fatalError}</pre>
{:else}

  <!-- ══════════════════════ NODE BOARD WINDOW ════════════════════════ -->
  {#if isNodeBoardWindow}
    <main class="nodeboard-shell">
      <div class="nodeboard-frame">

        <!-- Title bar -->
        <div class="nodeboard-titlebar" data-tauri-drag-region>
          <span class="tb-label" data-tauri-drag-region>FinNode</span>
          <div class="tb-actions">
            <button class="tb-btn" on:click={addNode}    title="Add node">＋</button>
            <button class="tb-btn" on:click={layoutGrid} title="Auto layout">⊞</button>
            <button class="tb-btn" on:click={openLauncher} title="Search (⌘K)">⌘</button>
          </div>
        </div>

        <!-- Canvas: bind canvasEl here (outer, unscaled logical container) -->
        <!-- svelte-ignore a11y-no-static-element-interactions -->
        <div
          class="canvas"
          class:canvas--panning={isPanning}
          class:canvas--drop={isDropActive}
          data-board-bg={appSettings.board_background}
          bind:this={canvasEl}
          on:pointerdown={beginCanvasPan}
          on:auxclick={preventMiddleAuxClick}
          on:wheel|passive={onWheelZoom}
          on:dragover={onBoardDragOver}
          on:dragleave={onBoardDragLeave}
          on:drop={onBoardDrop}
        >

          <!-- Decorative background affected by opacity slider -->
          <div class="canvas-background" style="--board-bg-opacity:{boardOpacity / 100}" aria-hidden="true">
            <div class="canvas-glow"></div>
            <div class="canvas-dots"></div>
          </div>

          <!-- Drag-drop hint overlay -->
          {#if isDropActive}
            <div class="drop-overlay" aria-hidden="true">
              <div class="drop-overlay-inner">
                <span class="drop-overlay-icon">⤓</span>
                <span>Drop files, folders, or links to create nodes</span>
              </div>
            </div>
          {/if}

          <!-- Zoom root: both SVG links + nodes scale together -->
          <div class="zoom-root"
            class:zoom-root--workspace-shift={isSwitchingWorkspace}
            bind:this={nodeLayer}
            style="transform:translate({boardPan.x}px,{boardPan.y}px) scale({zoomLevel});transform-origin:0 0;position:absolute;inset:0;width:100%;height:100%;">

            <!-- SVG links: coordinates match node renderX/Y (logical space) -->
            <svg class="links" {viewBox} style="position:absolute;inset:0;width:100%;height:100%;overflow:visible;">
              <defs>
                <marker id="arrowhead" markerWidth="7" markerHeight="7" refX="6" refY="3.5" orient="auto">
                  <path d="M0,0 L0,7 L7,3.5 z" fill="rgba(94,231,247,0.4)" />
                </marker>
                <filter id="link-glow">
                  <feGaussianBlur stdDeviation="2" result="blur"/>
                  <feMerge><feMergeNode in="blur"/><feMergeNode in="SourceGraphic"/></feMerge>
                </filter>
              </defs>
              {#each links as link (link.from+'-'+link.to)}
                <path class="link"
                  class:link--highlight={highlightNodeId&&(link.from===highlightNodeId||link.to===highlightNodeId)}
                  d={link.d}
                  marker-end="url(#arrowhead)"
                />
              {/each}
            </svg>

            <!-- Group hulls (behind nodes) + collapsed group chips -->
            {#each groupHulls as hull (hull.id)}
              {#if hull.collapsed}
                <button class="group-chip" style="left:{hull.cx}px;top:{hull.cy}px;--gc:{hull.color}"
                  on:pointerdown|stopPropagation
                  on:click|stopPropagation={()=>toggleGroupCollapsed(hull.id)}
                  title="Expand group">
                  <span class="group-chip-icon">▸</span>
                  <span class="group-chip-name">{hull.name}</span>
                  <span class="group-chip-count">{hull.count}</span>
                </button>
              {:else}
                <div class="group-hull" style="left:{hull.x}px;top:{hull.y}px;width:{hull.w}px;height:{hull.h}px;--gc:{hull.color}" aria-hidden="true">
                  <button class="group-hull-label"
                    on:pointerdown|stopPropagation
                    on:click|stopPropagation={()=>toggleGroupCollapsed(hull.id)}
                    title="Collapse group">
                    <span class="group-hull-dot"></span>{hull.name}
                    <span class="group-hull-toggle">▾</span>
                  </button>
                </div>
              {/if}
            {/each}

            <!-- Nodes -->
            {#each boardNodes as node (node.id)}
              <div
                class="node"
                class:node--selected={selectedIds.has(node.id)}
                class:node--dragging={draggingId===node.id}
                class:node--expanded={expandedNodeId===node.id}
                class:node--main={node.id===MAIN_NODE_ID}
                class:node--hovered={hoveredId===node.id}
                role="button"
                tabindex="0"
                use:nodeRef={node.id}
                in:fade={{ duration: motionDuration(160) }}
                out:fade={{ duration: motionDuration(120) }}
                style="left:{node.renderX}px;top:{node.renderY}px;--nc:{nodeColor(node)}"
                on:pointerdown={e=>beginDrag(e,node.id)}
                on:click={e=>onNodeClick(e,node.id)}
                on:keydown={e=>onNodeKeydown(e,node.id)}
                on:contextmenu={e=>openCtxMenu(e,node.id)}
                on:dblclick={()=>onNodeDoubleClick(node)}
                on:mouseenter={()=>onNodeEnter(node.id)}
                on:mouseleave={()=>onNodeLeave(node.id)}
              >
                <!-- Action buttons (shown when node is expanded / left-clicked) -->
                {#if expandedNodeId===node.id}
                  <div class="node-actions" role="group" aria-label="Actions for {node.name||'node'}">
                    {#if hasLaunchTarget(node,'open-path')}
                      <button class="action-btn action-btn--path"
                        on:pointerdown|stopPropagation
                        on:click|stopPropagation={()=>void launchNode(node,'open-path')}
                        title="Open folder">
                        <span>📁</span><span class="action-label">Open</span>
                      </button>
                    {/if}
                    {#if hasLaunchTarget(node,'open-editor')}
                      <button class="action-btn action-btn--editor"
                        on:pointerdown|stopPropagation
                        on:click|stopPropagation={()=>void launchNode(node,'open-editor')}
                        title="Open in editor">
                        <span>✏️</span><span class="action-label">Edit</span>
                      </button>
                    {/if}
                    {#if hasLaunchTarget(node,'open-browser')}
                      <button class="action-btn action-btn--browser"
                        on:pointerdown|stopPropagation
                        on:click|stopPropagation={()=>void launchNode(node,'open-browser')}
                        title="Open in browser">
                        <span>🌐</span><span class="action-label">Web</span>
                      </button>
                    {/if}
                    {#if hasLaunchTarget(node,'run-script')}
                      <button class="action-btn action-btn--script"
                        on:pointerdown|stopPropagation
                        on:click|stopPropagation={()=>void launchNode(node,'run-script')}
                        title="Run script">
                        <span>▶</span><span class="action-label">Run</span>
                      </button>
                    {/if}
                    {#if hasLaunchTarget(node,'run-macro')}
                      <button class="action-btn action-btn--macro"
                        on:pointerdown|stopPropagation
                        on:click|stopPropagation={()=>void launchNode(node,'run-macro')}
                        title="Run macro">
                        <span>⚡</span><span class="action-label">Macro</span>
                      </button>
                    {/if}
                    {#if !isLockedNode(node)}
                      <button class="action-btn action-btn--edit"
                        on:pointerdown|stopPropagation
                        on:click|stopPropagation={()=>{ openEditor(node.id); expandedNodeId=null; }}
                        title="Edit node settings">
                        <span>⚙</span><span class="action-label">Config</span>
                      </button>
                    {/if}
                  </div>
                {/if}

                <!-- Outer glow ring -->
                <div class="node-ring" aria-hidden="true"></div>

                <!-- Node body -->
                <div class="node-inner">
                  <div class="node-top-row">
                    <span class="node-dot"
                      class:node-dot--active={Boolean(node.last_launched)}
                      class:node-dot--core={node.id===MAIN_NODE_ID}
                    ></span>
                    {#if !isLockedNode(node)}
                      <button class="node-edit-btn" title="Edit"
                        on:click|stopPropagation={()=>openEditor(node.id)}>⋯</button>
                    {/if}
                  </div>

                  {#if isImageIcon(node.icon)}
                    <img class="node-icon-img" src={node.icon} alt={node.name||'Node icon'} />
                  {:else if isLogoIcon(node.icon)}
                    <img class="node-logo" src={appLogo} alt="FinNode" />
                  {:else}
                    <div class="node-icon">{node.icon||node.name?.charAt(0)?.toUpperCase()||'?'}</div>
                  {/if}

                  <div class="node-name">{node.name||'Untitled'}</div>

                  {#if expandedNodeId===node.id&&node.description}
                    <div class="node-desc">{node.description}</div>
                  {/if}
                </div>
              </div>
            {/each}
          </div><!-- /zoom-root -->
        </div><!-- /canvas -->

        <!-- Zoom controls -->
        <div class="zoom-controls">
          <button class="zoom-btn" on:click={zoomOut} title="Zoom out (Ctrl+scroll)" disabled={zoomLevel<=ZOOM_MIN}>−</button>
          <span class="zoom-pct">{Math.round(zoomLevel*100)}%</span>
          <button class="zoom-btn" on:click={zoomIn}  title="Zoom in (Ctrl+scroll)"  disabled={zoomLevel>=ZOOM_MAX}>+</button>
          <div class="zoom-divider"></div>
          <button class="zoom-btn" on:click={()=>{ zoomLevel=1; boardPan={x:0,y:0}; queueRender(); }} title="Reset zoom and pan">⊙</button>
        </div>

        <!-- Status strip -->
        <div class="board-left-controls">
          <div class="nb-opacity" title="Node board opacity">
            <span class="nb-opacity-label">Opacity</span>
            <input
              class="nb-opacity-slider"
              type="range"
              min={BOARD_OPACITY_MIN}
              max={BOARD_OPACITY_MAX}
              step="1"
              value={boardOpacity}
              on:input={e=>setBoardOpacity(e.currentTarget.value)}
              aria-label="Node board opacity"
            />
            <span class="nb-opacity-value">{boardOpacity}%</span>
          </div>
        </div>

        <div class="nb-status">
          <div class="nb-status-left">
            <span class="nb-status-dot" class:nb-status-dot--flash={statusFlash}></span>
            {#key statusVersion}
              <span class="nb-status-text"
                in:fade={{ duration: motionDuration(150) }}
                out:fade={{ duration: motionDuration(90) }}
              >{statusText}</span>
            {/key}
          </div>
          <div class="nb-status-right">
            <span class="nb-status-count">{nodes.length} nodes</span>
          </div>
        </div>

      </div><!-- /nodeboard-frame -->
    </main>

  <!-- ══════════════════════ SETTINGS WINDOW ══════════════════════════ -->
  {:else}
    <main class="settings-shell">
      <aside class="sidebar">

        <!-- Brand header -->
        <header class="panel brand-panel">
          <div class="brand-identity" data-tauri-drag-region>
            <div class="brand-logo-wrap">
              <img class="brand-logo" src={appLogo} alt="FinNode" />
            </div>
            <div class="brand-text">
              <h1>FinNode</h1>
              <p>Project Navigator</p>
            </div>
          </div>
          <div class="wm-btns">
            <button class="wm-btn wm-btn--min"   on:click={minimizeWindow} title="Minimize">−</button>
            <button class="wm-btn wm-btn--close"  on:click={exitApplication}   title="Exit app">✕</button>
          </div>
        </header>

        <!-- App settings -->
        <section class="panel">
          <div class="panel-head">
            <h2>Application</h2>
          </div>
          <div class="setting-list">
            <label class="setting-row">
              <span>Start app when the PC starts</span>
              <input
                type="checkbox"
                checked={appSettings.start_on_boot}
                disabled={savingStartOnBoot}
                on:change={e=>updateStartOnBoot(e.currentTarget.checked)}
              />
            </label>
          </div>
        </section>

        <!-- Workspaces -->
        <section class="panel">
          <div class="panel-head">
            <h2>Workspaces</h2>
            <span class="badge">{workspaces.length}</span>
          </div>
          <select value={activeWorkspaceId} on:change={e=>switchWorkspace(e.currentTarget.value)}>
            {#each workspaces as ws}
              <option value={ws.id}>{ws.name}</option>
            {/each}
          </select>
          <div class="input-row">
            <input bind:value={workspaceName} placeholder="New workspace name…" />
            <button class="btn-accent" on:click={createWorkspace}>+</button>
          </div>
          <div class="btn-pair">
            <button class="btn-ghost" on:click={exportWorkspace} title="Export active workspace to a shareable file">⭳ Export</button>
            <button class="btn-ghost" on:click={triggerImportWorkspace} title="Import a .finnode.json workspace">⭱ Import</button>
          </div>
          <input id="workspace-import-input" type="file" accept=".json,application/json" class="hidden-file" on:change={handleWorkspaceImport} />
          <button class="btn-danger" disabled={workspaces.length<=1} on:click={()=>deleteWorkspace(activeWorkspaceId)}>
            Delete workspace
          </button>
        </section>

        <!-- Appearance -->
        <section class="panel">
          <div class="panel-head">
            <h2>Appearance</h2>
            <span class="badge">{THEMES[appSettings.theme]?.label||'Cyan'}</span>
          </div>
          <span class="field-label">Theme</span>
          <div class="theme-swatches">
            {#each THEME_IDS as id}
              <button type="button"
                class="theme-swatch"
                class:theme-swatch--active={appSettings.theme===id}
                style="--ta:{THEMES[id].accent};--tb:{THEMES[id].accent2}"
                on:click={()=>setTheme(id)}
                title={THEMES[id].label}
                aria-label={THEMES[id].label}
              ></button>
            {/each}
          </div>
          <span class="field-label" style="margin-top:10px">Board background</span>
          <div class="bg-options">
            {#each BOARD_BACKGROUNDS as bg}
              <button type="button"
                class="bg-option"
                class:bg-option--active={appSettings.board_background===bg.id}
                on:click={()=>setBoardBackground(bg.id)}
              >{bg.label}</button>
            {/each}
          </div>
        </section>

        <!-- Insights -->
        <section class="panel">
          <div class="panel-head">
            <h2>Insights</h2>
            <span class="badge">{totalLaunches}</span>
          </div>
          <div class="insights-stats">
            <div class="insight-stat">
              <span class="insight-val">{totalLaunches}</span>
              <span class="insight-lbl">launches</span>
            </div>
            <div class="insight-stat">
              <span class="insight-val">{realNodes.length}</span>
              <span class="insight-lbl">nodes</span>
            </div>
            <div class="insight-stat">
              <span class="insight-val">{groups.length}</span>
              <span class="insight-lbl">groups</span>
            </div>
          </div>
          {#if mostUsedNodes.length}
            <span class="field-label" style="margin-top:10px">Most used</span>
            <div class="most-used">
              {#each mostUsedNodes as n (n.id)}
                <button class="most-used-row" style="--nc:{nodeColor(n)}"
                  on:click={()=>{ const a=nodePrimaryAction(n); if(a) void launchNode(n,a); }}>
                  <span class="most-used-pip"></span>
                  <span class="most-used-name" title={n.name||'Untitled'}>{n.name||'Untitled'}</span>
                  <span class="most-used-count">{n.launch_count}×</span>
                </button>
              {/each}
            </div>
          {:else}
            <p class="muted" style="padding:6px 2px 0">Launch nodes to build usage insights.</p>
          {/if}
        </section>

        <!-- Nodes -->
        <section class="panel">
          <div class="panel-head">
            <h2>Nodes</h2>
            <span class="badge">{nodes.length}</span>
          </div>
          <div class="btn-row">
            <button class="btn-primary" on:click={addNode}>＋ Add</button>
            <button class="btn-ghost"   on:click={layoutGrid}>⊞ Layout</button>
            <button class="btn-ghost"   on:click={openLauncher}>⌘ Search</button>
          </div>

          {#if selectedIds.size>0}
            <div class="sel-bar">
              <span>{selectedIds.size} selected</span>
              <button on:click={batchLaunch}>Open all</button>
              <button on:click={groupSelectedNodes}>⊟ Group</button>
              <button class="danger" on:click={batchDelete}>Delete</button>
              <button on:click={()=>selectedIds=new Set()}>✕</button>
            </div>
          {/if}

          <div class="node-list" class:node-list--workspace-shift={isSwitchingWorkspace}>
            {#each nodes as node (node.id)}
              {@const locked=isLockedNode(node)}
              <div
                class="node-row"
                class:node-row--sel={selectedIds.has(node.id)}
                style="--nc:{nodeColor(node)}"
                in:fade={{ duration: motionDuration(130) }}
                out:fade={{ duration: motionDuration(90) }}
              >
                <span class="node-row-pip"></span>
                <span class="node-row-name" title={node.name||'Untitled'}>{node.name||'Untitled'}</span>
                <div class="node-row-acts">
                  <button title="Open path" disabled={!hasLaunchTarget(node,'open-path')}
                    on:click={()=>void launchNode(node,'open-path')}>▶</button>
                  {#if !locked}
                    <button title="Edit" on:click={()=>openEditor(node.id)}>✏</button>
                  {/if}
                  <button title="Clone" disabled={locked} on:click={()=>cloneNode(node.id)}>⧉</button>
                  <button class="btn-danger-xs" title="Delete" disabled={locked} on:click={()=>deleteNode(node.id)}>✕</button>
                </div>
              </div>
            {/each}
          </div>
        </section>

        <!-- Status -->
        <section class="panel status-panel">
          <div class="panel-head">
            <h2>Status</h2>
            <span class="status-dot" class:status-dot--ok={!statusText.toLowerCase().includes('error')}></span>
          </div>
          {#key statusVersion}
            <p class="status-msg"
              in:fade={{ duration: motionDuration(150) }}
              out:fade={{ duration: motionDuration(90) }}
            >{statusText}</p>
          {/key}
          <div class="activity">
            {#if activityLog.length===0}
              <p class="muted">No activity yet.</p>
            {:else}
              {#each activityLog as item (item.id)}
                <div
                  class="activity-item"
                  in:fly={{ y: -8, duration: motionDuration(180), opacity: 0 }}
                  out:fade={{ duration: motionDuration(90) }}
                >{item.text}</div>
              {/each}
            {/if}
          </div>
        </section>

      </aside>
    </main>
  {/if}

  <!-- ══════════════════════ EDITOR MODAL ════════════════════════════ -->
  {#if editPopup.open&&editNode}
    <!-- svelte-ignore a11y-click-events-have-key-events -->
    <div
      class="modal-overlay"
      role="presentation"
      in:fade={{ duration: motionDuration(140) }}
      out:fade={{ duration: motionDuration(110) }}
    >
      <section
        class="editor-modal"
        in:scale={{ duration: motionDuration(180), start: 0.97 }}
        out:scale={{ duration: motionDuration(120), start: 0.99 }}
      >
        <header class="editor-header">
          <div class="editor-title-row">
            <div class="editor-preview-dot" style="background:{nodeColor(editNode)};box-shadow:0 0 10px {nodeColor(editNode)}88"></div>
            <h3>Edit Node</h3>
          </div>
          <button class="icon-btn" on:click={closeEditor}>✕</button>
        </header>

        <div class="editor-grid">
          <label class="field">
            <span>Name</span>
            <input bind:value={editDraft.name} placeholder="Node name" />
          </label>
          <label class="field">
            <span>Icon (character)</span>
            <input bind:value={editDraft.icon} placeholder="emoji or letter" maxlength="4" />
          </label>
        </div>

        <div class="editor-grid">
          <label class="field">
            <span>Node type</span>
            <select bind:value={editDraft.node_type}>
              <option value="default">Default</option>
              <option value="script">Script</option>
            </select>
          </label>
          <label class="field">
            <span>Group</span>
            <input list="group-options" bind:value={editDraft.group} placeholder="None" />
            <datalist id="group-options">
              {#each groups as g (g.id)}
                <option value={g.name}></option>
              {/each}
            </datalist>
          </label>
        </div>

        <label class="field field-toggle">
          <span>Run Macro On System Start</span>
          <div class="field-toggle-row">
            <input
              type="checkbox"
              checked={Boolean(editDraft.run_macro_on_system_start)}
              on:change={e=>editDraft={...editDraft, run_macro_on_system_start:e.currentTarget.checked}}
            />
              <small>Runs this node macro when FinNode starts from system startup. Main node is never used, and enabling this here turns it off for other nodes.</small>
          </div>
        </label>

        <label class="field">
          <span>Upload icon image</span>
          <input type="file" accept="image/*" on:change={handleNodeIconUpload} />
        </label>

        {#if isImageIcon(editDraft.icon)}
          <div class="icon-preview">
            <img class="icon-preview-img" src={editDraft.icon} alt="Icon preview" />
            <button class="btn-ghost" on:click={clearUploadedNodeIcon}>Remove image</button>
          </div>
        {/if}

        <label class="field">
          <span>Description</span>
          <textarea rows="2" bind:value={editDraft.description} placeholder="Brief description…"></textarea>
        </label>

        <div class="field-group">
          <label class="field">
            <span>📁 Folder path</span>
            <input bind:value={editDraft.path} placeholder="/path/to/project" />
          </label>
          <label class="field">
            <span>✏️ Editor command</span>
            <input bind:value={editDraft.editor} placeholder="code /path/to/project" />
          </label>
          <label class="field">
            <span>🌐 Browser URL</span>
            <input bind:value={editDraft.browser} placeholder="https://example.com" />
          </label>
          <label class="field">
            <span>{editDraft.node_type==='script' ? '▶ Script command (fallback)' : '▶ Script command'}</span>
            <input bind:value={editDraft.script} placeholder="npm run dev" />
          </label>
        </div>

        {#if editDraft.node_type==='script'}
          <label class="field">
            <span>Upload script file</span>
            <input
              type="file"
              accept=".sh,.bash,.zsh,.command,.bat,.cmd,.ps1,.py,.js,.ts,.rb,.pl,text/*"
              on:change={handleNodeScriptUpload}
            />
          </label>

          <div class="script-upload-info">
            {#if editScriptUpload}
              <span>Selected file: {editScriptUpload.name}</span>
            {:else if editDraft.uploaded_script_name}
              <span>Saved file: {editDraft.uploaded_script_name}</span>
            {:else}
              <span>No uploaded script file selected.</span>
            {/if}
            <button
              type="button"
              class="btn-ghost"
              disabled={!editScriptUpload && !editDraft.uploaded_script_path}
              on:click={clearUploadedScriptSelection}
            >
              Remove upload
            </button>
          </div>
          {#if editDraft.uploaded_script_path}
            <p class="script-upload-path" title={editDraft.uploaded_script_path}>{editDraft.uploaded_script_path}</p>
          {/if}
        {/if}

        <!-- Color picker -->
        <label class="field">
          <span>Node color</span>
          <div class="color-picker">
            {#each NODE_COLORS as color}
              <button type="button"
                class="color-swatch"
                class:color-swatch--active={editDraft.color===color}
                style="--sw:{NODE_COLOR_MAP[color]}"
                on:click={()=>editDraft={...editDraft,color}}
                title={color}
              >
                {#if editDraft.color===color}
                  <span class="color-check">✓</span>
                {/if}
              </button>
            {/each}
          </div>
        </label>

        <section class="links-section">
          <h4>Macros</h4>
          <div class="macro-list">
            {#if editMacroSteps.length===0}
              <p class="muted" style="padding:8px">No macro steps yet.</p>
            {:else}
              {#each editMacroSteps as step, index (index)}
                <div class="macro-row" class:macro-row--shortcut={step.action==='keyboard-shortcut'}>
                  <select
                    value={step.action}
                    on:change={e=>updateMacroAction(index, e.currentTarget.value)}
                  >
                    {#each MACRO_ACTIONS as action}
                      <option value={action}>{macroActionLabel(action)}</option>
                    {/each}
                  </select>

                  {#if step.action==='keyboard-shortcut'}
                    <div class="macro-shortcut-field">
                      <input
                        value={step.value}
                        placeholder={macroValuePlaceholder(step.action)}
                        on:input={e=>updateMacroValue(index, e.currentTarget.value)}
                      />
                      {#if step.value}
                        <div class="macro-shortcut-preview">{formatShortcutSequence(step.value)}</div>
                      {:else if recordingShortcutIndex===index}
                        <div class="macro-shortcut-preview">Recording... press keys, then press next combo for multi-switch.</div>
                      {/if}
                    </div>
                    <button
                      type="button"
                      class="macro-record-btn"
                      class:macro-record-btn--active={recordingShortcutIndex===index}
                      on:click={()=>toggleShortcutRecording(index)}
                    >
                      {recordingShortcutIndex===index ? 'Stop' : 'Record'}
                    </button>
                  {:else}
                    <input
                      value={step.value}
                      placeholder={macroValuePlaceholder(step.action)}
                      disabled={step.action==='run-uploaded-script'}
                      on:input={e=>updateMacroValue(index, e.currentTarget.value)}
                    />
                  {/if}

                  <button type="button" class="btn-danger-xs" on:click={()=>removeMacroStep(index)}>✕</button>
                </div>
              {/each}
            {/if}
          </div>
          <button type="button" class="btn-ghost" on:click={addMacroStep}>＋ Add macro step</button>
        </section>

        <!-- Links -->
        <section class="links-section">
          <h4>Connected nodes</h4>
          <div class="links-list">
            {#each nodes.filter(n=>n.id!==editNode.id) as candidate (candidate.id)}
              <label class="link-row">
                <div class="link-row-dot" style="background:{nodeColor(candidate)}"></div>
                <span>{candidate.name||'Untitled'}</span>
                <input type="checkbox"
                  checked={editSelectedLinks.includes(candidate.id)}
                  on:change={e=>toggleEditLink(candidate.id,e.currentTarget.checked)} />
              </label>
            {/each}
            {#if nodes.filter(n=>n.id!==editNode?.id).length===0}
              <p class="muted" style="padding:8px">No other nodes yet.</p>
            {/if}
          </div>
        </section>

        <footer class="editor-footer">
          <button class="btn-accent" on:click={saveEditor}>Save changes</button>
          <button class="btn-ghost"  on:click={closeEditor}>Cancel</button>
        </footer>
      </section>
    </div>
  {/if}

  <!-- ══════════════════════ CONTEXT MENU ════════════════════════════ -->
  {#if contextMenu.open&&contextNode}
    <div
      class="ctx-menu"
      style="left:{contextMenu.x}px;top:{contextMenu.y}px"
      on:pointerdown|stopPropagation
      in:scale={{ duration: motionDuration(130), start: 0.97 }}
      out:fade={{ duration: motionDuration(90) }}
    >
      <div class="ctx-title">
        <div class="ctx-dot" style="background:{nodeColor(contextNode)}"></div>
        {contextNode.name||'Untitled node'}
      </div>
      <div class="ctx-group">
        <button disabled={!hasLaunchTarget(contextNode,'open-path')}    on:click={()=>launchFromContext('open-path')}>   📁 Open path</button>
        <button disabled={!hasLaunchTarget(contextNode,'open-editor')}  on:click={()=>launchFromContext('open-editor')}> ✏️ Open editor</button>
        <button disabled={!hasLaunchTarget(contextNode,'open-browser')} on:click={()=>launchFromContext('open-browser')}>🌐 Open browser</button>
        <button disabled={!hasLaunchTarget(contextNode,'run-script')}   on:click={()=>launchFromContext('run-script')}>  ▶ Run script</button>
        <button disabled={!hasLaunchTarget(contextNode,'run-macro')}    on:click={()=>launchFromContext('run-macro')}>  ⚡ Run macro</button>
      </div>
      <div class="ctx-sep"></div>
      <div class="ctx-group">
        {#if !isLockedNode(contextNode)}
          <button on:click={openEditorFromMenu}>⚙ Edit node</button>
          <button on:click={()=>{ const id=contextNode.id; closeCtx(); void groupSingleNode(id); }}>⊟ {contextNode.group ? 'Change group…' : 'Add to group…'}</button>
          {#if contextNode.group}
            <button on:click={()=>{ assignNodesToGroup([contextNode.id], ''); closeCtx(); updateStatus('Removed from group'); }}>⊟ Remove from group</button>
          {/if}
        {/if}
        <button on:click={addConnected}>⊕ Add connected</button>
        <button on:click={connectNearest}>⟶ Connect nearest</button>
        <button on:click={clearLinks}>⊘ Clear links</button>
        <button disabled={isLockedNode(contextNode)} on:click={()=>{cloneNode(contextNode.id);closeCtx();}}>⧉ Clone</button>
      </div>
      <div class="ctx-sep"></div>
      <button class="ctx-danger" disabled={isLockedNode(contextNode)}
        on:click={()=>{deleteNode(contextNode.id);closeCtx();}}>✕ Delete node</button>
    </div>
  {/if}

  <!-- ══════════════════════ LAUNCHER ════════════════════════════════ -->
  {#if showLauncher}
    <!-- svelte-ignore a11y-click-events-have-key-events -->
    <div
      class="launcher-overlay"
      role="presentation"
      on:click|self={closeLauncher}
      in:fade={{ duration: motionDuration(140) }}
      out:fade={{ duration: motionDuration(110) }}
    >
      <section
        class="launcher"
        in:scale={{ duration: motionDuration(170), start: 0.97 }}
        out:scale={{ duration: motionDuration(120), start: 0.99 }}
      >
        <div class="launcher-search">
          <span class="launcher-icon">⌘</span>
          <input id="launcher-input" class="launcher-input"
            placeholder="Search nodes, run macros, type a command…"
            bind:value={launcherQuery}
            on:keydown={launcherKey} />
          {#if launcherQuery}
            <button class="launcher-clear" on:click={()=>launcherQuery=''}>✕</button>
          {/if}
        </div>
        {#if !launcherQuery.trim()}
          <div class="launcher-section-label">Recent &amp; commands</div>
        {/if}
        <div class="launcher-list">
          {#each launcherResults as result, i (result.id)}
            <button class="launcher-item"
              class:launcher-item--active={i===launcherIndex}
              class:launcher-item--command={result.type==='command'}
              in:fly={{ y: 6, duration: motionDuration(150), delay: prefersReducedMotion ? 0 : i * 22, opacity: 0 }}
              out:fade={{ duration: motionDuration(80) }}
              on:mouseenter={()=>launcherIndex=i}
              on:click={()=>runLauncherResult(result)}>
              <span class="launcher-item-glyph" style={result.type==='node' ? `color:${result.color}` : ''}>{result.icon}</span>
              <span class="launcher-item-name">{result.title}</span>
              {#if result.subtitle}
                <span class="launcher-item-path">{result.subtitle}</span>
              {/if}
              <span class="launcher-item-kind">{result.type==='command' ? 'cmd' : (result.action ? result.action.replace('open-','').replace('run-','') : '')}</span>
            </button>
          {/each}
          {#if launcherResults.length===0}
            <div class="launcher-empty">No matching nodes or commands</div>
          {/if}
        </div>
        <div class="launcher-hint">
          <kbd>↑↓</kbd> navigate &nbsp; <kbd>↵</kbd> run &nbsp; <kbd>esc</kbd> close
        </div>
      </section>
    </div>
  {/if}

  <!-- ══════════════════════ MACRO PROMPT DIALOG ══════════════════════ -->
  {#if promptDialog.open}
    <div class="modal-overlay prompt-overlay" role="presentation"
      in:fade={{ duration: motionDuration(120) }} out:fade={{ duration: motionDuration(90) }}>
      <section class="prompt-modal"
        in:scale={{ duration: motionDuration(160), start: 0.96 }}
        out:scale={{ duration: motionDuration(110), start: 0.99 }}>
        <header class="prompt-header">
          <span class="prompt-spark">⌨</span>
          <h3>{promptDialog.label}</h3>
        </header>
        <input id="macro-prompt-input" class="prompt-input"
          bind:value={promptDialog.value}
          on:keydown={promptDialogKey}
          placeholder="Type a value…" />
        <footer class="prompt-footer">
          <button class="btn-accent" on:click={confirmPromptDialog}>Continue</button>
          <button class="btn-ghost"  on:click={cancelPromptDialog}>Cancel</button>
        </footer>
      </section>
    </div>
  {/if}

{/if}

<!-- ─────────────────────────────── STYLES ───────────────────────────── -->
<style>
  /* ── Tokens ─────────────────────────────────────────────────────── */
  :global(:root) {
    --bg:      #06090f;
    --s0:      #080d16;
    --s1:      #0d1525;
    --s2:      #121e30;
    --s3:      #192538;
    --text:    #d8ecff;
    --soft:    #6d92b0;
    --dim:     #3c5268;
    --accent:  #5ee7f7;
    --accent2: #b09eff;
    --gold:    #fdd87a;
    --danger:  #ff6478;
    --ok:      #6ee89a;
    --border:  rgba(110,170,220,0.13);
    --bsoft:   rgba(110,170,220,0.07);
    --motion-fast: 120ms;
    --motion-base: 180ms;
    --motion-slow: 260ms;
    --motion-curve: cubic-bezier(.22,1,.36,1);
    --motion-pop: cubic-bezier(.34,1.56,.64,1);
    --r-sm: 8px;  --r-md: 14px;  --r-lg: 20px;  --r-xl: 26px;
    --node-sz: 108px;
    --settings-w: 320px;
  }

  /* ── Reset ──────────────────────────────────────────────────────── */
  * { box-sizing: border-box; }
  :global(html), :global(body), :global(#app) { width:100%; height:100%; margin:0; }
  :global(body) {
    font-family: 'DM Sans', 'Segoe UI', system-ui, sans-serif;
    background: radial-gradient(ellipse at 30% 20%, #0c1e3a 0%, #06090f 65%);
    color: var(--text);
    -webkit-font-smoothing: antialiased;
  }
  :global(html.desktop-overlay-window), :global(body.desktop-mode) { background: transparent; }

  /* ── Settings shell ─────────────────────────────────────────────── */
  .settings-shell {
    width:100%; height:100%;
    display:flex; align-items:flex-start;
    padding:14px; overflow:auto;
  }
  .sidebar {
    width:var(--settings-w);
    display:flex; flex-direction:column; gap:10px;
    overflow:auto;
    padding:10px 8px;
    border-radius:22px;
    border:1px solid rgba(94,231,247,.22);
    background:
      radial-gradient(120% 140% at -20% 0%, rgba(94,231,247,.08) 0%, transparent 58%),
      radial-gradient(130% 140% at 120% 100%, rgba(176,158,255,.08) 0%, transparent 62%),
      rgba(5,8,14,.6);
    box-shadow:
      0 0 0 1px rgba(94,231,247,.08),
      0 0 28px rgba(94,231,247,.2),
      0 0 58px rgba(176,158,255,.16),
      0 18px 42px rgba(0,0,0,.46);
  }

  /* ── Panels ─────────────────────────────────────────────────────── */
  .panel {
    border:1px solid var(--border);
    border-radius:var(--r-lg);
    background:linear-gradient(155deg, var(--s2) 0%, var(--s1) 100%);
    padding:14px;
    box-shadow:0 4px 20px rgba(0,0,0,.25), inset 0 1px 0 rgba(255,255,255,.03);
  }
  .brand-panel {
    padding:16px;
    background:linear-gradient(155deg, var(--s3) 0%, var(--s2) 100%);
    border-color:rgba(94,231,247,.15);
    display:flex; align-items:center; justify-content:space-between;
    position:sticky; top:0; z-index:5;
  }
  .brand-identity {
    display:flex; align-items:center; gap:12px;
    cursor:grab;
    user-select:none;
  }
  .brand-identity:active { cursor:grabbing; }
  .brand-logo-wrap {
    width:42px; height:42px; border-radius:14px; flex-shrink:0;
    display:grid; place-items:center;
    background:linear-gradient(135deg,rgba(94,231,247,.18),rgba(176,158,255,.12));
    border:1px solid rgba(94,231,247,.22);
    box-shadow:0 0 18px rgba(94,231,247,.1);
  }
  .brand-logo { width:26px; height:26px; object-fit:contain; border-radius:6px; }
  .brand-text h1 { margin:0; font-size:1rem; font-weight:700; letter-spacing:.02em; color:var(--text); }
  .brand-text p  { margin:2px 0 0; font-size:.72rem; color:var(--soft); }

  .panel-head {
    display:flex; align-items:center; justify-content:space-between; margin-bottom:10px;
  }
  .panel-head h2 {
    margin:0; font-size:.64rem; font-weight:600;
    text-transform:uppercase; letter-spacing:.14em; color:var(--soft);
  }
  .setting-row {
    display:flex;
    align-items:center;
    justify-content:space-between;
    gap:12px;
    padding:9px 10px;
    border:1px solid var(--bsoft);
    border-radius:var(--r-sm);
    background:rgba(6,9,15,.45);
    font-size:.78rem;
    color:var(--text);
  }
  .setting-list {
    display:flex;
    flex-direction:column;
    gap:8px;
  }
  .setting-row input[type=checkbox] {
    width:16px;
    height:16px;
    accent-color:var(--accent);
    flex-shrink:0;
  }
  .badge {
    font-size:.64rem; font-weight:600; padding:2px 7px;
    border-radius:999px; background:rgba(94,231,247,.08);
    border:1px solid rgba(94,231,247,.18); color:var(--accent);
  }

  /* ── WM buttons ─────────────────────────────────────────────────── */
  .wm-btns { display:flex; gap:5px; }
  .wm-btn {
    width:24px; height:24px; border-radius:50%; font-size:.68rem; font-weight:700;
    display:grid; place-items:center; cursor:pointer;
    transition:all 120ms; padding:0; line-height:1;
  }
  .wm-btn--min {
    background:rgba(254,188,46,.14); color:#febc2e; border:1px solid rgba(254,188,46,.28);
  }
  .wm-btn--min:hover  { background:rgba(254,188,46,.28); }
  .wm-btn--close {
    background:rgba(255,95,87,.14);  color:#ff5f57; border:1px solid rgba(255,95,87,.28);
  }
  .wm-btn--close:hover { background:rgba(255,95,87,.28); }

  /* ── Form elements ──────────────────────────────────────────────── */
  .panel select,
  .panel input:not([type=file]):not([type=checkbox]) {
    width:100%; font:inherit; font-size:.8rem; color:var(--text);
    border-radius:var(--r-sm); border:1px solid var(--border);
    background:rgba(6,9,15,.75); padding:8px 10px; outline:none;
    transition:border-color 150ms, box-shadow 150ms;
  }
  .panel select:focus,
  .panel input:not([type=file]):not([type=checkbox]):focus {
    border-color:rgba(94,231,247,.4);
    box-shadow:0 0 0 3px rgba(94,231,247,.07);
  }

  /* ── Buttons ────────────────────────────────────────────────────── */
  button { font:inherit; cursor:pointer; border-radius:var(--r-sm); transition:all 120ms ease; }
  button:disabled { opacity:.35; cursor:not-allowed; }

  .btn-primary {
    padding:8px 12px; font-size:.78rem; font-weight:600;
    background:linear-gradient(135deg,rgba(94,231,247,.18),rgba(94,231,247,.08));
    border:1px solid rgba(94,231,247,.28); color:var(--accent);
  }
  .btn-primary:hover:not(:disabled) { background:linear-gradient(135deg,rgba(94,231,247,.26),rgba(94,231,247,.14)); }

  .btn-accent {
    padding:8px 14px; font-weight:600;
    background:linear-gradient(135deg,rgba(94,231,247,.22),rgba(94,231,247,.1));
    border:1px solid rgba(94,231,247,.32); color:var(--accent);
  }
  .btn-accent:hover:not(:disabled) { background:linear-gradient(135deg,rgba(94,231,247,.32),rgba(94,231,247,.18)); }

  .btn-ghost {
    padding:8px 12px; font-size:.78rem;
    background:rgba(6,9,15,.5); border:1px solid var(--border); color:var(--soft);
  }
  .btn-ghost:hover:not(:disabled) { border-color:rgba(110,170,220,.3); color:var(--text); }

  .btn-danger {
    width:100%; margin-top:8px; padding:8px 12px; font-size:.78rem;
    background:rgba(255,100,120,.05); border:1px solid rgba(255,100,120,.2); color:#ff8fa0;
  }
  .btn-danger:hover:not(:disabled) { background:rgba(255,100,120,.1); border-color:rgba(255,100,120,.38); }

  .btn-danger-xs {
    padding:5px 7px; font-size:.68rem; border-radius:6px;
    background:rgba(255,100,120,.05); border:1px solid rgba(255,100,120,.18); color:#ff8fa0;
  }
  .btn-danger-xs:hover:not(:disabled) { background:rgba(255,100,120,.12); }

  .icon-btn {
    width:30px; height:30px; padding:0; display:grid; place-items:center;
    font-size:.72rem; background:rgba(6,9,15,.5);
    border:1px solid var(--border); color:var(--soft); border-radius:var(--r-sm);
  }
  .icon-btn:hover { border-color:rgba(110,170,220,.3); color:var(--text); }

  /* ── Layout helpers ─────────────────────────────────────────────── */
  .input-row { display:grid; grid-template-columns:1fr auto; gap:8px; margin-top:8px; }
  .btn-row   { display:grid; grid-template-columns:repeat(3,minmax(0,1fr)); gap:6px; margin-top:8px; }

  /* ── Selection bar ──────────────────────────────────────────────── */
  .sel-bar {
    margin-top:10px; display:grid; grid-template-columns:1fr repeat(3,auto); gap:5px;
    align-items:center; padding:8px; font-size:.72rem; color:var(--accent);
    background:rgba(94,231,247,.04); border:1px solid rgba(94,231,247,.14);
    border-radius:var(--r-sm);
  }
  .sel-bar button { padding:5px 8px; font-size:.7rem; background:rgba(6,9,15,.5); border:1px solid var(--border); color:var(--soft); border-radius:6px; }
  .sel-bar .danger { border-color:rgba(255,100,120,.2); color:#ff8fa0; }

  /* ── Node list (sidebar) ────────────────────────────────────────── */
  .node-list {
    margin-top:10px;
    display:flex;
    flex-direction:column;
    gap:4px;
    max-height:240px;
    overflow:auto;
    transition:opacity var(--motion-base) var(--motion-curve), transform var(--motion-base) var(--motion-curve);
  }
  .node-list--workspace-shift { opacity:.68; transform:translateY(4px); }
  .node-row {
    border:1px solid var(--bsoft); border-radius:var(--r-sm);
    background:rgba(6,9,15,.5); padding:7px 10px;
    display:flex; align-items:center; gap:8px;
    transition:border-color var(--motion-fast) var(--motion-curve), background var(--motion-fast) var(--motion-curve);
  }
  .node-row:hover { background:rgba(94,231,247,.03); border-color:var(--border); }
  .node-row--sel  { border-color:rgba(94,231,247,.28); background:rgba(94,231,247,.04); }
  .node-row-pip  { width:4px; height:26px; border-radius:2px; flex-shrink:0; background:var(--nc); opacity:.75; }
  .node-row-name { flex:1; font-size:.78rem; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; color:var(--text); }
  .node-row-acts { display:flex; gap:4px; flex-shrink:0; }
  .node-row-acts button {
    width:24px; height:24px; padding:0; display:grid; place-items:center; font-size:.68rem;
    background:rgba(6,9,15,.6); border:1px solid var(--border); color:var(--soft); border-radius:6px;
  }
  .node-row-acts button:hover:not(:disabled) { border-color:rgba(94,231,247,.3); color:var(--accent); }

  /* ── Status panel ───────────────────────────────────────────────── */
  .status-panel { display:flex; flex-direction:column; gap:8px; }
  .status-dot { width:8px; height:8px; border-radius:50%; background:var(--dim); flex-shrink:0; }
  .status-dot--ok { background:var(--ok); box-shadow:0 0 6px rgba(110,232,154,.5); }
  .status-msg { margin:0; font-size:.8rem; color:var(--soft); }
  .activity { display:flex; flex-direction:column; gap:4px; max-height:100px; overflow:auto; }
  .activity-item { font-size:.7rem; color:var(--dim); padding:3px 0; border-bottom:1px solid var(--bsoft); line-height:1.3; will-change:transform, opacity; }
  .muted { margin:0; color:var(--dim); font-size:.74rem; }

  /* ════════════════ NODE BOARD ════════════════════════════════════ */
  .nodeboard-shell {
    width:100%; height:100%;
    background:transparent;
    display:grid; place-items:stretch;
  }
  .nodeboard-frame {
    display:flex; flex-direction:column;
    width:100%; height:100%;
    border-radius:20px; overflow:hidden;
    border:1px solid rgba(94,231,247,.15);
    /* Glass: transparent frosted panel */
    background:rgba(6,9,16,0.38);
    backdrop-filter:blur(32px) saturate(1.6) brightness(0.9);
    -webkit-backdrop-filter:blur(32px) saturate(1.6) brightness(0.9);
    box-shadow:
      0 0 0 1px rgba(94,231,247,.06),
      0 40px 80px rgba(0,0,0,.55),
      inset 0 1px 0 rgba(255,255,255,.06),
      inset 0 -1px 0 rgba(0,0,0,.15);
    position:relative;
  }

  /* ── Title bar ───────────────────────────────────────────────────── */
  .nodeboard-titlebar {
    flex-shrink:0; height:34px;
    display:flex; align-items:center; gap:0; padding:0 12px;
    background:rgba(6,10,18,.45);
    border-bottom:1px solid rgba(94,231,247,.08);
    border-radius:20px 20px 0 0;
    cursor:grab; user-select:none;
  }
  .nodeboard-titlebar:active { cursor:grabbing; }
  .tb-label {
    flex:1; text-align:center; font-size:.66rem; font-weight:600;
    letter-spacing:.14em; color:rgba(180,210,240,.3);
    text-transform:uppercase; pointer-events:none;
  }
  .tb-actions { display:flex; gap:4px; flex-shrink:0; }
  .tb-btn {
    width:26px; height:26px; border-radius:8px; border:1px solid var(--border);
    background:rgba(12,20,32,.7); color:var(--soft); font-size:.8rem;
    display:grid; place-items:center; cursor:pointer;
    transition:all 120ms;
  }
  .tb-btn:hover { border-color:rgba(94,231,247,.3); color:var(--accent); background:rgba(94,231,247,.06); }

  /* ── Canvas ──────────────────────────────────────────────────────── */
  .canvas {
    position:relative; flex:1; min-height:0;
    overflow:hidden;
    background:transparent;
  }
  .canvas--panning,
  .canvas--panning * {
    cursor:grabbing !important;
  }

  .canvas-background {
    position:absolute;
    inset:0;
    pointer-events:none;
    z-index:0;
    opacity:var(--board-bg-opacity, 1);
  }

  .canvas-glow {
    position:absolute;
    inset:0;
    background:
      radial-gradient(ellipse at 18% 12%, rgba(94,231,247,.04) 0%, transparent 50%),
      radial-gradient(ellipse at 82% 88%, rgba(176,158,255,.03) 0%, transparent 50%),
      transparent;
  }

  /* Board background patterns (theme-aware, switch via data-board-bg) */
  .canvas-dots {
    position:absolute; inset:0; pointer-events:none;
    opacity:.30;
    background-image:radial-gradient(circle, var(--accent) 1px, transparent 1px);
    background-size:28px 28px;
    transition:opacity var(--motion-base) var(--motion-curve), background-size var(--motion-base) var(--motion-curve);
  }
  .canvas[data-board-bg="grid"] .canvas-dots {
    opacity:.16;
    background-image:
      linear-gradient(var(--accent) 1px, transparent 1px),
      linear-gradient(90deg, var(--accent) 1px, transparent 1px);
    background-size:34px 34px, 34px 34px;
  }
  .canvas[data-board-bg="aurora"] .canvas-dots {
    opacity:.5;
    background-image:
      radial-gradient(40% 50% at 20% 18%, var(--accent) 0%, transparent 60%),
      radial-gradient(40% 50% at 82% 82%, var(--accent2) 0%, transparent 60%);
    background-size:cover, cover;
    filter:blur(8px);
  }
  .canvas[data-board-bg="void"] .canvas-dots { opacity:0; }
  .canvas[data-board-bg="aurora"] .canvas-glow { opacity:.6; }

  /* Drop-to-create overlay */
  .canvas--drop { outline:2px dashed var(--accent); outline-offset:-10px; }
  .drop-overlay {
    position:absolute; inset:0; z-index:40; pointer-events:none;
    display:grid; place-items:center;
    background:radial-gradient(ellipse at center, var(--accent-glow, rgba(94,231,247,.18)) 0%, transparent 70%);
  }
  .drop-overlay-inner {
    display:flex; flex-direction:column; align-items:center; gap:8px;
    padding:22px 30px; border-radius:var(--r-lg);
    border:1.5px dashed var(--accent); background:rgba(6,9,15,.7);
    color:var(--accent); font-size:.84rem; font-weight:600; letter-spacing:.02em;
    box-shadow:0 0 40px var(--accent-glow, rgba(94,231,247,.25));
  }
  .drop-overlay-icon { font-size:1.8rem; line-height:1; }

  /* zoom-root: scaled container for both SVG + nodes */
  .zoom-root {
    position:absolute; inset:0;
    width:100%; height:100%;
    transform-origin:0 0;
    will-change:transform, opacity;
    transition:opacity var(--motion-base) var(--motion-curve);
    z-index:1;
  }
  .zoom-root--workspace-shift { opacity:.74; }

  /* ── SVG links ───────────────────────────────────────────────────── */
  .links { position:absolute; inset:0; pointer-events:none; overflow:visible; }
  .link {
    fill:none; stroke:rgba(94,231,247,.2); stroke-width:1.5;
    transition:stroke var(--motion-base) var(--motion-curve), stroke-width var(--motion-base) var(--motion-curve);
    filter:drop-shadow(0 0 3px rgba(94,231,247,.15));
  }
  .link--highlight {
    stroke:rgba(94,231,247,.7); stroke-width:2.5;
    filter:drop-shadow(0 0 6px rgba(94,231,247,.4));
  }

  /* ═══════════════════ NODE CIRCLES ═══════════════════════════════ */
  .node {
    position:absolute;
    width:var(--node-sz); height:var(--node-sz);
    border-radius:50%;
    display:flex; align-items:center; justify-content:center;
    cursor:grab; user-select:none;
    /* Layered depth effect */
    background:
      radial-gradient(circle at 28% 22%, rgba(255,255,255,.1) 0%, transparent 50%),
      linear-gradient(180deg, var(--s3) 0%, var(--s1) 100%);
    border:1.5px solid rgba(110,165,200,.2);
    box-shadow:
      0 8px 28px rgba(0,0,0,.4),
      0 2px 8px rgba(0,0,0,.25),
      inset 0 1px 0 rgba(255,255,255,.06),
      inset 0 -1px 0 rgba(0,0,0,.18);
    transition:
      transform var(--motion-base) var(--motion-pop),
      box-shadow var(--motion-base) var(--motion-curve),
      border-color var(--motion-base) var(--motion-curve);
    transform-origin:center center;
    will-change:left,top,transform;
    overflow:visible; /* allow action buttons to pop out */
    z-index:1;
  }
  /* Ring glow */
  .node-ring {
    position:absolute; inset:-4px; border-radius:50%;
    border:1.5px solid transparent;
    transition:all var(--motion-slow) var(--motion-curve); pointer-events:none;
    background:transparent;
  }
  .node--hovered .node-ring {
    border-color:var(--nc);
    box-shadow:0 0 20px color-mix(in srgb, var(--nc) 55%, transparent);
    opacity:.6;
  }
  .node--hovered {
    transform:scale(1.07);
    z-index:3;
    border-color:rgba(var(--nc),.4);
    box-shadow:
      0 14px 36px rgba(0,0,0,.5),
      0 0 0 1.5px var(--nc),
      0 0 26px color-mix(in srgb, var(--nc) 22%, transparent),
      inset 0 1px 0 rgba(255,255,255,.08);
  }
  .node--selected {
    border-color:var(--accent);
    box-shadow:
      0 10px 30px rgba(0,0,0,.4),
      0 0 0 2px rgba(94,231,247,.55),
      0 0 24px rgba(94,231,247,.15),
      inset 0 1px 0 rgba(255,255,255,.06);
  }
  .node--main {
    border-color:rgba(253,216,122,.5);
    background:
      radial-gradient(circle at 28% 22%, rgba(253,216,122,.2) 0%, transparent 55%),
      linear-gradient(180deg, rgba(40,30,12,.95) 0%, rgba(22,16,8,.9) 100%);
    box-shadow:
      0 10px 30px rgba(0,0,0,.4),
      0 0 0 1px rgba(253,216,122,.25),
      0 0 28px rgba(253,216,122,.12),
      inset 0 1px 0 rgba(255,255,255,.06);
  }
  .node--main .node-name { color:var(--gold); font-weight:700; }
  .node--dragging {
    cursor:grabbing; z-index:10;
    transform:scale(1.08) rotate(1.5deg);
    box-shadow:
      0 24px 56px rgba(0,0,0,.6),
      0 0 0 2px rgba(94,231,247,.4),
      inset 0 1px 0 rgba(255,255,255,.07);
  }
  .node--expanded {
    transform:scale(1.15); z-index:5;
    border-color:rgba(94,231,247,.55);
    box-shadow:
      0 18px 44px rgba(0,0,0,.55),
      0 0 0 2px rgba(94,231,247,.45),
      0 0 36px rgba(94,231,247,.12),
      inset 0 1px 0 rgba(255,255,255,.07);
  }

  /* ── Node inner ─────────────────────────────────────────────────── */
  .node-inner {
    display:flex; flex-direction:column; align-items:center; justify-content:center;
    gap:3px; width:100%; height:100%;
    position:relative; z-index:1;
  }
  .node-top-row {
    display:flex; align-items:center; justify-content:space-between;
    width:100%; padding:0 8px; position:absolute; top:10px;
  }
  .node-dot { width:6px; height:6px; border-radius:50%; background:rgba(110,165,200,.25); flex-shrink:0; }
  .node-dot--active { background:var(--ok);   box-shadow:0 0 6px rgba(110,232,154,.55); }
  .node-dot--core   { background:var(--gold); box-shadow:0 0 8px rgba(253,216,122,.65); }
  .node-edit-btn {
    width:16px; height:16px; padding:0; display:grid; place-items:center;
    font-size:.78rem; line-height:1; border-radius:4px;
    background:rgba(6,9,15,.7); border:1px solid rgba(110,165,200,.18); color:var(--soft);
    opacity:0; transition:opacity var(--motion-fast), border-color var(--motion-fast), color var(--motion-fast);
  }
  .node:hover .node-edit-btn { opacity:1; }
  .node-edit-btn:hover { border-color:rgba(94,231,247,.4); color:var(--accent); }

  .node-icon {
    width:38px; height:38px; border-radius:50%;
    display:grid; place-items:center; font-size:1rem; font-weight:700;
    color:var(--nc); background:rgba(6,9,15,.85);
    border:1px solid rgba(110,165,200,.14);
    box-shadow:0 0 14px color-mix(in srgb, var(--nc) 22%, transparent);
    text-shadow:0 0 10px var(--nc);
    flex-shrink:0;
  }
  .node-logo { width:36px; height:36px; border-radius:50%; object-fit:contain; padding:6px; background:rgba(6,9,15,.7); border:1px solid rgba(110,165,200,.14); flex-shrink:0; }
  .node-icon-img { width:40px; height:40px; border-radius:50%; object-fit:cover; border:1.5px solid rgba(110,165,200,.2); flex-shrink:0; }
  .node-name {
    font-size:.62rem; font-weight:500; letter-spacing:.02em; text-align:center;
    max-width:88px; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;
    color:var(--soft); line-height:1;
  }
  .node-desc {
    position:absolute; bottom:-32px; left:50%; transform:translateX(-50%);
    white-space:nowrap; font-size:.58rem; color:var(--dim);
    background:rgba(6,9,15,.9); border:1px solid var(--border);
    border-radius:6px; padding:3px 8px; pointer-events:none; z-index:6;
  }

  /* ── Action buttons (click-expanded bubble) ──────────────────────── */
  .node-actions {
    position:absolute;
    top:-54px; left:50%; transform:translateX(-50%);
    display:flex; align-items:center; gap:4px;
    padding:6px 8px;
    background:rgba(6,10,18,.92);
    border:1px solid rgba(94,231,247,.22);
    border-radius:999px;
    box-shadow:
      0 12px 32px rgba(0,0,0,.5),
      0 0 0 1px rgba(94,231,247,.06),
      inset 0 1px 0 rgba(255,255,255,.06);
    backdrop-filter:blur(16px);
    z-index:20;
    white-space:nowrap;
    pointer-events:auto;
    animation:actions-pop .2s var(--motion-pop) both;
  }
  @keyframes actions-pop {
    from { transform:translateX(-50%) scale(.75); opacity:0; }
    to   { transform:translateX(-50%) scale(1);   opacity:1; }
  }
  .action-btn {
    display:flex; align-items:center; gap:4px;
    padding:5px 10px; border-radius:999px;
    border:1px solid transparent;
    background:rgba(255,255,255,.04);
    color:var(--soft); font-size:.65rem; font-weight:500;
    cursor:pointer; transition:all var(--motion-fast) var(--motion-curve);
  }
  .action-btn:hover {
    background:rgba(94,231,247,.1);
    border-color:rgba(94,231,247,.3); color:var(--accent);
    transform:translateY(-1px);
    box-shadow:0 4px 12px rgba(94,231,247,.12);
  }
  .action-btn--path:hover   { color:#6ee89a; border-color:rgba(110,232,154,.3); background:rgba(110,232,154,.08); }
  .action-btn--editor:hover { color:#b09eff; border-color:rgba(176,158,255,.3); background:rgba(176,158,255,.08); }
  .action-btn--browser:hover{ color:#5ee7f7; border-color:rgba(94,231,247,.3);  background:rgba(94,231,247,.08); }
  .action-btn--script:hover { color:#fdd87a; border-color:rgba(253,216,122,.3); background:rgba(253,216,122,.08); }
  .action-btn--macro:hover  { color:#c4a8ff; border-color:rgba(196,168,255,.3); background:rgba(196,168,255,.08); }
  .action-btn--edit:hover   { color:#8fa3b5; border-color:rgba(143,163,181,.3); background:rgba(143,163,181,.08); }
  .action-label { font-size:.6rem; letter-spacing:.02em; }

  /* ── Zoom controls ───────────────────────────────────────────────── */
  .zoom-controls {
    position:absolute; bottom:14px; right:14px; z-index:10;
    display:flex; align-items:center; gap:4px;
    background:rgba(6,10,18,.72); backdrop-filter:blur(10px);
    border:1px solid rgba(94,231,247,.16); border-radius:999px;
    padding:4px 8px;
    box-shadow:0 4px 18px rgba(0,0,0,.35);
  }
  .zoom-btn {
    width:22px; height:22px; border-radius:50%; font-size:.9rem;
    display:grid; place-items:center; cursor:pointer; padding:0;
    background:rgba(16,28,44,.8); border:1px solid rgba(110,165,200,.18); color:var(--soft);
    transition:all var(--motion-fast) var(--motion-curve); line-height:1;
  }
  .zoom-btn:hover:not(:disabled) { border-color:rgba(94,231,247,.4); color:var(--accent); background:rgba(94,231,247,.08); }
  .zoom-btn:disabled { opacity:.28; cursor:not-allowed; }
  .zoom-pct { font-size:.62rem; font-weight:600; color:var(--dim); min-width:34px; text-align:center; letter-spacing:.04em; }
  .zoom-divider { width:1px; height:14px; background:var(--border); margin:0 2px; }

  .board-left-controls {
    position:absolute;
    left:12px;
    bottom:34px;
    z-index:11;
    pointer-events:none;
  }

  /* ── Status strip ────────────────────────────────────────────────── */
  .nb-status {
    flex-shrink:0; height:26px;
    display:grid;
    grid-template-columns:minmax(0,1fr) minmax(0,1fr);
    align-items:center;
    gap:8px;
    padding:0 14px;
    background:rgba(4,7,12,.45);
    border-top:1px solid rgba(94,231,247,.06);
    border-radius:0 0 20px 20px;
  }
  .nb-status-left,
  .nb-status-right {
    display:flex;
    align-items:center;
    gap:8px;
    min-width:0;
  }
  .nb-status-right { justify-content:flex-end; }
  .nb-status-dot {
    width:6px; height:6px; border-radius:50%;
    background:var(--ok); box-shadow:0 0 5px rgba(110,232,154,.5);
    animation:nb-pulse 2s ease-in-out infinite; flex-shrink:0;
    transition:transform var(--motion-fast) var(--motion-curve), box-shadow var(--motion-fast) var(--motion-curve);
  }
  .nb-status-dot--flash {
    transform:scale(1.18);
    box-shadow:0 0 12px rgba(110,232,154,.65);
  }
  @keyframes nb-pulse { 0%,100%{opacity:.4;transform:scale(.8);} 50%{opacity:1;transform:scale(1.1);} }
  .nb-status-text  { flex:1; min-width:0; font-size:.62rem; color:var(--dim); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
  .nb-opacity {
    display:flex;
    align-items:center;
    gap:6px;
    flex-shrink:0;
    min-width:184px;
    padding:7px 10px;
    border-radius:999px;
    border:1px solid rgba(94,231,247,.24);
    background:rgba(6,10,18,.78);
    box-shadow:
      0 6px 20px rgba(0,0,0,.35),
      0 0 0 1px rgba(94,231,247,.08),
      0 0 18px rgba(94,231,247,.16);
    pointer-events:auto;
  }
  .nb-opacity-label,
  .nb-opacity-value {
    font-size:.58rem;
    color:var(--dim);
    letter-spacing:.04em;
    text-transform:uppercase;
    opacity:.85;
  }
  .nb-opacity-value {
    min-width:34px;
    text-align:right;
  }
  .nb-opacity-slider {
    appearance:none;
    width:102px;
    height:3px;
    border-radius:999px;
    background:rgba(94,231,247,.24);
    outline:none;
  }
  .nb-opacity-slider::-webkit-slider-thumb {
    appearance:none;
    width:10px;
    height:10px;
    border-radius:50%;
    border:1px solid rgba(94,231,247,.55);
    background:rgba(10,22,38,.95);
    box-shadow:0 0 0 1px rgba(94,231,247,.18), 0 0 10px rgba(94,231,247,.22);
    cursor:pointer;
  }
  .nb-opacity-slider::-moz-range-thumb {
    width:10px;
    height:10px;
    border-radius:50%;
    border:1px solid rgba(94,231,247,.55);
    background:rgba(10,22,38,.95);
    box-shadow:0 0 0 1px rgba(94,231,247,.18), 0 0 10px rgba(94,231,247,.22);
    cursor:pointer;
  }
  .nb-status-count { font-size:.62rem; color:var(--dim); flex-shrink:0; opacity:.6; }

  /* ════════════════ MODALS ══════════════════════════════════════════ */
  .modal-overlay, .launcher-overlay {
    position:fixed; inset:0;
    background:rgba(3,6,12,.7);
    backdrop-filter:blur(6px);
    display:grid; place-items:center; z-index:50; padding:20px;
    will-change:opacity;
  }

  /* ── Editor modal ────────────────────────────────────────────────── */
  .editor-modal {
    width:min(500px,96vw); max-height:90vh; overflow:auto;
    border-radius:var(--r-xl);
    border:1px solid rgba(110,165,200,.2);
    background:linear-gradient(155deg, var(--s3) 0%, var(--s2) 40%, var(--s1) 100%);
    padding:22px; display:flex; flex-direction:column; gap:14px;
    box-shadow:0 40px 80px rgba(0,0,0,.55), 0 0 0 1px rgba(255,255,255,.025);
    will-change:transform, opacity;
  }
  .editor-header {
    display:flex; align-items:center; justify-content:space-between;
  }
  .editor-title-row { display:flex; align-items:center; gap:10px; }
  .editor-preview-dot {
    width:14px; height:14px; border-radius:50%; flex-shrink:0;
    transition:background 200ms, box-shadow 200ms;
  }
  .editor-modal h3 { margin:0; font-size:1rem; font-weight:700; letter-spacing:.02em; }
  .editor-grid { display:grid; grid-template-columns:1fr 1fr; gap:10px; }
  .field { display:flex; flex-direction:column; gap:5px; }
  .field span {
    font-size:.62rem; font-weight:600; text-transform:uppercase;
    letter-spacing:.1em; color:var(--dim);
  }
  .field-toggle-row {
    display:flex;
    align-items:flex-start;
    gap:10px;
    padding:8px 10px;
    border-radius:var(--r-sm);
    border:1px solid var(--border);
    background:rgba(6,9,15,.58);
  }
  .field-toggle-row input[type=checkbox] {
    width:16px;
    height:16px;
    margin-top:1px;
    flex-shrink:0;
    accent-color:var(--accent);
  }
  .field-toggle-row small {
    font-size:.7rem;
    line-height:1.35;
    color:var(--soft);
  }
  .field input:not([type=file]):not([type=checkbox]),
  .field select,
  .field textarea {
    width:100%; font:inherit; font-size:.8rem; color:var(--text);
    border-radius:var(--r-sm); border:1px solid var(--border);
    background:rgba(6,9,15,.75); padding:8px 10px; outline:none;
    transition:border-color 150ms, box-shadow 150ms;
  }
  .field input:focus, .field select:focus, .field textarea:focus {
    border-color:rgba(94,231,247,.38); box-shadow:0 0 0 3px rgba(94,231,247,.06);
  }
  .field input[type=file] {
    font:inherit; font-size:.75rem; color:var(--soft);
    background:rgba(6,9,15,.5); border:1px dashed var(--border);
    border-radius:var(--r-sm); padding:8px; width:100%;
  }
  .field-group { display:flex; flex-direction:column; gap:8px; }
  .script-upload-info {
    display:flex;
    align-items:center;
    justify-content:space-between;
    gap:10px;
    border:1px dashed rgba(110,165,200,.2);
    border-radius:var(--r-sm);
    background:rgba(6,9,15,.4);
    padding:8px 10px;
    font-size:.74rem;
    color:var(--soft);
  }
  .script-upload-path {
    margin:0;
    font-size:.7rem;
    color:var(--dim);
    white-space:nowrap;
    overflow:hidden;
    text-overflow:ellipsis;
  }
  .icon-preview {
    display:flex; align-items:center; gap:12px; padding:10px;
    background:rgba(94,231,247,.03); border:1px dashed rgba(94,231,247,.2); border-radius:var(--r-md);
  }
  .icon-preview-img { width:46px; height:46px; border-radius:50%; object-fit:cover; border:1.5px solid rgba(110,165,200,.25); }

  /* Color picker */
  .color-picker { display:flex; gap:9px; flex-wrap:wrap; align-items:center; }
  .color-swatch {
    width:28px; height:28px; border-radius:50%; flex-shrink:0;
    background:var(--sw); border:2px solid transparent;
    cursor:pointer; padding:0; position:relative;
    transition:transform 120ms, border-color 120ms, box-shadow 120ms;
    box-shadow:0 0 10px color-mix(in srgb, var(--sw) 40%, transparent);
  }
  .color-swatch:hover { transform:scale(1.18); }
  .color-swatch--active { border-color:rgba(255,255,255,.75); transform:scale(1.14); }
  .color-check {
    position:absolute; inset:0; display:grid; place-items:center;
    font-size:.55rem; color:rgba(0,0,0,.7); font-weight:700;
  }

  /* Links section */
  .links-section { border-top:1px solid var(--bsoft); padding-top:12px; }
  .links-section h4 {
    margin:0 0 8px; font-size:.62rem; font-weight:600;
    text-transform:uppercase; letter-spacing:.1em; color:var(--dim);
  }
  .links-list { display:flex; flex-direction:column; gap:4px; max-height:150px; overflow:auto; }
  .macro-list {
    display:flex;
    flex-direction:column;
    gap:6px;
    margin-bottom:8px;
    max-height:168px;
    overflow:auto;
  }
  .macro-row {
    display:grid;
    grid-template-columns:minmax(0,148px) minmax(0,1fr) auto;
    gap:6px;
    align-items:center;
    border:1px solid var(--bsoft);
    border-radius:var(--r-sm);
    padding:6px;
    background:rgba(6,9,15,.45);
  }
  .macro-row--shortcut {
    grid-template-columns:minmax(0,148px) minmax(0,1fr) auto auto;
  }
  .macro-row select,
  .macro-row input {
    width:100%;
    font:inherit;
    font-size:.74rem;
    color:var(--text);
    border-radius:6px;
    border:1px solid var(--border);
    background:rgba(6,9,15,.75);
    padding:6px 8px;
    outline:none;
  }
  .macro-row select:focus,
  .macro-row input:focus {
    border-color:rgba(94,231,247,.38);
    box-shadow:0 0 0 2px rgba(94,231,247,.06);
  }
  .macro-shortcut-field {
    display:flex;
    flex-direction:column;
    gap:4px;
    min-width:0;
  }
  .macro-shortcut-preview {
    font-size:.66rem;
    color:var(--accent);
    white-space:nowrap;
    overflow:hidden;
    text-overflow:ellipsis;
    padding:2px 4px;
    border-radius:6px;
    background:rgba(94,231,247,.07);
    border:1px solid rgba(94,231,247,.16);
    letter-spacing:.01em;
  }
  .macro-record-btn {
    min-width:62px;
    padding:6px 8px;
    font-size:.68rem;
    border-radius:6px;
    border:1px solid rgba(94,231,247,.25);
    background:rgba(94,231,247,.08);
    color:var(--accent);
  }
  .macro-record-btn:hover {
    border-color:rgba(94,231,247,.42);
    background:rgba(94,231,247,.14);
  }
  .macro-record-btn--active {
    color:#fff;
    background:rgba(110,232,154,.16);
    border-color:rgba(110,232,154,.45);
    box-shadow:0 0 14px rgba(110,232,154,.24);
  }
  .link-row {
    display:flex; align-items:center; gap:9px; font-size:.78rem;
    border:1px solid var(--bsoft); border-radius:var(--r-sm); padding:7px 10px;
    background:rgba(6,9,15,.5); cursor:pointer; transition:border-color 120ms;
  }
  .link-row:hover { border-color:var(--border); }
  .link-row-dot { width:8px; height:8px; border-radius:50%; flex-shrink:0; }
  .link-row span { flex:1; }
  .link-row input[type=checkbox] { width:14px; height:14px; accent-color:var(--accent); }
  .editor-footer {
    display:flex; align-items:center; gap:8px; justify-content:flex-end;
    border-top:1px solid var(--bsoft); padding-top:14px; margin-top:2px;
  }

  /* ── Context menu ────────────────────────────────────────────────── */
  .ctx-menu {
    position:fixed; z-index:60; width:218px;
    border:1px solid rgba(110,165,200,.18); border-radius:var(--r-lg);
    padding:8px;
    background:linear-gradient(155deg, var(--s3) 0%, var(--s2) 100%);
    box-shadow:0 24px 56px rgba(0,0,0,.55), 0 0 0 1px rgba(255,255,255,.025);
    backdrop-filter:blur(20px);
    will-change:transform, opacity;
  }
  .ctx-title {
    font-size:.74rem; font-weight:600; color:var(--soft);
    padding:4px 6px 8px; display:flex; align-items:center; gap:8px;
    border-bottom:1px solid var(--bsoft); margin-bottom:5px;
  }
  .ctx-dot { width:8px; height:8px; border-radius:50%; flex-shrink:0; }
  .ctx-group { display:flex; flex-direction:column; gap:2px; }
  .ctx-sep { height:1px; background:var(--bsoft); margin:5px 0; }
  .ctx-menu button {
    width:100%; text-align:left; font-size:.74rem;
    padding:7px 9px; border-radius:var(--r-sm);
    border:1px solid transparent; background:transparent; color:var(--soft);
    transition:all 100ms;
  }
  .ctx-menu button:hover:not(:disabled) { background:rgba(94,231,247,.06); border-color:rgba(94,231,247,.12); color:var(--text); }
  .ctx-menu button:disabled { opacity:.32; }
  .ctx-danger { color:#ff8fa0 !important; }
  .ctx-danger:hover:not(:disabled) { background:rgba(255,100,120,.08) !important; border-color:rgba(255,100,120,.2) !important; }

  /* ── Launcher ────────────────────────────────────────────────────── */
  .launcher {
    width:min(540px,94vw); border-radius:var(--r-xl);
    border:1px solid rgba(110,165,200,.18);
    background:linear-gradient(155deg, var(--s3) 0%, var(--s2) 100%);
    padding:16px; display:flex; flex-direction:column; gap:10px;
    box-shadow:0 40px 80px rgba(0,0,0,.6), 0 0 0 1px rgba(255,255,255,.025);
    backdrop-filter:blur(24px);
    will-change:transform, opacity;
  }
  .launcher-search {
    display:flex; align-items:center; gap:10px;
    background:rgba(6,9,15,.75); border:1px solid rgba(94,231,247,.22);
    border-radius:var(--r-md); padding:10px 14px;
    box-shadow:0 0 0 3px rgba(94,231,247,.06);
  }
  .launcher-icon { font-size:.9rem; color:var(--accent); opacity:.5; flex-shrink:0; }
  .launcher-input {
    flex:1; border:none; background:transparent; font:inherit;
    font-size:.88rem; color:var(--text); outline:none;
  }
  .launcher-input::placeholder { color:var(--dim); }
  .launcher-clear { background:none; border:none; color:var(--dim); font-size:.72rem; padding:2px; cursor:pointer; border-radius:4px; }
  .launcher-clear:hover { color:var(--text); }
  .launcher-list { display:flex; flex-direction:column; gap:3px; max-height:300px; overflow:auto; }
  .launcher-item {
    width:100%; display:flex; align-items:center; gap:10px; text-align:left;
    padding:10px 12px; border-radius:var(--r-sm);
    border:1px solid transparent; background:rgba(6,9,15,.4); color:var(--text);
    cursor:pointer; transition:all var(--motion-fast) var(--motion-curve);
  }
  .launcher-item:hover, .launcher-item--active { background:rgba(94,231,247,.06); border-color:rgba(94,231,247,.16); }
  .launcher-item-name { flex:1; font-size:.82rem; font-weight:500; }
  .launcher-item-path { font-size:.68rem; color:var(--dim); max-width:180px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .launcher-empty { padding:18px; font-size:.78rem; color:var(--dim); text-align:center; }
  .launcher-hint {
    display:flex; gap:10px; justify-content:center;
    font-size:.64rem; color:var(--dim);
    border-top:1px solid var(--bsoft); padding-top:10px;
  }
  .launcher-hint kbd {
    padding:2px 6px; border:1px solid var(--bsoft);
    border-radius:5px; background:rgba(6,9,15,.5); font-family:inherit;
  }

  /* ── Fatal error ─────────────────────────────────────────────────── */
  .fatal { margin:0; padding:24px; color:#ff9aaa; white-space:pre-wrap; font-size:.8rem; }

  /* ── Scrollbars ──────────────────────────────────────────────────── */
  :global(*::-webkit-scrollbar) { width:4px; height:4px; }
  :global(*::-webkit-scrollbar-track) { background:transparent; }
  :global(*::-webkit-scrollbar-thumb) { background:rgba(110,165,200,.18); border-radius:2px; }
  :global(*::-webkit-scrollbar-thumb:hover) { background:rgba(110,165,200,.32); }

  @media (prefers-reduced-motion:reduce) {
    .node-actions,
    .nb-status-dot {
      animation:none;
    }
  }

  /* ── Responsive ──────────────────────────────────────────────────── */
  @media (max-width:980px) {
    .settings-shell  { padding:10px; }
    .sidebar         { width:100%; }
    .editor-grid     { grid-template-columns:1fr; }
    .btn-row         { grid-template-columns:1fr 1fr; }
    .sel-bar         { grid-template-columns:1fr 1fr; }
    .nodeboard-frame { border-radius:12px; }
    .board-left-controls {
      left:10px;
      bottom:34px;
    }
    .nb-status {
      grid-template-columns:minmax(0,1fr) auto;
    }
    .nb-status-right {
      display:none;
    }
    .nb-opacity      { min-width:130px; gap:4px; padding:6px 8px; }
    .nb-opacity-label{ display:none; }
    .nb-opacity-slider { width:78px; }
    .script-upload-info {
      flex-direction:column;
      align-items:flex-start;
    }
  }

  /* ════════════════ NEW FEATURE STYLES ════════════════════════════ */
  .hidden-file { display:none; }
  .field-label {
    display:block; font-size:.6rem; font-weight:600; text-transform:uppercase;
    letter-spacing:.13em; color:var(--soft); margin-bottom:7px;
  }
  .btn-pair { display:grid; grid-template-columns:1fr 1fr; gap:6px; margin-top:8px; }

  /* Appearance: theme swatches + board background */
  .theme-swatches { display:flex; gap:8px; flex-wrap:wrap; }
  .theme-swatch {
    width:30px; height:30px; border-radius:9px; padding:0; cursor:pointer;
    border:1px solid var(--border);
    background:linear-gradient(135deg, var(--ta) 0%, var(--ta) 50%, var(--tb) 50%, var(--tb) 100%);
    box-shadow:0 0 0 0 transparent; transition:transform 120ms, box-shadow 160ms, border-color 160ms;
  }
  .theme-swatch:hover { transform:translateY(-1px); }
  .theme-swatch--active {
    border-color:var(--ta);
    box-shadow:0 0 0 2px rgba(6,9,15,.9), 0 0 0 4px var(--ta), 0 0 14px var(--ta);
  }
  .bg-options { display:grid; grid-template-columns:repeat(4,1fr); gap:6px; }
  .bg-option {
    padding:7px 4px; font-size:.68rem; color:var(--soft);
    background:rgba(6,9,15,.5); border:1px solid var(--border); border-radius:var(--r-sm);
  }
  .bg-option:hover { color:var(--text); border-color:rgba(110,170,220,.3); }
  .bg-option--active {
    color:var(--accent); border-color:rgba(94,231,247,.4);
    background:linear-gradient(135deg, rgba(94,231,247,.16), rgba(94,231,247,.06));
  }

  /* Insights */
  .insights-stats { display:grid; grid-template-columns:repeat(3,1fr); gap:8px; }
  .insight-stat {
    display:flex; flex-direction:column; align-items:center; gap:2px;
    padding:10px 6px; border:1px solid var(--bsoft); border-radius:var(--r-sm);
    background:rgba(6,9,15,.45);
  }
  .insight-val { font-size:1.15rem; font-weight:700; color:var(--accent); line-height:1; font-variant-numeric:tabular-nums; }
  .insight-lbl { font-size:.58rem; text-transform:uppercase; letter-spacing:.12em; color:var(--soft); }
  .most-used { display:flex; flex-direction:column; gap:5px; }
  .most-used-row {
    display:flex; align-items:center; gap:9px; padding:7px 9px;
    border:1px solid var(--bsoft); border-radius:var(--r-sm);
    background:rgba(6,9,15,.45); color:var(--text); text-align:left;
  }
  .most-used-row:hover { border-color:rgba(94,231,247,.28); background:rgba(94,231,247,.05); }
  .most-used-pip { width:8px; height:8px; border-radius:50%; background:var(--nc); box-shadow:0 0 8px var(--nc); flex-shrink:0; }
  .most-used-name { flex:1; font-size:.78rem; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .most-used-count { font-size:.7rem; font-weight:700; color:var(--accent); font-variant-numeric:tabular-nums; }

  /* Group hulls + chips on the board */
  .group-hull {
    position:absolute; border-radius:18px; pointer-events:none; z-index:0;
    border:1.5px dashed color-mix(in srgb, var(--gc) 55%, transparent);
    background:color-mix(in srgb, var(--gc) 8%, transparent);
    box-shadow:inset 0 0 30px color-mix(in srgb, var(--gc) 10%, transparent);
  }
  .group-hull-label {
    position:absolute; top:-2px; left:14px;
    display:inline-flex; align-items:center; gap:6px; pointer-events:auto;
    padding:3px 10px; border-radius:999px; cursor:pointer;
    font-size:.66rem; font-weight:700; letter-spacing:.04em;
    color:var(--gc);
    background:rgba(6,9,15,.85); border:1px solid color-mix(in srgb, var(--gc) 45%, transparent);
  }
  .group-hull-dot { width:7px; height:7px; border-radius:50%; background:var(--gc); box-shadow:0 0 8px var(--gc); }
  .group-hull-toggle { opacity:.7; font-size:.6rem; }
  .group-chip {
    position:absolute; z-index:3; transform:translate(-12px,-6px);
    display:inline-flex; align-items:center; gap:8px; cursor:pointer;
    padding:9px 14px; border-radius:14px;
    color:var(--gc); font-size:.8rem; font-weight:700;
    background:rgba(8,12,20,.92);
    border:1px solid color-mix(in srgb, var(--gc) 50%, transparent);
    box-shadow:0 6px 22px rgba(0,0,0,.5), 0 0 18px color-mix(in srgb, var(--gc) 22%, transparent);
  }
  .group-chip:hover { transform:translate(-12px,-8px); }
  .group-chip-icon { font-size:.7rem; }
  .group-chip-count {
    font-size:.66rem; padding:1px 7px; border-radius:999px;
    background:color-mix(in srgb, var(--gc) 20%, transparent);
  }

  /* Command palette additions */
  .launcher-section-label {
    font-size:.58rem; text-transform:uppercase; letter-spacing:.14em;
    color:var(--dim); padding:2px 4px 6px;
  }
  .launcher-item-glyph { width:20px; text-align:center; font-size:.92rem; flex-shrink:0; }
  .launcher-item--command .launcher-item-glyph { color:var(--accent2); }
  .launcher-item-kind {
    font-size:.58rem; text-transform:uppercase; letter-spacing:.1em;
    color:var(--dim); padding:2px 6px; border-radius:6px;
    border:1px solid var(--bsoft); flex-shrink:0;
  }
  .launcher-item--command .launcher-item-kind { color:var(--accent2); border-color:rgba(176,158,255,.25); }

  /* Macro prompt dialog */
  .prompt-overlay { z-index:120; }
  .prompt-modal {
    width:min(420px, 92vw);
    border-radius:var(--r-lg); padding:20px;
    border:1px solid rgba(94,231,247,.24);
    background:linear-gradient(155deg, var(--s2), var(--s1));
    box-shadow:0 24px 60px rgba(0,0,0,.6), 0 0 40px var(--accent-glow, rgba(94,231,247,.18));
  }
  .prompt-header { display:flex; align-items:center; gap:10px; margin-bottom:14px; }
  .prompt-header h3 { margin:0; font-size:.95rem; font-weight:600; color:var(--text); }
  .prompt-spark {
    width:30px; height:30px; border-radius:9px; display:grid; place-items:center; flex-shrink:0;
    color:var(--accent); background:rgba(94,231,247,.12); border:1px solid rgba(94,231,247,.25);
  }
  .prompt-input {
    width:100%; font:inherit; font-size:.9rem; color:var(--text);
    border-radius:var(--r-sm); border:1px solid rgba(94,231,247,.3);
    background:rgba(6,9,15,.8); padding:11px 13px; outline:none;
  }
  .prompt-input:focus { border-color:var(--accent); box-shadow:0 0 0 3px rgba(94,231,247,.1); }
  .prompt-footer { display:flex; gap:8px; margin-top:16px; }
  .prompt-footer .btn-accent { flex:1; }
</style>