import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { listen } from "@tauri-apps/api/event";
import {
  Check,
  BookOpen,
  CircleHelp,
  Clock3,
  Copy,
  Database,
  History,
  Mic,
  RotateCcw,
  Search,
  Settings as SettingsIcon,
  Sparkles,
  Trash2,
  Wifi,
  WifiOff
} from "lucide-react";
import type { AppSettings, AppStatus, DictationResult, HistoryEntry, StatusPayload } from "./types";
import { DEFAULT_CLEANUP_PROMPT } from "./types";
import { DictationShortcut } from "./shortcut";
import { ShortcutInput } from "./ShortcutInput";
import { displayShortcut } from "./shortcutCapture";

const DEFAULT_SETTINGS: AppSettings = {
  shortcut: "Ctrl+Alt+Space",
  activationBehavior: "toggle",
  microphoneName: null,
  language: "auto",
  model: "tiny",
  transcriptionMode: "local",
  transcriptionProvider: "whisper-cpp",
  transcriptionBaseUrl: "",
  transcriptionModel: "",
  transcriptionTimeoutSeconds: 90,
  transcriptionApiKeyConfigured: false,
  cleanupEnabled: false,
  provider: "none",
  providerBaseUrl: "http://127.0.0.1:11434",
  providerModel: "",
  providerApiKeyConfigured: false,
  cleanupPrompt: DEFAULT_CLEANUP_PROMPT,
  launchAtStartup: false
};

type Page = "home" | "history" | "settings" | "about";

function sourceLabel(source?: string): string {
  if (source?.startsWith("local:")) return `Local Whisper ${source.slice(6) === "base" ? "Base" : "Tiny"}`;
  if (source === "mixed:remote+local") return "Remote + local fallback";
  if (source === "remote:whisper-cpp") return "Your Whisper server";
  if (source?.startsWith("remote:")) return "Remote transcription";
  return "Source not recorded";
}

function seconds(milliseconds: number): string { return (milliseconds / 1000).toFixed(1); }

