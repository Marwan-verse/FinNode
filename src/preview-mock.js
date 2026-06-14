/*
 * Web-only preview bridge.
 *
 * When FinNode runs inside its Tauri shell the real Rust commands are used.
 * When the same frontend is served as a plain web page (`npm run dev`, or the
 * Vite preview), there is no IPC backend — every `invoke` would reject and the
 * board would render empty. This module provides an in-memory stand-in so the
 * documented web preview is actually usable: it seeds a realistic demo layout
 * and answers the command surface the UI depends on.
 *
 * It is a no-op in production: `HAS_TAURI` short-circuits to the real bridge.
 */

export const HAS_TAURI =
  typeof window !== 'undefined' && Boolean(window.__TAURI_IPC__ || window.__TAURI__);

function demoLayout() {
  return {
    active_workspace: 'client',
    workspaces: [
      {
        id: 'client',
        name: 'Client Project',
        zoom: 1, pan_x: 0, pan_y: 0,
        groups: [
          { id: 'Frontend', name: 'Frontend', color: 'cyan',   collapsed: false },
          { id: 'Backend',  name: 'Backend',  color: 'violet', collapsed: false }
        ],
        nodes: [
          { id: 'main-node', name: 'main', icon: 'logo', description: 'Core entry node',
            x: 360, y: 60, color: 'cyan', locked: true, links: ['web', 'api'] },
          { id: 'web', name: 'Web App', icon: '🪟', description: 'Svelte client. Opens folder + editor + localhost.',
            x: 150, y: 200, color: 'cyan', group: 'Frontend', launch_count: 14,
            last_launched: new Date(Date.now() - 1000 * 60 * 6).toISOString(),
            links: ['design'],
            targets: { path: '~/work/web', editor: 'code ~/work/web', browser: 'http://localhost:5173', script: 'npm run dev' } },
          { id: 'design', name: 'Design System', icon: '🎨', description: 'Figma + Storybook.',
            x: 150, y: 360, color: 'rose', group: 'Frontend', launch_count: 7,
            last_launched: new Date(Date.now() - 1000 * 60 * 90).toISOString(),
            targets: { browser: 'https://www.figma.com' } },
          { id: 'api', name: 'API Server', icon: '⚙', description: 'Rust API. Runs the dev server.',
            x: 600, y: 210, color: 'green', group: 'Backend', node_type: 'script', launch_count: 9,
            last_launched: new Date(Date.now() - 1000 * 60 * 22).toISOString(),
            links: ['db', 'staging'],
            targets: { path: '~/work/api', script: 'cargo run' } },
          { id: 'db', name: 'Database', icon: '🗄', description: 'Postgres console + backups.',
            x: 770, y: 360, color: 'violet', group: 'Backend', launch_count: 5,
            last_launched: new Date(Date.now() - 1000 * 60 * 60 * 5).toISOString(),
            targets: { path: '~/work/db' } },
          { id: 'staging', name: 'Staging', icon: '🌐', description: 'Live staging environment.',
            x: 600, y: 360, color: 'cyan', group: 'Backend', launch_count: 4,
            targets: { browser: 'https://staging.example.com' } },
          { id: 'deploy', name: 'Deploy', icon: '🚀', description: 'Tagged release macro with a prompt.',
            x: 410, y: 470, color: 'amber', launch_count: 3,
            macros: [
              { action: 'prompt-input', value: 'Release tag' },
              { action: 'type-text', value: 'git tag {{input}} && git push --tags' },
              { action: 'keyboard-shortcut', value: 'Enter' }
            ],
            targets: { path: '~/work', script: 'npm run release' } },
          { id: 'docs', name: 'Docs', icon: '📚', description: 'Project documentation.',
            x: 360, y: 320, color: 'slate', launch_count: 2,
            targets: { browser: 'https://docs.example.com' } }
        ]
      },
      {
        id: 'personal',
        name: 'Personal',
        zoom: 1, pan_x: 0, pan_y: 0, groups: [],
        nodes: [
          { id: 'main-node', name: 'main', icon: 'logo', x: 200, y: 80, color: 'cyan', locked: true, links: [] },
          { id: 'notes', name: 'Notes', icon: '📝', x: 120, y: 220, color: 'green', launch_count: 11,
            targets: { path: '~/notes', editor: 'code ~/notes' } }
        ]
      }
    ],
    command_history: [],
    settings: { start_on_boot: true, run_macro_on_system_start: false, theme: 'cyan', board_background: 'dots' }
  };
}

let layout = demoLayout();

function clone(value) {
  return JSON.parse(JSON.stringify(value));
}
function randId(prefix) {
  return `${prefix}-${Math.random().toString(36).slice(2, 8)}`;
}

export async function previewInvoke(cmd, args = {}) {
  switch (cmd) {
    case 'load_layout':
      return clone(layout);
    case 'save_layout':
      if (args?.layout) {
        layout = { ...args.layout, settings: layout.settings };
      }
      return null;
    case 'get_app_settings':
      return clone(layout.settings);
    case 'set_start_on_boot':
      layout.settings.start_on_boot = Boolean(args?.enabled);
      return null;
    case 'set_run_macro_on_system_start':
      layout.settings.run_macro_on_system_start = Boolean(args?.enabled);
      return null;
    case 'set_ui_preferences':
      if (args?.theme) layout.settings.theme = args.theme;
      if (args?.boardBackground) layout.settings.board_background = args.boardBackground;
      return null;
    case 'list_workspaces':
      return clone(layout.workspaces);
    case 'create_workspace': {
      const ws = { id: randId('ws'), name: args?.name || 'Workspace', nodes: [], zoom: 1, pan_x: 0, pan_y: 0, groups: [] };
      layout.workspaces.push(ws);
      return clone(ws);
    }
    case 'switch_workspace':
      if (args?.workspaceId) layout.active_workspace = args.workspaceId;
      return clone(layout);
    case 'delete_workspace':
      layout.workspaces = layout.workspaces.filter(w => w.id !== args?.workspaceId);
      if (layout.active_workspace === args?.workspaceId) {
        layout.active_workspace = layout.workspaces[0]?.id ?? 'default';
      }
      return null;
    case 'rename_workspace':
      layout.workspaces = layout.workspaces.map(w => w.id !== args?.workspaceId ? w : { ...w, name: args?.name ?? w.name });
      return null;
    case 'export_workspace':
      return `~/AppData/Roaming/FinNode/exports/${args?.workspaceId || 'workspace'}-${Date.now()}.finnode.json`;
    case 'save_uploaded_script':
      return { path: `~/scripts/${args?.fileName || 'script'}`, name: args?.fileName || 'script' };
    case 'get_command_history':
      return clone(layout.command_history || []);
    case 'get_platform':
      return 'web';
    // Window/desktop/macro side effects are inert in the web preview.
    case 'launch_node':
    case 'run_node_macro':
    case 'clear_command_history':
    case 'set_stealth_mode':
    case 'set_desktop_visibility':
    case 'set_desktop_click_through':
    case 'update_node_bounds':
    case 'hide_main_window':
    case 'show_main_window':
    case 'show_settings_view':
    case 'exit_app':
    case 'pin_desktop_bottom':
    case 'set_window_bottom':
      return null;
    default:
      return null;
  }
}

export async function previewListen() {
  return () => {};
}
