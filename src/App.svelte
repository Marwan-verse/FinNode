<script>
  import { onMount, tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/tauri';
  import { listen } from '@tauri-apps/api/event';
  import { appWindow } from '@tauri-apps/api/window';
  import appLogo from '../src-tauri/icons/icon.png';

  const SETTINGS_KEY = 'finnode.settings.v2';
  const LOGO_ICON = 'logo';
  const THEMES = {
    dark: { bg:'#081321', panel:'rgba(9,18,31,0.88)', line:'rgba(120,227,255,0.25)', text:'#e8f7ff', muted:'rgba(200,238,255,0.72)', glow:'rgba(124,244,255,0.45)', accent:'#7cf4ff', accent2:'#9dffb9', danger:'#ff8fa3', bodyBg:'linear-gradient(145deg,#050b14 0%,#0a1628 55%,#050b14 100%)' },
    light: { bg:'#f0f4f8', panel:'rgba(255,255,255,0.92)', line:'rgba(30,80,120,0.25)', text:'#1a2a3a', muted:'rgba(40,60,80,0.65)', glow:'rgba(30,120,200,0.35)', accent:'#1e88e5', accent2:'#43a047', danger:'#e53935', bodyBg:'linear-gradient(145deg,#e8eef4 0%,#f5f7fa 55%,#e8eef4 100%)' },
    midnight: { bg:'#0a0020', panel:'rgba(15,5,40,0.92)', line:'rgba(160,100,255,0.25)', text:'#e8e0ff', muted:'rgba(200,180,255,0.7)', glow:'rgba(160,100,255,0.45)', accent:'#b388ff', accent2:'#ea80fc', danger:'#ff5252', bodyBg:'linear-gradient(145deg,#0a0020 0%,#15083a 55%,#0a0020 100%)' },
    forest: { bg:'#041a0a', panel:'rgba(5,25,12,0.92)', line:'rgba(100,230,140,0.25)', text:'#e0ffe8', muted:'rgba(180,255,200,0.7)', glow:'rgba(100,230,140,0.45)', accent:'#69f0ae', accent2:'#b9f6ca', danger:'#ff8a80', bodyBg:'linear-gradient(145deg,#041a0a 0%,#082a14 55%,#041a0a 100%)' }
  };
  const NODE_COLORS = ['cyan','green','purple','orange','pink','red','blue','yellow'];
  const COLOR_MAP = { cyan:'124,244,255', green:'100,230,140', purple:'160,100,255', orange:'255,170,50', pink:'255,100,180', red:'255,80,80', blue:'80,140,255', yellow:'255,220,60' };
  const settingsTabs = [
    { id:'general', label:'General' }, { id:'appearance', label:'Appearance' },
    { id:'nodes', label:'Nodes' }, { id:'tray', label:'Tray' },
    { id:'shortcuts', label:'Shortcuts' }, { id:'history', label:'History' }
  ];
  const nodeTemplates = [
    { id:'web-project', name:'Web Project', icon:'◈', description:'Frontend app + docs + browser', browser:'https://vite.dev', script:'npm run dev' },
    { id:'rust-app', name:'Rust App', icon:'⬡', description:'Cargo workflow and crates links', browser:'https://crates.io', script:'cargo check' },
    { id:'docs-hub', name:'Documentation Hub', icon:LOGO_ICON, description:'Notes, references, and quick links', browser:'https://doc.rust-lang.org', script:'npm run build:web' },
    { id:'research-stack', name:'Research Stack', icon:'⟁', description:'Context, ideas, and experiments', browser:'https://github.com/trending', script:'npm run build:web' }
  ];
  const MAIN_NODE_ID = 'main-node';
  const MAIN_NODE_NAME = 'main';

  let nodes = [], renderNodes = [], smoothNodes = [], links = [];
  let stealth = false, showDesktop = true, activeTab = 'general';
  let draggingId = null, dragOffset = {x:0,y:0}, pendingPointer = null;
  let dragFrame = null, nodeSpringFrame = null, saveTimer = null;
  let statusText = 'Loading...', fatalError = '';
  let selectedTemplate = nodeTemplates[0].id;
  let activityLog = [], currentWindowLabel = 'main', isDesktopWindow = false;
  let expandedNodeId = null, dragMoved = false, dragStart = {x:0,y:0};
  let editPopup = {open:false,x:0,y:0,nodeId:null};
  let editDraft = {name:'',description:'',path:'',browser:'',script:'',color:'cyan',macros:[]};
  let editSelectedLinks = [], editNode = null, contextNode = null;
  let contextMenu = {open:false,x:0,y:0,nodeId:null};
  let nodeLayer, viewBox = '0 0 1 1';
  const nodeElements = new Map();

  // Workspaces
  let workspaces = [], activeWorkspaceId = 'default', workspaceName = '';
  // Multi-select
  let selectedIds = new Set(), lastClickWasSelect = false;
  // Quick launcher
  let showLauncher = false, launcherQuery = '', launcherIndex = 0;
  // Node bounds update (for cursor polling click-through)
  let boundsFrame = null;
  // Tooltip
  let hoveredId = null, tooltipPos = {x:0,y:0}, tooltipTimer = null;
  // Highlight connections
  let highlightNodeId = null;
  // Command history
  let commandHistory = [];
  // Theme
  let settings = loadSettings();
  // Platform
  let osPlatform = 'windows';
  let supportsClickThrough = true;

  function isLockedNode(nodeOrId) {
    const id = typeof nodeOrId === 'string' ? nodeOrId : nodeOrId?.id;
    return id === MAIN_NODE_ID || Boolean(nodeOrId?.locked);
  }
  function createMainNode(anchor) {
    let x = 80, y = 80;
    if (anchor) {
      x = Math.max(24, anchor.x - 140);
      y = Math.max(24, anchor.y - 40);
    }
    return {
      id: MAIN_NODE_ID,
      name: MAIN_NODE_NAME,
      icon: '',
      description: '',
      x, y,
      links: [],
      targets: { path:null, editor:null, browser:null, script:null },
      color: 'cyan',
      group: null,
      macros: [],
      collapsed: false,
      last_launched: null,
      locked: true
    };
  }
  function ensureMainNode(list) {
    let changed = false;
    const next = list.map(n=>{
      if (n.id !== MAIN_NODE_ID) return n;
      const updated = {...n, name: MAIN_NODE_NAME, icon:'', locked:true};
      if (updated.name !== n.name || updated.icon !== n.icon || updated.locked !== n.locked) changed = true;
      return updated;
    });
    if (next.some(n=>n.id===MAIN_NODE_ID)) return { nodes:next, added: changed };
    const anchor = next[0];
    return { nodes:[createMainNode(anchor), ...next], added:true };
  }

  function createDefaultSettings() {
    return {
      general: { openOnLogin:false, startMinimizedToTray:false, restoreLastMode:true, lastMode:'settings' },
      appearance: { theme:'dark', motionScale:1, nodeGlow:0.45 },
      nodes: { showDesktop:true, smoothness:0.2, clickThrough:true },
      tray: { leftClickAction:'open-settings' },
      shortcuts: { toggleStealth:'Alt+S' }
    };
  }
  function cloneObj(v) { return JSON.parse(JSON.stringify(v)); }
  function mergeSettings(base, inc) {
    const n = cloneObj(base);
    if (!inc || typeof inc !== 'object') return n;
    for (const [k,v] of Object.entries(inc)) {
      if (v && typeof v === 'object' && !Array.isArray(v) && n[k] && typeof n[k] === 'object' && !Array.isArray(n[k])) n[k] = mergeSettings(n[k], v);
      else if (v !== undefined) n[k] = v;
    }
    return n;
  }
  function loadSettings() {
    const d = createDefaultSettings();
    if (typeof window === 'undefined') return d;
    try { const r = localStorage.getItem(SETTINGS_KEY); return r ? mergeSettings(d, JSON.parse(r)) : d; } catch { return d; }
  }
  function persistSettings() { if (typeof window !== 'undefined') localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings)); }
  function updateSettings(fn) { const n = cloneObj(settings); fn(n); settings = n; persistSettings(); }

  function applyTheme(name) {
    const t = THEMES[name] || THEMES.dark;
    const r = document.documentElement;
    r.style.setProperty('--bg', t.bg); r.style.setProperty('--panel', t.panel);
    r.style.setProperty('--line', t.line); r.style.setProperty('--text', t.text);
    r.style.setProperty('--muted', t.muted); r.style.setProperty('--glow', t.glow);
    r.style.setProperty('--accent', t.accent); r.style.setProperty('--accent-2', t.accent2);
    r.style.setProperty('--danger', t.danger);
  }

  function nodeColor(node) { return COLOR_MAP[node?.color || 'cyan'] || COLOR_MAP.cyan; }
  function isLogoIcon(icon) { return icon === LOGO_ICON || icon === '⟡'; }
  function recordActivity(msg) {
    const ts = new Date().toLocaleTimeString();
    activityLog = [{id:`${Date.now()}-${Math.random()}`, text:`${ts} - ${msg}`}, ...activityLog].slice(0,24);
  }

  $: { renderNodes = nodes.map(n => { const s = smoothNodes.find(i=>i.id===n.id); const raw = draggingId===n.id; return {...n, renderX: raw?n.x:(s?s.x:n.x), renderY: raw?n.y:(s?s.y:n.y)}; }); }
  $: isDesktopWindow = currentWindowLabel === 'desktop';
  $: supportsClickThrough = osPlatform === 'windows' || osPlatform === 'linux';
  $: { if (typeof document !== 'undefined') {
    document.documentElement.classList.toggle('desktop-overlay-window', isDesktopWindow);
    document.body.classList.toggle('is-stealth', stealth);
    document.body.classList.toggle('desktop-mode', isDesktopWindow);
    document.documentElement.style.setProperty('--motion-scale', String(Math.max(0.4, settings.appearance.motionScale)));
    document.documentElement.style.setProperty('--node-glow', String(Math.max(0.15, settings.appearance.nodeGlow)));
    applyTheme(settings.appearance.theme);
  }}
  $: contextNode = contextMenu.nodeId ? nodes.find(n=>n.id===contextMenu.nodeId) : null;
  $: editNode = editPopup.nodeId ? nodes.find(n=>n.id===editPopup.nodeId) : null;
  $: launcherResults = launcherQuery.trim() ? nodes.filter(n=>n.name.toLowerCase().includes(launcherQuery.toLowerCase())).slice(0,8) : nodes.slice(0,8);

  function nodeRef(el, id) { nodeElements.set(id, el); queueRender(); return { destroy() { nodeElements.delete(id); scheduleBoundsUpdate(); } }; }
  function queueRender() { void tick().then(()=>{ renderConnections(); scheduleBoundsUpdate(); }); }

  async function detectPlatform() {
    try {
      osPlatform = await invoke('get_platform');
    } catch {
      osPlatform = 'windows';
    }
    const supported = osPlatform === 'windows' || osPlatform === 'linux';
    if (!supported) {
      updateSettings(d=>{d.nodes.clickThrough=false;});
      void syncDesktopCT(false);
    }
  }

  /** Schedule a bounding-box update for the cursor-polling thread. */
  function scheduleBoundsUpdate() {
    if (!isDesktopWindow) return;
    if (boundsFrame !== null) return;
    boundsFrame = requestAnimationFrame(()=>{
      boundsFrame = null;
      void syncNodeBounds();
    });
  }

  /** Send screen-space bounding boxes of all interactive elements to Rust. */
  async function syncNodeBounds() {
    if (!isDesktopWindow) return;
    const rects = [];
    for (const el of nodeElements.values()) rects.push(el.getBoundingClientRect());
    // Also include popups/overlays so they remain interactive
    if (typeof document !== 'undefined') {
      document.querySelectorAll('.context-menu, .node-editor-popup, .batch-bar, .launcher-overlay').forEach(el=>{
        rects.push(el.getBoundingClientRect());
      });
    }
    let scale = 1;
    try { scale = await appWindow.scaleFactor(); } catch {}
    const bounds = rects.map(r=>({
      left: r.left * scale,
      top: r.top * scale,
      right: r.right * scale,
      bottom: r.bottom * scale
    })).filter(r=>r.right > r.left && r.bottom > r.top);
    try { await invoke('update_node_bounds', { bounds }); } catch (e) { /* silent */ }
  }

  function startSpring() { if (nodeSpringFrame !== null) return; nodeSpringFrame = requestAnimationFrame(stepSpring); }
  function syncSmooth(imm=false) {
    const cur = new Map(smoothNodes.map(i=>[i.id,i]));
    smoothNodes = nodes.map(n => { const e = cur.get(n.id); return (!e||imm) ? {id:n.id,x:n.x,y:n.y,vx:0,vy:0} : e; });
    startSpring();
  }
  function stepSpring() {
    nodeSpringFrame = null;
    if (!smoothNodes.length) return;
    const st = Math.max(0.08, Math.min(0.45, settings.nodes.smoothness)), damp = 0.82;
    const tgts = new Map(nodes.map(n=>[n.id,n]));
    let active = false;
    smoothNodes = smoothNodes.map(i => {
      const t = tgts.get(i.id); if (!t) return i;
      if (draggingId===i.id) return {...i,x:t.x,y:t.y,vx:0,vy:0};
      const dx=t.x-i.x, dy=t.y-i.y, vx=(i.vx+dx*st)*damp, vy=(i.vy+dy*st)*damp;
      if (Math.abs(dx)>0.14||Math.abs(dy)>0.14||Math.abs(vx)>0.14||Math.abs(vy)>0.14) active=true;
      return {...i, x:i.x+vx, y:i.y+vy, vx, vy};
    });
    queueRender();
    if (active) nodeSpringFrame = requestAnimationFrame(stepSpring);
  }

  async function syncDesktopVis(v) { try { await invoke('set_desktop_visibility',{visible:v}); } catch(e) { updateStatus(String(e)); } }
  async function syncDesktopCT(e) { try { await invoke('set_desktop_click_through',{enabled:e}); } catch(e2) { updateStatus(String(e2)); } }
  function updateDesktopCT(en,sync=true) {
    if (!supportsClickThrough) {
      updateSettings(d=>{d.nodes.clickThrough=false;});
      if (sync) void syncDesktopCT(false);
      updateStatus(`Background click-through not supported on ${osPlatform}`);
      return;
    }
    updateSettings(d=>{d.nodes.clickThrough=Boolean(en);});
    if(sync) void syncDesktopCT(Boolean(en));
    scheduleBoundsUpdate();
  }

  function applyDesktopVis(vis,sync=true) {
    showDesktop = Boolean(vis); closeCtx(); if(!vis){closeEditor();expandedNodeId=null;}
    if(!isDesktopWindow) updateSettings(d=>{d.nodes.showDesktop=showDesktop; if(d.general.restoreLastMode)d.general.lastMode=showDesktop?'desktop':'settings';});
    if(sync) void syncDesktopVis(showDesktop);
    queueRender();
  }
  function closeCtx() { if(contextMenu.open){ contextMenu={open:false,x:0,y:0,nodeId:null}; void tick().then(scheduleBoundsUpdate); } }
  function closeEditor() { if(editPopup.open){ editPopup={open:false,x:0,y:0,nodeId:null}; void tick().then(scheduleBoundsUpdate); } }
  function openEditor(nid) {
    if (isLockedNode(nid)) return;
    const n=nodes.find(i=>i.id===nid), el=nodeElements.get(nid);
    if(!n||!el) return;
    const r=el.getBoundingClientRect(), pw=320, ph=420;
    let x=r.right+14; if(x+pw>innerWidth-12) x=r.left-pw-14;
    x=Math.max(12,Math.min(x,innerWidth-pw-12));
    const y=Math.max(12,Math.min(r.top,innerHeight-ph-12));
    editDraft={name:n.name??'',description:n.description??'',path:n.targets?.path??'',browser:n.targets?.browser??'',script:n.targets?.script??'',color:n.color||'cyan',macros:[...(n.macros||[])]};
    editSelectedLinks=[...(n.links??[])];
    editPopup={open:true,x,y,nodeId:nid};
    void tick().then(scheduleBoundsUpdate);
  }
  function toggleEditLink(tid,en) { editSelectedLinks = en ? [...new Set([...editSelectedLinks,tid])] : editSelectedLinks.filter(i=>i!==tid); }
  function saveEditor() {
    if(!editPopup.nodeId || isLockedNode(editPopup.nodeId)) return;
    const nid=editPopup.nodeId;
    nodes=nodes.map(n=>{
      if(n.id!==nid) return n;
      return {...n, name:editDraft.name.trim()||n.name, description:editDraft.description.trim(),
        color:editDraft.color||'cyan', macros:[...editDraft.macros],
        links:[...new Set(editSelectedLinks.filter(l=>l!==nid))],
        targets:{...(n.targets??{}), path:editDraft.path.trim()||null, browser:editDraft.browser.trim()||null, script:editDraft.script.trim()||null}};
    });
    scheduleSave(); queueRender(); updateStatus('Node updated'); closeEditor();
  }
  function addMacroStep() { editDraft.macros = [...editDraft.macros, {action:'open-browser',value:''}]; }
  function removeMacroStep(i) { editDraft.macros = editDraft.macros.filter((_,idx)=>idx!==i); }

  function toggleExpanded(nid) {
    if (isLockedNode(nid)) return;
    expandedNodeId = expandedNodeId===nid ? null : nid;
    if(expandedNodeId!==nid) closeEditor();
    void tick().then(scheduleBoundsUpdate);
  }
  function openEditorSoon(nid) { expandedNodeId=nid; void tick().then(()=>openEditor(nid)); }
  function openCtxMenu(ev,nid) {
    ev.preventDefault(); ev.stopPropagation();
    closeEditor();
    contextMenu={open:true, x:Math.max(10,Math.min(ev.clientX,innerWidth-240)), y:Math.max(10,Math.min(ev.clientY,innerHeight-300)), nodeId:nid};
    void tick().then(scheduleBoundsUpdate);
  }

  // Context menu actions
  function addConnected() { const s=contextNode; if(!s){closeCtx();return;} const t=nodeTemplates.find(i=>i.id===selectedTemplate)||nodeTemplates[0]; const n={id:uid(t.id),name:`${t.name} Node`,icon:t.icon,description:t.description,x:s.x+260,y:s.y+30,links:[],targets:{path:'.',editor:'.',browser:t.browser,script:t.script},color:'cyan',group:null,macros:[],collapsed:false,last_launched:null}; nodes=[...nodes.map(i=>i.id!==s.id?i:{...i,links:[...new Set([...(i.links??[]),n.id])]}),n]; syncSmooth(true); scheduleSave(); updateStatus(`Added connected node`); closeCtx(); }
  function connectNearest() { const s=contextNode; if(!s){closeCtx();return;} const cands=nodes.filter(n=>n.id!==s.id); if(!cands.length){closeCtx();return;} const near=cands.reduce((b,c)=>(((c.x-s.x)**2+(c.y-s.y)**2)<((b.x-s.x)**2+(b.y-s.y)**2)?c:b)); nodes=nodes.map(n=>n.id!==s.id?n:{...n,links:[...new Set([...(n.links??[]),near.id])]}); scheduleSave(); queueRender(); closeCtx(); }
  function clearLinks() { const s=contextNode; if(!s){closeCtx();return;} nodes=nodes.map(n=>n.id===s.id?{...n,links:[]}:n); scheduleSave(); queueRender(); closeCtx(); }
  function cloneFromMenu() { if(contextNode)cloneNode(contextNode.id); closeCtx(); }
  function deleteFromMenu() { if(contextNode)deleteNode(contextNode.id); closeCtx(); }

  // Multi-select
  function toggleSelect(nid, ev) {
    if(!ev.ctrlKey&&!ev.metaKey) return false;
    const next = new Set(selectedIds);
    if(next.has(nid)) next.delete(nid); else next.add(nid);
    selectedIds = next; lastClickWasSelect = true; scheduleBoundsUpdate(); return true;
  }
  function batchLaunch() { for(const id of selectedIds) { const n=nodes.find(i=>i.id===id); if(n) void launchNode(n,'open-path'); } selectedIds=new Set(); scheduleBoundsUpdate(); }
  function batchDelete() { for(const id of selectedIds) deleteNode(id); selectedIds=new Set(); scheduleBoundsUpdate(); }

  // Quick launcher
  function openLauncher() { showLauncher=true; launcherQuery=''; launcherIndex=0; void tick().then(()=>{const el=document.getElementById('launcher-input'); if(el)el.focus(); scheduleBoundsUpdate();}); }
  function closeLauncher() { showLauncher=false; launcherQuery=''; void tick().then(scheduleBoundsUpdate); }
  function launcherKey(ev) {
    if(ev.key==='Escape'){closeLauncher();return;}
    if(ev.key==='ArrowDown'){launcherIndex=Math.min(launcherIndex+1,launcherResults.length-1);return;}
    if(ev.key==='ArrowUp'){launcherIndex=Math.max(launcherIndex-1,0);return;}
    if(ev.key==='Enter'&&launcherResults[launcherIndex]){void launchNode(launcherResults[launcherIndex],'open-path');closeLauncher();}
  }

  // Smart layout
  function layoutGrid() {
    const cols = Math.ceil(Math.sqrt(nodes.length)), gap = 200;
    nodes = nodes.map((n,i)=>({...n, x:80+(i%cols)*gap, y:80+Math.floor(i/cols)*gap}));
    syncSmooth(true); scheduleSave(); updateStatus('Grid layout applied');
  }

  // Workspace management
  async function loadWorkspaces() {
    try {
      const layout = await invoke('load_layout');
      workspaces=layout.workspaces||[]; activeWorkspaceId=layout.active_workspace||'default'; commandHistory=layout.command_history||[];
      const ws=workspaces.find(w=>w.id===activeWorkspaceId)||workspaces[0];
      if(ws){nodes=ws.nodes||[];}
      const ensured = ensureMainNode(nodes);
      nodes = ensured.nodes;
      if (ensured.added) scheduleSave();
      syncSmooth(true);
    } catch(e) { updateStatus(String(e)); }
  }
  async function switchWorkspace(id) {
    try {
      const layout=await invoke('switch_workspace',{workspaceId:id});
      workspaces=layout.workspaces; activeWorkspaceId=layout.active_workspace; commandHistory=layout.command_history||[];
      const ws=workspaces.find(w=>w.id===activeWorkspaceId);
      if(ws){nodes=ws.nodes||[];}
      const ensured = ensureMainNode(nodes);
      nodes = ensured.nodes;
      if (ensured.added) scheduleSave();
      syncSmooth(true); queueRender(); updateStatus(`Switched to ${ws?.name}`);
    } catch(e) { updateStatus(String(e)); }
  }
  async function createWorkspace() { if(!workspaceName.trim()) return; try { await invoke('create_workspace',{name:workspaceName.trim()}); workspaceName=''; await loadWorkspaces(); updateStatus('Workspace created'); } catch(e) { updateStatus(String(e)); } }
  async function deleteWorkspace(id) { try { await invoke('delete_workspace',{workspaceId:id}); await loadWorkspaces(); updateStatus('Workspace deleted'); } catch(e) { updateStatus(String(e)); } }

  // Core CRUD
  function uid(base) { return `${base}-${Math.random().toString(36).slice(2,8)}`; }
  function hasText(v) { return typeof v==='string'&&v.trim().length>0; }
  function hasAction(n,a) { const t=n?.targets??{}; if(a==='open-path')return hasText(t.path); if(a==='open-editor')return hasText(t.editor)||hasText(t.path); if(a==='open-browser')return hasText(t.browser); if(a==='run-script')return hasText(t.script); return false; }
  function hasAnyActions(n) { return hasAction(n,'open-path')||hasAction(n,'open-editor')||hasAction(n,'open-browser')||hasAction(n,'run-script'); }
  async function launchNode(n,a) { try { await invoke('launch_node',{node:n,action:a}); updateStatus(`Launched ${n.name}`); } catch(e) { updateStatus(String(e)); } }
  async function runMacro(steps) { try { await invoke('run_node_macro',{steps}); updateStatus('Macro started'); } catch(e) { updateStatus(String(e)); } }
  async function toggleStealth() { try { await invoke('set_stealth_mode',{enabled:!stealth}); } catch(e) { updateStatus(String(e)); } }
  async function openSettingsView() { try { await invoke('show_settings_view'); } catch(e) { updateStatus(String(e)); } }
  async function revealGhost() { if(!stealth) return; try { await invoke('set_stealth_mode',{enabled:false}); } catch(e) { updateStatus(String(e)); } }
  async function hideToTray() { try { await invoke('hide_main_window'); } catch(e) { updateStatus(String(e)); } }
  async function exitApp() { try { await invoke('exit_app'); } catch(e) { updateStatus(String(e)); } }
  async function pinBottom() { try { await invoke('pin_desktop_bottom'); } catch(e) {} }
  function toggleDesktop() { applyDesktopVis(!showDesktop, true); }

  function addNode() {
    const t=nodeTemplates.find(i=>i.id===selectedTemplate)||nodeTemplates[0]; const off=nodes.length*18;
    const n={id:uid(t.id),name:'',icon:t.icon,description:'',x:90+off,y:110+off,links:[],targets:{path:null,editor:null,browser:null,script:null},color:'cyan',group:null,macros:[],collapsed:false,last_launched:null};
    nodes=[...nodes,n]; syncSmooth(true); scheduleSave(); updateStatus('Added node'); openEditorSoon(n.id);
  }
  function cloneNode(id) {
    if (isLockedNode(id)) return;
    const n=nodes.find(i=>i.id===id); if(!n) return;
    nodes=[...nodes,{...n,id:uid(n.id),name:`${n.name} Copy`,x:n.x+26,y:n.y+26,links:[...(n.links??[])],targets:{...(n.targets??{})},locked:false}];
    syncSmooth(true); scheduleSave();
  }
  function deleteNode(id) {
    if (isLockedNode(id)) { updateStatus('Main node is locked'); return; }
    if(contextMenu.nodeId===id)closeCtx();
    if(editPopup.nodeId===id)closeEditor();
    if(expandedNodeId===id)expandedNodeId=null;
    nodes=nodes.filter(n=>n.id!==id).map(n=>({...n,links:(n.links??[]).filter(l=>l!==id)}));
    syncSmooth(true); scheduleSave();
  }
  function moveNode(id,dir) { const i=nodes.findIndex(n=>n.id===id); if(i<0) return; const ti=i+dir; if(ti<0||ti>=nodes.length) return; const next=[...nodes]; const [m]=next.splice(i,1); next.splice(ti,0,m); nodes=next; scheduleSave(); }

  function scheduleSave() {
    if(saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(async()=>{
      const ws = workspaces.find(w=>w.id===activeWorkspaceId);
      if(ws) { ws.nodes=nodes; ws.zoom=1; ws.pan_x=0; ws.pan_y=0; }
      await invoke('save_layout',{layout:{active_workspace:activeWorkspaceId,workspaces,command_history:commandHistory}});
      updateStatus('Saved');
    }, 220);
  }
  function updateStatus(t) { statusText=t; recordActivity(t); }

  function beginDrag(ev,id) {
    if(ev.button!==0||(ev.target instanceof Element&&ev.target.closest('button'))) return;
    if(toggleSelect(id,ev)) return;
    const n=nodes.find(i=>i.id===id); if(!n) return;
    draggingId=id; dragMoved=false; dragStart={x:ev.clientX,y:ev.clientY};
    closeCtx(); closeEditor();
    const r=ev.currentTarget.getBoundingClientRect();
    dragOffset={x:ev.clientX-r.left,y:ev.clientY-r.top};
    ev.currentTarget.classList.add('is-dragging');
  }
  function onPointerMove(ev) {
    if(!draggingId||!nodeLayer) return;
    if(!dragMoved){dragMoved=Math.abs(ev.clientX-dragStart.x)>4||Math.abs(ev.clientY-dragStart.y)>4;}
    pendingPointer={x:ev.clientX,y:ev.clientY};
    if(dragFrame!==null) return;
    dragFrame=requestAnimationFrame(()=>{
      dragFrame=null; if(!pendingPointer||!draggingId||!nodeLayer) return;
      const p=pendingPointer; pendingPointer=null;
      const n=nodes.find(i=>i.id===draggingId); if(!n) return;
      const lr=nodeLayer.getBoundingClientRect();
      n.x=p.x-lr.left-dragOffset.x;
      n.y=p.y-lr.top-dragOffset.y;
      nodes=[...nodes]; queueRender();
    });
  }
  function onPointerUp() {
    if(!draggingId) return;
    const rid=draggingId;
    if(dragFrame!==null){cancelAnimationFrame(dragFrame);dragFrame=null;} pendingPointer=null;
    const el=nodeElements.get(draggingId); if(el) el.classList.remove('is-dragging');
    draggingId=null;
    if(dragMoved){syncSmooth(true);scheduleSave();pinBottom();}
    if(!dragMoved&&!lastClickWasSelect) {
      if (isLockedNode(rid)) {
        expandedNodeId = null; closeEditor();
        void openSettingsView();
      } else {
        toggleExpanded(rid);
      }
    }
    dragMoved=false; lastClickWasSelect=false;
  }

  function centerOf(el) { const r=el.getBoundingClientRect(), lr=nodeLayer.getBoundingClientRect(); return {x:r.left-lr.left+r.width/2, y:r.top-lr.top+r.height/2}; }
  function renderConnections() {
    if(!nodeLayer) return;
    const b=nodeLayer.getBoundingClientRect(); if(!b.width||!b.height) return;
    viewBox=`0 0 ${b.width} ${b.height}`;
    const next=[];
    for(const n of renderNodes) for(const tid of n.links??[]) {
      const se=nodeElements.get(n.id), te=nodeElements.get(tid); if(!se||!te) continue;
      const s=centerOf(se), t=centerOf(te), ox=Math.max(100,Math.abs(t.x-s.x)*0.35);
      next.push({d:`M ${s.x} ${s.y} C ${s.x+ox} ${s.y}, ${t.x-ox} ${t.y}, ${t.x} ${t.y}`, from:n.id, to:tid});
    }
    links=next;
  }

  // Tooltip
  function onNodeEnter(ev,nid) { hoveredId=nid; highlightNodeId=nid; const r=ev.currentTarget.getBoundingClientRect(); tooltipPos={x:r.left+r.width/2,y:r.top-8}; }
  function onNodeLeave() { hoveredId=null; highlightNodeId=null; }

  async function bootstrap() {
    const ul1=await listen('stealth-changed',({payload})=>{stealth=Boolean(payload);updateStatus(`Stealth ${stealth?'on':'off'}`);});
    const ul2=await listen('layout-updated',async()=>{await loadWorkspaces();});
    const ul3=await listen('desktop-visibility-changed',({payload})=>{const v=Boolean(payload); if(isDesktopWindow){showDesktop=v;if(!v){expandedNodeId=null;closeCtx();closeEditor();} scheduleBoundsUpdate();}else applyDesktopVis(v,false);});
    const ul4=await listen('desktop-click-through-changed',({payload})=>{updateSettings(d=>{d.nodes.clickThrough=Boolean(payload);}); scheduleBoundsUpdate();});
    const ul5=await listen('open-settings-tab',({payload})=>{if(!isDesktopWindow)activeTab=typeof payload==='string'?payload:'general';});
    const ul6=await listen('toggle-quick-launcher',()=>{if(showLauncher)closeLauncher();else openLauncher();});

    await detectPlatform();
    await loadWorkspaces();
    if(isDesktopWindow) { showDesktop=true; await syncDesktopCT(settings.nodes.clickThrough); }
    else { showDesktop=settings.general.restoreLastMode?settings.general.lastMode==='desktop':settings.nodes.showDesktop; await syncDesktopVis(showDesktop); await syncDesktopCT(settings.nodes.clickThrough); }
    updateStatus(`Loaded ${nodes.length} nodes`); queueRender();

    const onResize=()=>queueRender();
    const onMove=e=>onPointerMove(e);
    const onUp=()=>onPointerUp();
    const onDown=e=>{if(!(e.target instanceof Element)){closeCtx();closeEditor();return;} if(!e.target.closest('.context-menu'))closeCtx(); if(!e.target.closest('.node-editor-popup'))closeEditor(); if(!e.target.closest('.node')&&!e.ctrlKey)selectedIds=new Set();};
    const onKey=e=>{if(e.key==='Escape'){closeCtx();closeEditor();closeLauncher();}};
    window.addEventListener('resize',onResize); window.addEventListener('pointermove',onMove);
    window.addEventListener('pointerup',onUp); window.addEventListener('pointerdown',onDown);
    window.addEventListener('keydown',onKey);

    return ()=>{ ul1();ul2();ul3();ul4();ul5();ul6(); window.removeEventListener('resize',onResize); window.removeEventListener('pointermove',onMove); window.removeEventListener('pointerup',onUp); window.removeEventListener('pointerdown',onDown); window.removeEventListener('keydown',onKey); if(saveTimer)clearTimeout(saveTimer); if(dragFrame!==null)cancelAnimationFrame(dragFrame); if(nodeSpringFrame!==null)cancelAnimationFrame(nodeSpringFrame); if(boundsFrame!==null)cancelAnimationFrame(boundsFrame); };
  }

  onMount(()=>{
    currentWindowLabel = appWindow.label ?? 'main';
    let disposed=false, cleanup=()=>{};
    void bootstrap().then(c=>{if(disposed){c();return;}cleanup=c;}).catch(e=>{fatalError=e?.stack??e?.message??String(e);});
    return ()=>{disposed=true;cleanup();};
  });
</script>

{#if fatalError}
  <pre class="fatal">{fatalError}</pre>
{:else if isDesktopWindow}
  <!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
  <main class="desktop-overlay stage" class:stage--hidden={!showDesktop}>
    <svg class="links" {viewBox}>
      {#each links as link}
        <path class="link" class:link--highlight={highlightNodeId&&(link.from===highlightNodeId||link.to===highlightNodeId)} d={link.d}></path>
      {/each}
    </svg>

    <div class="node-layer" bind:this={nodeLayer}>
      {#each renderNodes as node (node.id)}
        <article class="node" class:node--expanded={expandedNodeId===node.id} class:node--selected={selectedIds.has(node.id)}
          use:nodeRef={node.id} style="left:{node.renderX}px;top:{node.renderY}px;--nc:{nodeColor(node)};"
          on:pointerdown={ev=>beginDrag(ev,node.id)} on:contextmenu={ev=>openCtxMenu(ev,node.id)}
          on:mouseenter={ev=>onNodeEnter(ev,node.id)} on:mouseleave={onNodeLeave}>
          <div class="node__surface">
            {#if node.last_launched}
              <span class="node__status node__status--active" title="Recently launched"></span>
            {:else}
              <span class="node__status node__status--idle" title="Idle"></span>
            {/if}
            {#if expandedNodeId === node.id}
              <header class="node__header">
                <div class="node__icon">
                  {#if isLogoIcon(node.icon)}
                    <img class="node__logo" src={appLogo} alt="FinNode"/>
                  {:else}
                    {node.icon ?? '◆'}
                  {/if}
                </div>
                <div class="node__header-copy">
                  <div class="node__name">{node.name}</div>
                  <div class="node__meta">{node.id.slice(0,8)}</div>
                </div>
                {#if !isLockedNode(node)}
                  <button class="node__edit-trigger" on:click|stopPropagation={()=>openEditor(node.id)}>Edit</button>
                {/if}
              </header>
              <p class="node__body">{node.description || 'A linked context node'}</p>
              {#if hasAnyActions(node)}
                <div class="node__actions">
                  {#if hasAction(node,'open-path')}<button on:click|stopPropagation={()=>launchNode(node,'open-path')}>Folder</button>{/if}
                  {#if hasAction(node,'open-editor')}<button on:click|stopPropagation={()=>launchNode(node,'open-editor')}>Editor</button>{/if}
                  {#if hasAction(node,'open-browser')}<button on:click|stopPropagation={()=>launchNode(node,'open-browser')}>Browser</button>{/if}
                  {#if hasAction(node,'run-script')}<button on:click|stopPropagation={()=>launchNode(node,'run-script')}>Script</button>{/if}
                </div>
              {/if}
              {#if node.macros && node.macros.length > 0}
                <button class="node__macro-btn" on:click|stopPropagation={()=>runMacro(node.macros)}>▶ Run Macro ({node.macros.length} steps)</button>
              {/if}
            {:else}
              <div class="node__compact-title">{node.name}</div>
            {/if}
          </div>
        </article>
      {/each}
    </div>

    <!-- Multi-select toolbar -->
    {#if selectedIds.size > 0}
      <div class="batch-bar">
        <span>{selectedIds.size} selected</span>
        <button on:click={batchLaunch}>Open All</button>
        <button class="danger" on:click={batchDelete}>Delete All</button>
        <button on:click={()=>{selectedIds=new Set();}}>Clear</button>
      </div>
    {/if}

    {#if editPopup.open && editNode}
      <div class="node-editor-popup" style="left:{editPopup.x}px;top:{editPopup.y}px;" on:pointerdown|stopPropagation>
        <div class="node-editor-popup__title">Edit Node</div>
        <label><span>Title</span><input bind:value={editDraft.name}/></label>
        <label><span>Description</span><textarea rows="2" bind:value={editDraft.description}></textarea></label>
        <label><span>Folder path</span><input bind:value={editDraft.path} placeholder="/path/to/project"/></label>
        <label><span>Browser URL</span><input bind:value={editDraft.browser} placeholder="https://..."/></label>
        <label><span>Script</span><input bind:value={editDraft.script} placeholder="npm run dev"/></label>
        <label><span>Color</span>
          <div class="color-picker">
            {#each NODE_COLORS as c}
              <button class="color-dot" class:color-dot--active={editDraft.color===c} style="--dc:{COLOR_MAP[c]}" on:click={()=>{editDraft.color=c;}}></button>
            {/each}
          </div>
        </label>
        <div class="node-editor-popup__links">
          <div class="section__title">Macros</div>
          {#each editDraft.macros as step, i}
            <div class="macro-row">
              <select bind:value={step.action}><option value="open-browser">Browser</option><option value="open-path">Path</option><option value="run-script">Script</option><option value="delay">Delay(ms)</option></select>
              <input bind:value={step.value} placeholder="value"/>
              <button class="danger" on:click={()=>removeMacroStep(i)}>×</button>
            </div>
          {/each}
          <button class="chip chip--sm" on:click={addMacroStep}>+ Add Step</button>
        </div>
        <div class="node-editor-popup__links">
          <div class="section__title">Links</div>
          {#each nodes.filter(n=>n.id!==editNode.id) as cand (cand.id)}
            <label class="link-toggle"><span>{cand.name}</span><input type="checkbox" checked={editSelectedLinks.includes(cand.id)} on:change={ev=>toggleEditLink(cand.id,ev.currentTarget.checked)}/></label>
          {/each}
        </div>
        <div class="node-editor-popup__actions">
          <button on:click={saveEditor}>Save</button>
          <button class="ghost" on:click={closeEditor}>Cancel</button>
        </div>
      </div>
    {/if}

    {#if contextMenu.open && contextNode}
      <div class="context-menu" style="left:{contextMenu.x}px;top:{contextMenu.y}px;" on:pointerdown|stopPropagation>
        <div class="context-menu__title">{contextNode.name}</div>
        {#if isLockedNode(contextNode)}
          <button on:click={()=>{void openSettingsView(); closeCtx();}}>Open Settings</button>
        {:else}
          <button on:click={addConnected}>Add Connected Node</button>
          <button on:click={connectNearest}>Connect Nearest</button>
          <button on:click={clearLinks}>Clear Links</button>
          <button on:click={cloneFromMenu}>Clone</button>
          <button class="danger" on:click={deleteFromMenu}>Delete</button>
        {/if}
      </div>
    {/if}

    <!-- Quick launcher -->
    {#if showLauncher}
      <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
      <div class="launcher-overlay" role="presentation" on:click|self={closeLauncher}>
        <div class="launcher">
          <input id="launcher-input" class="launcher__input" placeholder="Search nodes..." bind:value={launcherQuery} on:keydown={launcherKey}/>
          <div class="launcher__results">
            {#each launcherResults as r, i (r.id)}
              <button class="launcher__item" class:launcher__item--active={i===launcherIndex} on:click={()=>{void launchNode(r,'open-path');closeLauncher();}}>
                {#if isLogoIcon(r.icon)}
                  <img class="launcher__icon" src={appLogo} alt=""/>
                {:else}
                  <span class="launcher__icon">{r.icon ?? '◆'}</span>
                {/if}
                {r.name}
              </button>
            {/each}
          </div>
        </div>
      </div>
    {/if}

    <!-- Tooltip -->
    {#if hoveredId && !expandedNodeId}
      {@const tn = nodes.find(n=>n.id===hoveredId)}
      {#if tn}
        <div class="tooltip" style="left:{tooltipPos.x}px;top:{tooltipPos.y}px;">{tn.name}{tn.description ? ` — ${tn.description}` : ''}</div>
      {/if}
    {/if}
  </main>
{:else}
  <div class="settings-app">
    <button class="ghost-fin" title="Reveal" on:mouseenter={revealGhost}></button>
    <aside class="rail rail--settings">
      <div class="settings-head">
        <div class="brand"><div class="brand__mark"><img class="brand__logo" src={appLogo} alt="FinNode"/></div><div><div class="brand__name">FinNode Settings</div><div class="brand__tag">desktop node control</div></div></div>
        <div class="window-controls">
          <button class="window-controls__btn" title="Minimize" on:click={hideToTray}>-</button>
          <button class="window-controls__btn window-controls__btn--danger" title="Exit" on:click={exitApp}>x</button>
        </div>
      </div>
      <div class="rail__tabs">
        {#each settingsTabs as tab}<button class:tab--active={activeTab===tab.id} class="tab" on:click={()=>{activeTab=tab.id;}}>{tab.label}</button>{/each}
      </div>
      <div class="rail__section settings-center">
        {#if activeTab==='general'}
          <div class="section__title">Startup</div>
          <label class="toggle-row"><span>Open on login</span><input type="checkbox" checked={settings.general.openOnLogin} on:change={ev=>updateSettings(d=>{d.general.openOnLogin=ev.currentTarget.checked;})}/></label>
          <label class="toggle-row"><span>Start minimized</span><input type="checkbox" checked={settings.general.startMinimizedToTray} on:change={ev=>updateSettings(d=>{d.general.startMinimizedToTray=ev.currentTarget.checked;})}/></label>
          <label class="toggle-row"><span>Restore last mode</span><input type="checkbox" checked={settings.general.restoreLastMode} on:change={ev=>updateSettings(d=>{d.general.restoreLastMode=ev.currentTarget.checked;})}/></label>
        {:else if activeTab==='appearance'}
          <div class="section__title">Theme</div>
          <div class="theme-row">
            {#each Object.keys(THEMES) as t}
              <button class="chip" class:chip--active={settings.appearance.theme===t} on:click={()=>updateSettings(d=>{d.appearance.theme=t;})}>{t}</button>
            {/each}
          </div>
          <label class="slider-row"><span>Motion: {settings.appearance.motionScale.toFixed(2)}</span><input type="range" min="0.4" max="1.6" step="0.05" value={settings.appearance.motionScale} on:input={ev=>updateSettings(d=>{d.appearance.motionScale=Number(ev.currentTarget.value);})}/></label>
          <label class="slider-row"><span>Glow: {settings.appearance.nodeGlow.toFixed(2)}</span><input type="range" min="0.15" max="1" step="0.05" value={settings.appearance.nodeGlow} on:input={ev=>updateSettings(d=>{d.appearance.nodeGlow=Number(ev.currentTarget.value);})}/></label>
        {:else if activeTab==='nodes'}
          <div class="section__title">Desktop Nodes</div>
          <button class="chip" on:click={toggleDesktop}>{showDesktop?'Hide Desktop':'Show Desktop'}</button>
          <label class="toggle-row"><span>Background click-through</span><input type="checkbox" checked={settings.nodes.clickThrough} disabled={!supportsClickThrough} on:change={ev=>updateDesktopCT(ev.currentTarget.checked)}/></label>
          <label class="slider-row"><span>Smoothness: {settings.nodes.smoothness.toFixed(2)}</span><input type="range" min="0.08" max="0.45" step="0.01" value={settings.nodes.smoothness} on:input={ev=>updateSettings(d=>{d.nodes.smoothness=Number(ev.currentTarget.value);})}/></label>
          <div class="section__title" style="margin-top:14px;">Workspaces</div>
          <div class="template-row">
            <select on:change={ev=>switchWorkspace(ev.currentTarget.value)} value={activeWorkspaceId}>
              {#each workspaces as ws}<option value={ws.id}>{ws.name}</option>{/each}
            </select>
            {#if workspaces.length>1}<button class="chip chip--sm danger" on:click={()=>deleteWorkspace(activeWorkspaceId)}>Del</button>{/if}
          </div>
          <div class="template-row"><input bind:value={workspaceName} placeholder="New workspace name"/><button class="chip chip--sm" on:click={createWorkspace}>Create</button></div>
          <div class="section__title" style="margin-top:14px;">Layout</div>
          <button class="chip" on:click={layoutGrid}>Auto Grid Layout</button>
          <div class="section__title" style="margin-top:14px;">Node Manager</div>
          <div class="template-row">
            <select bind:value={selectedTemplate}>{#each nodeTemplates as t}<option value={t.id}>{t.name}</option>{/each}</select>
            <button class="chip chip--sm" on:click={addNode}>Add</button>
          </div>
          <div class="node-manager">
            {#each nodes as node, i (node.id)}
              {@const locked = isLockedNode(node)}
              <div class="node-row">
                <div class="node-row__title">{node.name||'(unnamed)'}</div>
                <div class="node-row__actions">
                  <button on:click={()=>moveNode(node.id,-1)} disabled={i===0}>↑</button>
                  <button on:click={()=>moveNode(node.id,1)} disabled={i===nodes.length-1}>↓</button>
                  <button on:click={()=>cloneNode(node.id)} disabled={locked}>⧉</button>
                  <button class="danger" on:click={()=>deleteNode(node.id)} disabled={locked}>×</button>
                </div>
              </div>
            {/each}
          </div>
        {:else if activeTab==='tray'}
          <div class="section__title">Quick Actions</div>
          <button class="chip" on:click={toggleStealth}>{stealth?'Disable Stealth':'Toggle Stealth'}</button>
          <button class="chip" on:click={toggleDesktop}>{showDesktop?'Hide Desktop':'Show Desktop'}</button>
          <button class="chip" on:click={hideToTray}>Hide To Tray</button>
          <button class="chip chip--danger" on:click={exitApp}>Exit</button>
        {:else if activeTab==='history'}
          <div class="section__title">Command History</div>
          {#if commandHistory.length===0}
            <div class="hint">No commands recorded yet.</div>
          {:else}
            <button class="chip chip--sm danger" on:click={async()=>{await invoke('clear_command_history');commandHistory=[];updateStatus('History cleared');}}>Clear All</button>
            <div class="activity-list">
              {#each commandHistory as h, i (i)}
                <div class="activity-item"><strong>{h.node_name}</strong> — {h.action} <span class="hint">{h.timestamp}</span></div>
              {/each}
            </div>
          {/if}
        {:else}
          <div class="section__title">Shortcuts</div>
          <div class="hint">Alt+S — Toggle Stealth</div>
          <div class="hint">Alt+I — Toggle Background Click-Through</div>
          <div class="hint">Alt+Space — Quick Launcher</div>
          <div class="hint">Ctrl+Click — Multi-select nodes</div>
          <div class="hint">Right-click node — Context menu</div>
        {/if}
      </div>
      <div class="rail__section meter">
        <div class="section__title">Activity</div>
        <div class="activity-list">
          {#if activityLog.length===0}<div class="hint">No activity yet.</div>
          {:else}{#each activityLog as item (item.id)}<div class="activity-item">{item.text}</div>{/each}{/if}
        </div>
      </div>
      <div class="status-bar status-bar--settings"><span>{statusText}</span><span class="status-dot"></span></div>
    </aside>
  </div>
{/if}

<style>
  :global(:root) { color-scheme:dark; --bg:#081321; --panel:rgba(9,18,31,0.88); --panel-strong:rgba(13,24,39,0.96); --line:rgba(120,227,255,0.25); --text:#e8f7ff; --muted:rgba(200,238,255,0.72); --glow:rgba(124,244,255,0.45); --accent:#7cf4ff; --accent-2:#9dffb9; --danger:#ff8fa3; --shadow:0 20px 56px rgba(0,0,0,0.42); --motion-scale:1; --node-glow:0.45; font-family:'Space Grotesk',sans-serif; }
  * { box-sizing:border-box; }
  :global(html),:global(body) { width:100%;height:100%;margin:0;overflow:hidden;color:var(--text); background:radial-gradient(circle at 18% 15%,rgba(52,201,255,0.2),transparent 34%),radial-gradient(circle at 88% 8%,rgba(157,255,185,0.16),transparent 28%),linear-gradient(145deg,#050b14 0%,#0a1628 55%,#050b14 100%); }
  :global(html.desktop-overlay-window),:global(body.desktop-overlay-window) { background:transparent; }
  :global(body.is-stealth) .rail { transform:translateX(-18px);opacity:0.15; }
  :global(body.is-stealth) .ghost-fin { opacity:1; }
  :global(#app) { width:100%;height:100%; }

  .settings-app { position:fixed;inset:0;display:flex;width:100%;height:100%;min-height:100vh; }
  .desktop-overlay { position:relative;width:100%;height:100%;background:transparent;pointer-events:none; }
  .settings-head { display:flex;justify-content:space-between;align-items:flex-start;gap:12px; }
  .window-controls { display:flex;gap:8px; }
  .window-controls__btn { width:34px;height:30px;border:1px solid rgba(124,244,255,0.28);border-radius:10px;background:rgba(8,15,26,0.82);color:var(--text);font:inherit;font-size:1rem;cursor:pointer; }
  .window-controls__btn:hover { border-color:rgba(124,244,255,0.45); }
  .window-controls__btn--danger { border-color:rgba(255,143,163,0.4);color:#ffd9e1; }
  .ghost-fin { border:0;padding:0;position:fixed;top:0;left:0;width:4px;height:100vh;background:linear-gradient(180deg,transparent,rgba(124,244,255,0.8),transparent);box-shadow:0 0 18px rgba(124,244,255,0.85);opacity:0;transition:opacity 180ms ease;z-index:30; }
  .rail { position:relative;display:flex;flex-direction:column;gap:14px;padding:20px;background:var(--panel-strong);border-right:1px solid rgba(124,244,255,0.2);transition:transform 260ms ease,opacity 260ms ease; }
  .rail--settings { width:100%;border-right:0;overflow-y:auto;flex:1;min-height:100%;height:100%;align-self:stretch; }
  .brand { display:flex;gap:14px;align-items:center; }
  .brand__mark { width:48px;height:48px;display:grid;place-items:center;border-radius:16px;background:rgba(7,14,24,0.9);box-shadow:0 0 18px rgba(0,0,0,0.4); }
  .brand__logo { width:28px;height:28px;object-fit:contain;filter:drop-shadow(0 0 8px rgba(124,244,255,0.3)); }
  .brand__name { font-size:1.32rem;font-weight:700;letter-spacing:0.04em; }
  .brand__tag,.section__title,.hint,.status-bar { color:var(--muted); }
  .rail__section { padding:16px;border:1px solid rgba(124,244,255,0.2);border-radius:18px;background:rgba(6,12,20,0.52); }
  .rail__tabs { display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:7px; }
  .tab { border:1px solid rgba(124,244,255,0.2);background:rgba(8,15,26,0.6);color:var(--muted);border-radius:11px;padding:9px 8px;font:inherit;font-size:0.76rem;cursor:pointer;transition:border-color 140ms ease,color 140ms ease; }
  .tab:hover { border-color:rgba(124,244,255,0.38); }
  .tab--active { color:var(--text);border-color:rgba(124,244,255,0.45);background:rgba(16,30,45,0.88);box-shadow:0 0 18px rgba(124,244,255,0.16); }
  .settings-center { display:flex;flex-direction:column;gap:8px;min-height:240px;flex:1;overflow-y:auto; }
  .toggle-row,.slider-row { display:flex;justify-content:space-between;align-items:center;gap:10px;color:rgba(233,248,255,0.92);font-size:0.84rem;margin-top:4px; }
  .toggle-row input { accent-color:#7cf4ff; }
  .slider-row input[type='range'] { width:46%; }
  .template-row { display:grid;grid-template-columns:1fr auto;gap:10px;margin-top:6px; }
  .template-row select,.template-row input,.node-editor-popup input,.node-editor-popup textarea,.node-editor-popup select { width:100%;border:1px solid rgba(124,244,255,0.22);background:rgba(8,15,26,0.8);color:var(--text);border-radius:12px;padding:8px 10px;font:inherit; }
  .theme-row { display:flex;gap:8px;flex-wrap:wrap; }
  .node-manager { display:flex;flex-direction:column;gap:8px;max-height:180px;overflow:auto;margin-top:8px; }
  .node-row { display:flex;justify-content:space-between;align-items:center;gap:10px;padding:9px 10px;border:1px solid rgba(124,244,255,0.12);border-radius:12px;background:rgba(8,14,23,0.58); }
  .node-row__title { font-size:0.82rem;font-weight:600;max-width:120px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap; }
  .node-row__actions { display:flex;gap:6px; }
  .node-row__actions button { border:1px solid rgba(124,244,255,0.2);background:rgba(8,15,26,0.78);color:var(--text);border-radius:8px;padding:4px 8px;font-size:0.72rem;cursor:pointer; }
  .node-row__actions button:disabled { opacity:0.45;cursor:not-allowed; }
  .node-row__actions .danger { border-color:rgba(255,143,163,0.4);color:#ffd9e1; }
  .activity-list { display:flex;flex-direction:column;gap:6px;max-height:110px;overflow:auto; }
  .activity-item { font-size:0.76rem;line-height:1.35;color:rgba(233,248,255,0.8);border-bottom:1px dashed rgba(124,244,255,0.16);padding-bottom:6px; }
  .section__title { margin-bottom:10px;text-transform:uppercase;letter-spacing:0.16em;font-size:0.72rem; }
  .chip,.node__actions button,.node-editor-popup__actions button { border:1px solid rgba(124,244,255,0.22);background:rgba(8,15,26,0.78);color:var(--text);border-radius:12px;padding:9px 12px;font:inherit;font-size:0.8rem;cursor:pointer;transition:transform 140ms ease,border-color 140ms ease,box-shadow 140ms ease; }
  .chip:hover,.node__actions button:hover { transform:translateY(-1px);border-color:rgba(124,244,255,0.45);box-shadow:0 0 18px rgba(124,244,255,0.16); }
  .chip { width:100%;margin-top:4px; }
  .chip--sm { width:auto;padding:6px 10px;font-size:0.74rem; }
  .chip--active { border-color:var(--accent);background:rgba(124,244,255,0.12);color:var(--accent); }
  .chip--danger,.danger { border-color:rgba(255,143,163,0.4);color:#ffd9e1; }
  .chip:disabled { opacity:0.45;cursor:not-allowed; }
  .stage { position:relative;width:100%;height:100%;overflow:hidden;transition:opacity calc(220ms * var(--motion-scale)) ease; }
  .stage--hidden { opacity:0;pointer-events:none; }
  .links,.node-layer { position:absolute;inset:0; }
  .links { pointer-events:none;z-index:1; }
  .link { fill:none;stroke:var(--line);stroke-width:2;stroke-linecap:round;stroke-linejoin:round;transition:stroke 200ms ease,stroke-width 200ms ease; }
  .link--highlight { stroke:var(--accent);stroke-width:3;filter:drop-shadow(0 0 8px var(--glow)); }
  .node-layer { padding:26px;pointer-events:none;transform-origin:0 0; }
  .node { position:absolute;width:84px;height:84px;aspect-ratio:1 / 1;border-radius:50%;overflow:hidden;cursor:grab;user-select:none;will-change:left,top;z-index:2;pointer-events:auto;transition:left calc(120ms * var(--motion-scale)) cubic-bezier(0.22,0.61,0.36,1),top calc(120ms * var(--motion-scale)) cubic-bezier(0.22,0.61,0.36,1); }
  .node:not(.node--expanded) { min-height:84px;max-height:84px; }
  .node:not(.node--expanded) .node__surface,
  .node:not(.node--expanded) .node__surface::after { border-radius:50%; }
  .node.node--expanded { width:272px;height:auto;min-height:190px;border-radius:22px;z-index:5; }
  .node--selected .node__surface { border-color:var(--accent) !important;box-shadow:0 0 20px var(--glow),0 12px 28px rgba(0,0,0,0.32) !important; }
  .node__surface { position:relative;width:100%;height:100%;display:grid;place-items:center;padding:12px;border-radius:inherit;background:linear-gradient(180deg,rgba(18,27,41,0.94),rgba(10,15,24,0.82));border:1px solid rgba(var(--nc),0.25);box-shadow:0 12px 28px rgba(0,0,0,0.32),0 0 calc(14px * var(--node-glow)) rgba(var(--nc),calc(0.14 * var(--node-glow))) inset;transition:transform calc(140ms * var(--motion-scale)) ease; }
  .node__surface::after { content:'';position:absolute;inset:0;border-radius:inherit;box-shadow:0 0 calc(10px * var(--node-glow)) rgba(var(--nc),calc(0.1 * var(--node-glow)));pointer-events:none; }
  .node--expanded .node__surface { display:flex;flex-direction:column;min-height:190px;padding:16px;align-items:stretch; }
  .node__compact-title { max-width:90%;text-align:center;font-size:0.74rem;font-weight:700;line-height:1.15;color:rgba(233,248,255,0.95);text-shadow:0 0 12px rgba(var(--nc),0.26); }
  :global(.node.is-dragging) { cursor:grabbing;transition:none; }
  :global(.node.is-dragging) .node__surface { transform:scale(1.02); }
  .node__status { position:absolute;top:6px;right:6px;width:8px;height:8px;border-radius:50%; }
  .node__status--active { background:#69f0ae;box-shadow:0 0 8px rgba(105,240,174,0.6); }
  .node__status--idle { background:rgba(200,238,255,0.35); }
  .node__header { display:flex;gap:10px;align-items:center; }
  .node__header-copy { min-width:0;flex:1; }
  .node__icon { width:42px;height:42px;display:grid;place-items:center;border-radius:14px;background:rgba(var(--nc),0.12);color:rgb(var(--nc));box-shadow:0 0 15px rgba(var(--nc),0.18); }
  .node__logo { width:22px;height:22px;object-fit:contain;filter:drop-shadow(0 0 8px rgba(var(--nc),0.35)); }
  .node__edit-trigger { border:1px solid rgba(var(--nc),0.25);background:rgba(7,14,24,0.86);color:var(--text);border-radius:10px;padding:5px 10px;font:inherit;font-size:0.76rem;cursor:pointer; }
  .node__name { font-size:1.05rem;font-weight:700; }
  .node__meta { color:var(--muted);font-size:0.82rem; }
  .node__body { margin:12px 0;color:rgba(233,248,255,0.82);line-height:1.4;font-size:0.82rem; }
  .node__actions { display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:8px; }
  .node__macro-btn { margin-top:8px;border:1px solid rgba(var(--nc),0.3);background:rgba(var(--nc),0.08);color:rgb(var(--nc));border-radius:10px;padding:7px 10px;font:inherit;font-size:0.74rem;cursor:pointer; }
  .status-bar { display:flex;justify-content:space-between;align-items:center;padding:12px 14px;border-radius:14px;background:rgba(5,9,16,0.58);border:1px solid rgba(124,244,255,0.2);color:var(--muted);font-size:0.82rem; }
  .status-bar--settings { margin-top:auto; }
  .status-dot { width:10px;height:10px;border-radius:50%;background:var(--accent-2);box-shadow:0 0 16px rgba(157,255,185,0.65);animation:pulse 2.4s ease-in-out infinite; }
  .meter { min-height:0; }
  .hint { line-height:1.5;margin-top:4px;font-size:0.78rem; }
  .fatal { white-space:pre-wrap;margin:0;padding:24px;color:#ffb4c2; }

  /* Editor popup */
  .node-editor-popup { position:fixed;width:320px;max-height:min(480px,calc(100vh - 24px));overflow:auto;z-index:130;border:1px solid rgba(124,244,255,0.35);border-radius:14px;background:rgba(8,14,24,0.96);box-shadow:0 20px 48px rgba(0,0,0,0.5),0 0 20px rgba(124,244,255,0.18);backdrop-filter:blur(16px);padding:12px;display:flex;flex-direction:column;gap:8px;pointer-events:auto; }
  .node-editor-popup__title { font-size:0.88rem;font-weight:700;margin-bottom:2px; }
  .node-editor-popup label { display:flex;flex-direction:column;gap:5px; }
  .node-editor-popup label > span { font-size:0.74rem;color:var(--muted); }
  .node-editor-popup textarea { resize:vertical;min-height:48px; }
  .node-editor-popup__links { border-top:1px solid rgba(124,244,255,0.15);margin-top:4px;padding-top:8px;display:flex;flex-direction:column;gap:6px; }
  .node-editor-popup__actions { display:grid;grid-template-columns:repeat(2,1fr);gap:8px;margin-top:6px; }
  .node-editor-popup__actions .ghost { background:rgba(10,18,28,0.65); }
  .link-toggle { display:flex;justify-content:space-between;align-items:center;gap:8px;font-size:0.78rem; }
  .link-toggle input { accent-color:#7cf4ff; }
  .color-picker { display:flex;gap:6px;flex-wrap:wrap; }
  .color-dot { width:24px;height:24px;border-radius:50%;border:2px solid transparent;background:rgb(var(--dc));cursor:pointer;transition:border-color 120ms; }
  .color-dot--active { border-color:var(--text);box-shadow:0 0 8px rgba(var(--dc),0.5); }
  .macro-row { display:grid;grid-template-columns:auto 1fr auto;gap:6px;align-items:center; }
  .macro-row select { width:auto; }

  /* Context menu */
  .context-menu { position:fixed;z-index:120;min-width:200px;border:1px solid rgba(124,244,255,0.35);border-radius:14px;background:rgba(8,14,24,0.96);box-shadow:0 20px 48px rgba(0,0,0,0.5);padding:10px;display:flex;flex-direction:column;gap:4px;pointer-events:auto; }
  .context-menu__title { font-size:0.82rem;font-weight:700;padding:4px 8px 8px;border-bottom:1px solid rgba(124,244,255,0.15);margin-bottom:2px; }
  .context-menu button { border:none;background:transparent;color:var(--text);border-radius:8px;padding:8px 10px;font:inherit;font-size:0.78rem;cursor:pointer;text-align:left; }
  .context-menu button:hover { background:rgba(124,244,255,0.12); }
  .context-menu button.danger:hover { background:rgba(255,143,163,0.12); }

  /* Batch bar */
  .batch-bar { position:fixed;top:20px;left:50%;transform:translateX(-50%);display:flex;gap:10px;align-items:center;padding:10px 16px;border:1px solid rgba(124,244,255,0.35);border-radius:14px;background:rgba(8,14,24,0.96);backdrop-filter:blur(16px);z-index:100;pointer-events:auto;font-size:0.82rem; }
  .batch-bar button { border:1px solid rgba(124,244,255,0.22);background:rgba(8,15,26,0.78);color:var(--text);border-radius:10px;padding:6px 12px;font:inherit;font-size:0.76rem;cursor:pointer; }

  /* Quick launcher */
  .launcher-overlay { position:fixed;inset:0;background:rgba(0,0,0,0.5);display:grid;place-items:center;z-index:200;pointer-events:auto; }
  .launcher { width:min(480px,90vw);border:1px solid rgba(124,244,255,0.35);border-radius:18px;background:rgba(8,14,24,0.98);box-shadow:0 20px 60px rgba(0,0,0,0.6);padding:12px;backdrop-filter:blur(20px); }
  .launcher__input { width:100%;border:1px solid rgba(124,244,255,0.22);background:rgba(8,15,26,0.8);color:var(--text);border-radius:14px;padding:14px 18px;font:inherit;font-size:1.1rem;outline:none; }
  .launcher__input:focus { border-color:rgba(124,244,255,0.45); }
  .launcher__results { display:flex;flex-direction:column;gap:4px;margin-top:8px;max-height:300px;overflow:auto; }
  .launcher__item { border:none;background:transparent;color:var(--text);border-radius:10px;padding:10px 14px;font:inherit;font-size:0.88rem;text-align:left;cursor:pointer;display:flex;align-items:center;gap:10px; }
  .launcher__icon { width:20px;height:20px;display:grid;place-items:center;object-fit:contain; }
  .launcher__item:hover,.launcher__item--active { background:rgba(124,244,255,0.1); }

  /* Tooltip */
  .tooltip { position:fixed;z-index:100;padding:8px 12px;border-radius:10px;background:rgba(8,14,24,0.96);border:1px solid rgba(124,244,255,0.25);color:var(--text);font-size:0.76rem;pointer-events:none;transform:translateX(-50%) translateY(-100%);white-space:nowrap;max-width:300px;overflow:hidden;text-overflow:ellipsis; }

  @keyframes pulse { 0%,100%{opacity:0.45;transform:scale(0.82);} 50%{opacity:1;transform:scale(1.08);} }
</style>