function App() {
  const [page, setPage] = useState<Page>("home");
  const [appVersion, setAppVersion] = useState("0.2.3");
  const [status, setStatus] = useState<AppStatus>("idle");
  const [statusText, setStatusText] = useState("Ready to dictate");
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [savedSettings, setSavedSettings] = useState<AppSettings | null>(null);
  const [savingSettings, setSavingSettings] = useState(false);
  const [activeShortcut, setActiveShortcut] = useState(DEFAULT_SETTINGS.shortcut);
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [latestResult, setLatestResult] = useState<DictationResult | null>(null);
  const [commandPending, setCommandPending] = useState(false);
  const [refreshingMicrophones, setRefreshingMicrophones] = useState(false);
  const [microphones, setMicrophones] = useState<string[]>([]);
  const [models, setModels] = useState<string[]>([]);
  const [baseInstalled, setBaseInstalled] = useState(false);
  const [installingBase, setInstallingBase] = useState(false);
  const [query, setQuery] = useState("");
  const [connectionState, setConnectionState] = useState<"idle" | "testing" | "ok" | "error">("idle");
  const [transcriptionConnectionState, setTranscriptionConnectionState] = useState<"idle" | "testing" | "ok" | "error">("idle");
  const [message, setMessage] = useState("");
  const savedSettingsRef = useRef<AppSettings>(DEFAULT_SETTINGS);
  const [cleanupTestState, setCleanupTestState] = useState<"idle" | "testing" | "ok" | "error">("idle");
  const providerTestGeneration = useRef(0);
  const processing = useRef(false);
  const statusRef = useRef<AppStatus>("idle");
  const startRecordingRef = useRef<() => Promise<void>>(async () => undefined);
  const stopAndProcessRef = useRef<() => Promise<void>>(async () => undefined);
  const settingsReady = useRef(false);
  const statusRevision = useRef(-1);

  useEffect(() => {
    getVersion().then(setAppVersion).catch(() => undefined);
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const apply = (next: StatusPayload) => {
      if (disposed || next.revision < statusRevision.current) return;
      statusRevision.current = next.revision;
      statusRef.current = next.status;
      setStatus(next.status);
      setStatusText(next.text);
    };
    listen<StatusPayload>("dictation-status", (event) => apply(event.payload)).then(async (stop) => {
      if (disposed) { stop(); return; }
      unlisten = stop;
      apply(await invoke<StatusPayload>("get_current_status"));
    }).catch((error) => setMessage(`Status updates unavailable: ${String(error)}`));
    return () => { disposed = true; unlisten?.(); };
  }, []);

  const refreshHistory = useCallback(async () => {
    setHistory(await invoke<HistoryEntry[]>("list_history"));
  }, []);

  useEffect(() => {
    Promise.all([
      invoke<AppSettings>("get_settings"),
      invoke<string[]>("list_microphones"),
      invoke<HistoryEntry[]>("list_history"),
      invoke<boolean>("is_base_model_installed")
    ]).then(([saved, devices, entries, installed]) => {
      const displayedSettings = installed || saved.model !== "base" ? saved : { ...saved, model: "tiny" as const };
      setSettings(displayedSettings);
      setSavedSettings(saved);
      savedSettingsRef.current = saved;
      settingsReady.current = true;
      setActiveShortcut(saved.shortcut);
      setMicrophones(devices);
      setHistory(entries);
      setBaseInstalled(installed);
    }).catch((error) => setMessage(String(error)));
  }, []);

  const stopAndProcess = useCallback(async () => {
    if (processing.current) return;
    processing.current = true;
    setCommandPending(true);
    statusRef.current = "transcribing";
    try {
      const result = await invoke<DictationResult>("stop_and_process");
      setLatestResult(result);
      if (result.warning) setMessage(result.warning);
      await refreshHistory();
    } catch (error) {
      setMessage(String(error));
    } finally {
      processing.current = false;
      setCommandPending(false);
    }
  }, [refreshHistory]);

  const startRecording = useCallback(async () => {
    if (!settingsReady.current || processing.current || ["listening", "transcribing", "cleaning"].includes(statusRef.current)) return;
    processing.current = true;
    setCommandPending(true);
    try {
      await invoke("start_recording");
      statusRef.current = "listening";
    } catch (error) {
      setMessage(String(error));
    } finally {
      processing.current = false;
      setCommandPending(false);
    }
  }, []);

  useEffect(() => { startRecordingRef.current = startRecording; }, [startRecording]);
  useEffect(() => { stopAndProcessRef.current = stopAndProcess; }, [stopAndProcess]);

  useEffect(() => {
    let disposed = false;
    const controller = new DictationShortcut({
      mode: () => savedSettingsRef.current.activationBehavior,
      listening: () => statusRef.current === "listening",
      busy: () => !settingsReady.current || processing.current,
      start: () => startRecordingRef.current(),
      stop: () => stopAndProcessRef.current(),
      error: (error) => setMessage(String(error))
    });
    let unlisten: (() => void) | undefined;
    listen<{ state: "Pressed" | "Released" }>("dictation-shortcut", (event) => {
      if (!disposed) controller.handle(event.payload.state);
    }).then((stop) => { if (disposed) stop(); else unlisten = stop; })
      .catch((error) => setMessage(`Shortcut listener unavailable: ${String(error)}`));
    return () => {
      disposed = true;
      controller.dispose();
      unlisten?.();
    };
  }, []);

  const saveSettings = async () => {
    if (processing.current || ["listening", "transcribing", "cleaning"].includes(statusRef.current)) return;
    const shortcut = settings.shortcut.trim();
    if (!shortcut) {
      setMessage("Enter a shortcut such as Ctrl+Alt+Space");
      return;
    }
    const savedSettings = { ...settings, shortcut };
    setSavingSettings(true);
    try {
      await invoke("save_settings", { settings: savedSettings });
      setSettings(savedSettings);
      setSavedSettings(savedSettings);
      savedSettingsRef.current = savedSettings;
      setActiveShortcut(shortcut);
      setMessage("Settings saved");
      window.setTimeout(() => setMessage(""), 1800);
    } catch (error) {
      setMessage(`Settings could not be saved: ${String(error)}`);
    } finally {
      setSavingSettings(false);
    }
  };

  const testProvider = async () => {
    const generation = ++providerTestGeneration.current;
    setConnectionState("testing");
    try {
      const discovered = await invoke<string[]>("test_provider", { settings });
      if (generation !== providerTestGeneration.current) return;
      setModels(discovered);
      setConnectionState("ok");
      if (!settings.providerModel && discovered.length) {
        setSettings((current) => ({ ...current, providerModel: discovered[0] }));
      }
      setMessage("Server reachable. Select a model, then use Test cleanup to verify a response.");
    } catch (error) {
      if (generation !== providerTestGeneration.current) return;
      setConnectionState("error");
      setMessage(`Cleanup server check failed: ${String(error)}`);
    }
  };

  useEffect(() => {
    providerTestGeneration.current += 1;
    setConnectionState("idle");
    setCleanupTestState("idle");
    setModels([]);
  }, [settings.provider, settings.providerBaseUrl]);

  useEffect(() => {
    providerTestGeneration.current += 1;
    setCleanupTestState("idle");
  }, [settings.providerModel, settings.cleanupPrompt]);

  const testCleanup = async () => {
    const generation = ++providerTestGeneration.current;
    setCleanupTestState("testing");
    setMessage("Testing cleanup with a short sample sentence…");
    try {
      await invoke("test_cleanup", { settings });
      if (generation !== providerTestGeneration.current) return;
      setCleanupTestState("ok");
      setMessage("Cleanup test passed. Save settings to use this configuration for dictation.");
    } catch (error) {
      if (generation !== providerTestGeneration.current) return;
      setCleanupTestState("error");
      setMessage(`Cleanup test failed: ${String(error)}`);
    }
  };

  const testTranscriptionProvider = async () => {
    setTranscriptionConnectionState("testing");
    try {
      await invoke("test_transcription_provider", { settings });
      setTranscriptionConnectionState("ok");
      setMessage("Transcription server connected");
    } catch (error) {
      setTranscriptionConnectionState("error");
      setMessage(`Transcription connection failed: ${String(error)}`);
    }
  };

  const installBaseModel = async () => {
    setInstallingBase(true);
    setMessage("Downloading and verifying the multilingual Base model…");
    try {
      await invoke("install_base_model");
      setBaseInstalled(true);
      setMessage("Base model installed and verified");
    } catch (error) {
      setMessage(`Base model installation failed: ${String(error)}`);
    } finally {
      setInstallingBase(false);
    }
  };

  const refreshMicrophones = async () => {
    setRefreshingMicrophones(true);
    try {
      setMicrophones(await invoke<string[]>("list_microphones"));
      setMessage("Microphone list refreshed");
    } catch (error) {
      setMessage(`Microphones could not be refreshed: ${String(error)}`);
    } finally {
      setRefreshingMicrophones(false);
    }
  };

  const visibleHistory = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return needle ? history.filter((entry) => entry.finalText.toLowerCase().includes(needle)) : history;
  }, [history, query]);

  const settingsHaveChanges = useMemo(
    () => savedSettings !== null && JSON.stringify(settings) !== JSON.stringify(savedSettings),
    [savedSettings, settings]
  );
  const dictationActive = commandPending || status === "listening" || status === "transcribing" || status === "cleaning";
  const activeSettings = savedSettings ?? DEFAULT_SETTINGS;
  const activeModeLabel = savedSettings === null ? "Loading settings…" : activeSettings.transcriptionMode === "local" ? "Local transcription" : activeSettings.transcriptionMode === "remote" ? "Remote transcription" : "Remote with local fallback";

  return (
    <div className="app-shell">
      <aside>
        <div className="brand"><span className="brand-mark"><Mic size={19} /></span><div><strong>VoiceDictate</strong><small>Private voice typing · v{appVersion}</small></div></div>
        <nav>
          <button className={page === "home" ? "active" : ""} onClick={() => setPage("home")}><Mic size={18} /> Dictate</button>
          <button className={page === "history" ? "active" : ""} onClick={() => setPage("history")}><History size={18} /> History</button>
          <button className={page === "settings" ? "active" : ""} onClick={() => setPage("settings")}><SettingsIcon size={18} /> Settings</button>
          <button className={page === "about" ? "active" : ""} onClick={() => setPage("about")}><CircleHelp size={18} /> Help &amp; About</button>
        </nav>
        <div className="privacy-note"><Database size={16} /><span><strong>{activeModeLabel}</strong><br />Audio is deleted after transcription.</span></div>
      </aside>

      <main>
        {page === "home" && (
          <section className="home-page">
            <button type="button" className={`orb ${status}`} disabled={commandPending || savedSettings === null || status === "transcribing" || status === "cleaning"} aria-label={status === "listening" ? "Finish dictation" : "Start dictation"} onClick={() => status === "listening" ? stopAndProcess() : startRecording()}>
              <Mic size={38} />
              {status === "listening" && <span className="pulse" />}
            </button>
            <h1>{statusText}</h1>
            <p className="muted">{savedSettings?.activationBehavior === "hold" ? <>Hold <kbd>{displayShortcut(activeShortcut)}</kbd> while speaking, then release.</> : <>Tap <kbd>{displayShortcut(activeShortcut)}</kbd> to start; tap again to stop.</>}</p>
            <div className="status-grid">
              <article><span>Transcription</span><strong>{activeSettings.transcriptionMode === "local" ? `Local Whisper ${activeSettings.model === "tiny" ? "Tiny" : "Base"}` : activeSettings.transcriptionMode === "remote" ? "Remote server" : "Automatic"}</strong><small>{activeSettings.transcriptionMode === "local" ? "Audio stays on this PC" : activeSettings.transcriptionMode === "remote" ? "Uses your configured server" : "Remote with local fallback"}</small></article>
              <article><span>AI cleanup</span><strong>{activeSettings.cleanupEnabled ? activeSettings.provider : "Off"}</strong><small>{activeSettings.cleanupEnabled ? activeSettings.providerModel || "Choose a model" : "Raw transcript"}</small></article>
              <article><span>History</span><strong>{history.length} entries</strong><small>Text only</small></article>
            </div>
            {history[0] && <div className="latest"><div><Clock3 size={16} /> Latest dictation</div><p>{history[0].finalText}</p><div className="result-meta">{sourceLabel(history[0].transcriptionSource)}{history[0].processingMs > 0 && <> · {seconds(history[0].processingMs)} sec processing</>}</div>{latestResult && latestResult.finalText === history[0].finalText && <details className="timing-details"><summary>Processing details</summary><p>Finish transcription: {seconds(latestResult.timings.finishMs)} sec · Cleanup: {seconds(latestResult.timings.cleanupMs)} sec · Paste: {seconds(latestResult.timings.pasteMs)} sec{latestResult.timings.cpuThreads > 0 && <> · {latestResult.timings.cpuThreads} CPU threads</>}{latestResult.timings.incrementalChunks > 0 && <> · {latestResult.timings.incrementalChunks} chunks processed while speaking</>}</p></details>}<button onClick={() => navigator.clipboard.writeText(history[0].finalText)}><Copy size={15} /> Copy</button></div>}
          </section>
        )}

        {page === "history" && (
          <section>
            <header className="page-header"><div><h1>History</h1><p>Your text stays on this PC.</p></div><button className="danger" onClick={async () => { await invoke("clear_history"); await refreshHistory(); }}><Trash2 size={16} /> Clear all</button></header>
            <label className="search"><Search size={17} /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search dictations" /></label>
            <div className="history-list">
              {visibleHistory.map((entry) => <article key={entry.id}><div className="history-meta"><span>{new Date(entry.createdAt).toLocaleString()}</span><span>{Math.round(entry.durationMs / 1000)} sec recording · {entry.processingMode}</span></div><p>{entry.finalText}</p><div className="result-meta">{sourceLabel(entry.transcriptionSource)}{entry.processingMs > 0 && <> · {seconds(entry.processingMs)} sec processing</>}</div><div className="row-actions"><button onClick={() => navigator.clipboard.writeText(entry.finalText)}><Copy size={15} /> Copy</button><button onClick={async () => { await invoke("delete_history", { id: entry.id }); await refreshHistory(); }}><Trash2 size={15} /> Delete</button></div></article>)}
              {!visibleHistory.length && <div className="empty">No dictations found.</div>}
            </div>
          </section>
        )}

        {page === "settings" && (
          <section>
            <header className="page-header"><div><h1>Settings</h1><p>Choose where speech is transcribed and optionally clean the resulting text.</p></div><button className={`settings-save ${settingsHaveChanges ? "unsaved" : "saved"}`} onClick={saveSettings} disabled={savingSettings || savedSettings === null || dictationActive}><Check size={16} /> {savingSettings ? "Saving…" : settingsHaveChanges ? "Unsaved changes — Save" : "Saved"}</button></header>
            <fieldset className="settings-grid" disabled={dictationActive || savingSettings}>
              <div className="card"><h2>Dictation</h2><ShortcutInput value={settings.shortcut} onChange={(shortcut) => setSettings((current) => ({ ...current, shortcut }))} disabled={dictationActive || savingSettings || savedSettings === null} /><label>Activation behavior<select value={settings.activationBehavior} onChange={(event) => setSettings({ ...settings, activationBehavior: event.target.value as AppSettings["activationBehavior"] })}><option value="toggle">Tap to start/stop</option><option value="hold">Hold to talk</option></select></label><label>Microphone<select value={settings.microphoneName ?? ""} onChange={(event) => setSettings({ ...settings, microphoneName: event.target.value || null })}><option value="">System default</option>{microphones.map((name) => <option key={name}>{name}</option>)}</select></label><button className="secondary microphone-refresh" disabled={refreshingMicrophones} onClick={refreshMicrophones}><RotateCcw size={15} className={refreshingMicrophones ? "spin" : ""} /> {refreshingMicrophones ? "Refreshing microphones…" : "Refresh microphones"}</button><div className="two-col"><label>Language<select value={settings.language} onChange={(event) => setSettings({ ...settings, language: event.target.value })}><option value="auto">Auto detect</option><option value="en">English</option><option value="es">Spanish</option><option value="fr">French</option><option value="de">German</option></select></label><label>{settings.transcriptionMode === "automatic" ? "Local fallback model" : "Local speech model"}<select value={settings.model} onChange={(event) => setSettings({ ...settings, model: event.target.value as "tiny" | "base" })}><option value="tiny">Tiny (included)</option><option value="base" disabled={!baseInstalled}>Base {baseInstalled ? "(installed)" : "(download first)"}</option></select></label></div>{!baseInstalled && settings.transcriptionMode !== "remote" && <button className="secondary" disabled={installingBase} onClick={installBaseModel}><Sparkles size={16} /> {installingBase ? "Installing Base model…" : "Install optional Base model"}</button>}</div>
              <div className="card">
                <h2>Transcription location</h2>
                <label>Method<select value={settings.transcriptionMode} onChange={(event) => setSettings({ ...settings, transcriptionMode: event.target.value as AppSettings["transcriptionMode"] })}><option value="local">Local — free and private</option><option value="remote">My server or online provider</option><option value="automatic">Automatic — remote, then local fallback</option></select></label>
                {settings.transcriptionMode === "local" ? <p className="privacy-warning safe"><strong>Audio stays on this PC.</strong> No transcription network request is made.</p> : <>
                  <label>Server type<select value={settings.transcriptionProvider} onChange={(event) => setSettings({ ...settings, transcriptionProvider: event.target.value as AppSettings["transcriptionProvider"] })}><option value="whisper-cpp">whisper.cpp server</option><option value="openai-compatible">OpenAI-compatible audio API</option></select></label>
                  <label>Server URL<input placeholder={settings.transcriptionProvider === "whisper-cpp" ? "http://100.x.x.x:8080" : "https://api.example.com/v1"} value={settings.transcriptionBaseUrl} onChange={(event) => setSettings({ ...settings, transcriptionBaseUrl: event.target.value })} /></label>
                  {settings.transcriptionProvider === "openai-compatible" && <label>Transcription model<input placeholder="whisper-large-v3-turbo or whisper-1" value={settings.transcriptionModel} onChange={(event) => setSettings({ ...settings, transcriptionModel: event.target.value })} /></label>}
                  <label>API key<input type="password" placeholder={settings.transcriptionApiKeyConfigured ? "Saved securely" : "Optional for private servers"} onChange={(event) => invoke("set_transcription_api_key", { apiKey: event.target.value })} /></label>
                  <label>Timeout (seconds)<input type="number" min={5} max={600} value={settings.transcriptionTimeoutSeconds} onChange={(event) => setSettings({ ...settings, transcriptionTimeoutSeconds: Number(event.target.value) || 90 })} /></label>
                  <button className="secondary" onClick={testTranscriptionProvider}>{transcriptionConnectionState === "testing" ? <RotateCcw className="spin" size={16} /> : transcriptionConnectionState === "ok" ? <Wifi size={16} /> : transcriptionConnectionState === "error" ? <WifiOff size={16} /> : <Sparkles size={16} />} {transcriptionConnectionState === "ok" ? "Connected" : transcriptionConnectionState === "error" ? "Connection failed" : "Test transcription server"}</button>
                  <button className="secondary" onClick={() => invoke("open_help_document", { document: "remote-transcription" }).catch((error) => setMessage(String(error)))}><BookOpen size={16} /> Open remote Whisper setup guide</button>
                  <p className="privacy-warning"><strong>Microphone audio will be sent to this server.</strong> VoiceDictate deletes its temporary copy after transcription. Use HTTPS for public providers.</p>
                  {settings.transcriptionMode === "automatic" && <p className="privacy-warning safe">If the server cannot be reached, bundled local Whisper will be used automatically.</p>}
                </>}
              </div>
              <div className="card"><div className="toggle-row"><div><h2>AI cleanup</h2><p>Only transcript text is sent.</p></div><button className={`switch ${settings.cleanupEnabled ? "on" : ""}`} onClick={() => setSettings({ ...settings, cleanupEnabled: !settings.cleanupEnabled })}><span /></button></div>{settings.cleanupEnabled && <><label>Provider<select value={settings.provider} onChange={(event) => setSettings({ ...settings, provider: event.target.value as AppSettings["provider"] })}><option value="none">None</option><option value="ollama">Ollama</option><option value="openai-compatible">OpenAI compatible</option></select></label><label>Base URL<input value={settings.providerBaseUrl} onChange={(event) => setSettings({ ...settings, providerBaseUrl: event.target.value })} /></label><label>Model<input list="provider-models" value={settings.providerModel} onChange={(event) => setSettings({ ...settings, providerModel: event.target.value })} /><datalist id="provider-models">{models.map((model) => <option key={model} value={model} />)}</datalist></label>{settings.provider === "openai-compatible" && <label>API key<input type="password" placeholder={settings.providerApiKeyConfigured ? "Saved securely" : "Optional for private servers"} onChange={(event) => invoke("set_provider_api_key", { apiKey: event.target.value })} /></label>}<button className="secondary" disabled={connectionState === "testing" || cleanupTestState === "testing" || settings.provider === "none"} onClick={testProvider}>{connectionState === "testing" ? <RotateCcw className="spin" size={16} /> : connectionState === "ok" ? <Wifi size={16} /> : connectionState === "error" ? <WifiOff size={16} /> : <Sparkles size={16} />} {connectionState === "ok" ? "Server reachable" : connectionState === "error" ? "Connection failed" : "Test connection"}</button><button className="secondary" disabled={cleanupTestState === "testing" || connectionState === "testing" || settings.provider === "none" || !settings.providerModel.trim()} onClick={testCleanup}>{cleanupTestState === "testing" ? "Testing cleanup…" : cleanupTestState === "ok" ? "Cleanup test passed" : "Test cleanup"}</button><p className="muted">Test cleanup sends a sample sentence to the selected model. Save settings before dictating.</p><label>Cleanup instructions<textarea rows={5} value={settings.cleanupPrompt} onChange={(event) => setSettings({ ...settings, cleanupPrompt: event.target.value })} /></label></>}</div>
            </fieldset>
          </section>
        )}

        {page === "about" && (
          <section>
            <header className="page-header"><div><h1>Help &amp; About</h1><p>VoiceDictate version {appVersion}</p></div></header>
            <div className="about-hero"><span className="brand-mark"><Mic size={24} /></span><div><h2>VoiceDictate {appVersion}</h2><p>Free, local-first Windows dictation with optional private or hosted transcription.</p></div></div>
            <h2 className="section-title">Choose how transcription runs</h2>
            <div className="about-grid">
              <article><strong>Local Whisper</strong><p>Included and ready immediately. Audio stays on this Windows PC. No server, account, API key, internet connection, or usage fee is required.</p></article>
              <article><strong>Your own server</strong><p>Run whisper.cpp on another Windows PC, K11, Linux VM, home server, or NAS. Connect over a trusted LAN or private Tailscale network.</p></article>
              <article><strong>Online provider</strong><p>Use a provider supporting the OpenAI-compatible audio transcription API. The user supplies the HTTPS URL, model, API key, and pays any provider charges.</p></article>
            </div>
            <div className="card help-card">
              <h2>Instructions and review material</h2>
              <p>These documents are bundled with this installation and can be opened without visiting GitHub.</p>
              <div className="guide-actions">
                <button className="secondary" onClick={() => invoke("open_help_document", { document: "remote-transcription" }).catch((error) => setMessage(String(error)))}><BookOpen size={16} /> Remote Whisper setup</button>
                <button className="secondary" onClick={() => invoke("open_help_document", { document: "video-guide" }).catch((error) => setMessage(String(error)))}><BookOpen size={16} /> Installation video checklist</button>
                <button className="secondary" onClick={() => invoke("open_help_document", { document: "building" }).catch((error) => setMessage(String(error)))}><BookOpen size={16} /> Build instructions</button>
              </div>
              <p className="privacy-warning"><strong>Remote and online modes send microphone audio.</strong> Local mode does not. Never expose an unprotected Whisper or Ollama port directly to the public internet.</p>
            </div>
          </section>
        )}
        {message && <div className="toast" role="status">{message}</div>}
      </main>
    </div>
  );
}

export default App;
