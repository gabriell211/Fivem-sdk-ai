import { invoke } from '@tauri-apps/api/core';

export interface WorkspaceFile {
  path: string;
  size: number;
}

export interface FxServerStatus {
  installed: boolean;
  running: boolean;
  build?: string | null;
  executable?: string | null;
}

export interface NuiTarget {
  id: string;
  title: string;
  url: string;
  targetType: string;
  webSocketDebuggerUrl: string;
}

export interface ScreenshotEvidence {
  path: string;
  size: number;
  sha256: string;
  mime: string;
  dataUrl: string;
}

export interface AgentRequest {
  workspace: string;
  prompt: string;
  endpoint: string;
  model: string;
  apiKey?: string;
}

export const api = {
  listFiles: (workspace: string) =>
    invoke<WorkspaceFile[]>('list_workspace_files', { workspace }),
  readFile: (workspace: string, path: string) =>
    invoke<string>('read_workspace_file', { workspace, path }),
  writeFile: (workspace: string, path: string, content: string) =>
    invoke<void>('write_workspace_file', { workspace, path, content }),
  bootstrapWorkspace: (workspace: string) =>
    invoke<void>('bootstrap_workspace', { workspace }),
  installFxServer: () => invoke<FxServerStatus>('install_fxserver'),
  serverStatus: () => invoke<FxServerStatus>('fxserver_status'),
  startServer: (workspace: string, licenseKey: string) =>
    invoke<void>('start_fxserver', { workspace, licenseKey }),
  stopServer: () => invoke<void>('stop_fxserver'),
  serverCommand: (command: string) => invoke<void>('fxserver_command', { command }),
  connectFiveM: () => invoke<void>('connect_fivem'),
  runBridgeTest: (action: string, args: Record<string, unknown> = {}) =>
    invoke<Record<string, unknown>>('run_bridge_test', { action, args }),
  captureScreenshot: (workspace: string) =>
    invoke<ScreenshotEvidence>('capture_screenshot', { workspace }),
  nuiTargets: () => invoke<NuiTarget[]>('nui_targets'),
  openNuiDevtools: () => invoke<void>('open_nui_devtools'),
  runAgent: (request: AgentRequest) => invoke<string>('run_agent', { request }),
};
