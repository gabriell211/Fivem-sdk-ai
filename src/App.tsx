import Editor from '@monaco-editor/react';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import { useEffect, useMemo, useState } from 'react';
import { api, type FxServerStatus, type NuiTarget, type ScreenshotEvidence, type WorkspaceFile } from './api';

type LogEvent = { stream: 'stdout' | 'stderr' | 'system'; line: string };

const languageFor = (path: string) => {
  const ext = path.split('.').pop()?.toLowerCase();
  if (ext === 'lua') return 'lua';
  if (ext === 'ts' || ext === 'tsx') return 'typescript';
  if (ext === 'js' || ext === 'jsx') return 'javascript';
  if (ext === 'json') return 'json';
  if (ext === 'css') return 'css';
  if (ext === 'html') return 'html';
  if (ext === 'md') return 'markdown';
  return 'plaintext';
};

export default function App() {
  const [workspace, setWorkspace] = useState('');
  const [files, setFiles] = useState<WorkspaceFile[]>([]);
  const [activePath, setActivePath] = useState('');
  const [content, setContent] = useState('');
  const [dirty, setDirty] = useState(false);
  const [logs, setLogs] = useState<LogEvent[]>([]);
  const [status, setStatus] = useState<FxServerStatus>({ installed: false, running: false });
  const [licenseKey, setLicenseKey] = useState('');
  const [agentPrompt, setAgentPrompt] = useState('Analise o resource aberto, corrija problemas e teste no FXServer.');
  const [agentResult, setAgentResult] = useState('');
  const [endpoint, setEndpoint] = useState('http://127.0.0.1:11434/v1');
  const [model, setModel] = useState('qwen3-coder:latest');
  const [apiKey, setApiKey] = useState('');
  const [busy, setBusy] = useState(false);
  const [nuiTargets, setNuiTargets] = useState<NuiTarget[]>([]);
  const [screenshot, setScreenshot] = useState<ScreenshotEvidence | null>(null);
  const [vehicleModel, setVehicleModel] = useState('adder');
  const [teleportText, setTeleportText] = useState('-75.0, -818.0, 326.0, 180.0');

  const language = useMemo(() => languageFor(activePath), [activePath]);

  const refreshStatus = async () => setStatus(await api.serverStatus());
  const refreshFiles = async (root = workspace) => {
    if (!root) return;
    setFiles(await api.listFiles(root));
  };

  useEffect(() => {
    void refreshStatus();
    const unlistenPromise = listen<LogEvent>('fxserver://log', (event) => {
      setLogs((current) => [...current.slice(-999), event.payload]);
    });
    return () => {
      void unlistenPromise.then((unlisten) => unlisten());
    };
  }, []);

  const chooseWorkspace = async () => {
    const selected = await open({ directory: true, multiple: false, title: 'Abrir projeto FiveM' });
    if (typeof selected !== 'string') return;
    setWorkspace(selected);
    setActivePath('');
    setContent('');
    await refreshFiles(selected);
  };

  const openFile = async (path: string) => {
    if (!workspace) return;
    if (dirty && activePath && !window.confirm('Descartar alterações não salvas?')) return;
    setContent(await api.readFile(workspace, path));
    setActivePath(path);
    setDirty(false);
  };

  const save = async () => {
    if (!workspace || !activePath) return;
    await api.writeFile(workspace, activePath, content);
    setDirty(false);
  };

  const withBusy = async (task: () => Promise<void>) => {
    setBusy(true);
    try {
      await task();
    } catch (error) {
      setLogs((current) => [...current, { stream: 'system', line: String(error) }]);
    } finally {
      setBusy(false);
      await refreshStatus();
    }
  };

  const runScenario = (action: string, args: Record<string, unknown> = {}) => withBusy(async () => {
    const result = await api.runBridgeTest(action, args);
    setAgentResult(JSON.stringify(result, null, 2));
  });

  const teleport = () => {
    const values = teleportText.split(',').map((value) => Number(value.trim()));
    if (values.length < 3 || values.slice(0, 3).some((value) => !Number.isFinite(value))) {
      setAgentResult('Coordenadas inválidas. Use: x, y, z, heading');
      return;
    }
    void runScenario('teleport', {
      x: values[0],
      y: values[1],
      z: values[2],
      ...(Number.isFinite(values[3]) ? { heading: values[3] } : {}),
    });
  };

  const captureVisual = () => withBusy(async () => {
    if (!workspace) throw new Error('Abra um workspace primeiro.');
    const evidence = await api.captureScreenshot(workspace);
    setScreenshot(evidence);
    setAgentResult(`Screenshot validado: ${evidence.path}\nSHA-256: ${evidence.sha256}\n${evidence.size} bytes`);
  });

  const refreshNui = () => withBusy(async () => {
    const targets = await api.nuiTargets();
    setNuiTargets(targets);
    setAgentResult(targets.length
      ? `NUI/CEF: ${targets.length} target(s) detectado(s).\n${targets.map((target) => `- ${target.title || '(sem título)'} — ${target.url}`).join('\n')}`
      : 'Nenhum target NUI/CEF detectado. Abra o FiveM e um resource com NUI.');
  });

  const runAgent = () => withBusy(async () => {
    if (!workspace) throw new Error('Abra um workspace primeiro.');
    const result = await api.runAgent({ workspace, prompt: agentPrompt, endpoint, model, apiKey: apiKey || undefined });
    setAgentResult(result);
    await refreshFiles();
    if (activePath) setContent(await api.readFile(workspace, activePath));
  });

  return (
    <main className="app-shell">
      <header className="topbar">
        <div className="brand"><span className="brand-mark">F</span><div><strong>FiveM SDK AI</strong><small>{workspace || 'Nenhum projeto aberto'}</small></div></div>
        <div className="actions">
          <button onClick={chooseWorkspace}>Abrir projeto</button>
          <button disabled={!workspace || busy} onClick={() => withBusy(async () => { await api.bootstrapWorkspace(workspace); await refreshFiles(); })}>Preparar</button>
          <button disabled={busy} onClick={() => withBusy(async () => { await api.installFxServer(); })}>{status.installed ? 'Atualizar FXServer' : 'Instalar FXServer'}</button>
          <button className="primary" disabled={!workspace || !status.installed || status.running || busy} onClick={() => withBusy(async () => api.startServer(workspace, licenseKey))}>Start</button>
          <button disabled={!status.running || busy} onClick={() => withBusy(api.stopServer)}>Stop</button>
          <button disabled={!status.running} onClick={() => withBusy(api.connectFiveM)}>Entrar no jogo</button>
        </div>
      </header>

      <section className="workspace-grid">
        <aside className="sidebar panel">
          <div className="panel-title">Explorer <button className="icon" onClick={() => void refreshFiles()}>↻</button></div>
          <div className="file-list">
            {files.map((file) => <button key={file.path} className={file.path === activePath ? 'file active' : 'file'} onClick={() => void openFile(file.path)} title={`${file.size} bytes`}>{file.path}</button>)}
          </div>
        </aside>

        <section className="editor-stack panel">
          <div className="tabs"><span>{activePath || 'Abra um arquivo'}</span>{dirty && <span className="dirty">●</span>}<button disabled={!dirty} onClick={() => void save()}>Salvar</button></div>
          <Editor
            height="100%"
            theme="vs-dark"
            language={language}
            value={content}
            onChange={(value) => { setContent(value ?? ''); setDirty(true); }}
            options={{ minimap: { enabled: false }, fontSize: 14, automaticLayout: true, scrollBeyondLastLine: false, tabSize: 2 }}
          />
        </section>

        <aside className="agent panel">
          <div className="panel-title">AI Test Engineer</div>
          <label>Endpoint<input value={endpoint} onChange={(e) => setEndpoint(e.target.value)} /></label>
          <label>Modelo<input value={model} onChange={(e) => setModel(e.target.value)} /></label>
          <label>API key <input type="password" autoComplete="off" value={apiKey} onChange={(e) => setApiKey(e.target.value)} placeholder="opcional para servidor local" /></label>
          <textarea value={agentPrompt} onChange={(e) => setAgentPrompt(e.target.value)} />
          <button className="primary" disabled={!workspace || busy} onClick={() => void runAgent()}>Executar agente</button>
          <div className="test-actions">
            <button disabled={!status.running || busy} onClick={() => void runScenario('ping')}>Ping in-game</button>
            <button disabled={!status.running || busy} onClick={() => void runScenario('snapshot')}>Snapshot player</button>
            <button disabled={!status.running || busy} onClick={() => void captureVisual()}>Screenshot real</button>
            <button disabled={!status.running || busy} onClick={() => void refreshNui()}>Detectar NUI</button>
            <button disabled={!status.running} onClick={() => void api.openNuiDevtools()}>NUI DevTools</button>
          </div>
          <div className="scenario-controls">
            <label>Teleport (x, y, z, heading)
              <input value={teleportText} onChange={(e) => setTeleportText(e.target.value)} />
            </label>
            <button disabled={!status.running || busy} onClick={teleport}>Teleportar e validar</button>
            <label>Veículo de teste
              <input value={vehicleModel} onChange={(e) => setVehicleModel(e.target.value)} />
            </label>
            <div className="scenario-buttons">
              <button disabled={!status.running || busy || !vehicleModel.trim()} onClick={() => void runScenario('spawn_vehicle', { model: vehicleModel.trim(), warp: true })}>Spawn + entrar</button>
              <button disabled={!status.running || busy} onClick={() => void runScenario('cleanup')}>Cleanup</button>
            </div>
          </div>
          {screenshot && (
            <figure className="screenshot-evidence">
              <img src={screenshot.dataUrl} alt="Screenshot capturado do cliente FiveM em execução" />
              <figcaption>{screenshot.path} · {Math.round(screenshot.size / 1024)} KiB</figcaption>
            </figure>
          )}
          {nuiTargets.length > 0 && (
            <div className="nui-targets">
              {nuiTargets.slice(0, 8).map((target) => (
                <div key={target.id || target.url} title={target.url}>
                  <strong>{target.title || target.targetType || 'CEF target'}</strong>
                  <span>{target.url}</span>
                </div>
              ))}
            </div>
          )}
          <pre className="agent-result">{agentResult || 'A resposta e os testes da IA aparecem aqui.'}</pre>
        </aside>

        <section className="console panel">
          <div className="panel-title"><span>FXServer Console</span><span className={status.running ? 'badge online' : 'badge'}>{status.running ? 'RUNNING' : 'STOPPED'}{status.build ? ` · ${status.build}` : ''}</span></div>
          <pre>{logs.map((log, index) => <span key={`${index}-${log.line}`} className={`log-${log.stream}`}>{log.line}\n</span>)}</pre>
          <div className="console-inputs">
            <input type="password" value={licenseKey} onChange={(e) => setLicenseKey(e.target.value)} placeholder="Cfx.re license key (somente para iniciar)" />
          </div>
        </section>
      </section>
    </main>
  );
}
