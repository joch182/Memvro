import { useEffect, useState, type CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import "./App.css";

type ModelOption = {
  id: string;
  name: string;
  description: string;
  filename: string;
  size: string;
};

type ModelStatus = {
  selectedId: string;
  options: ModelOption[];
  downloaded: boolean;
};

function App() {
  const [isRecording, setIsRecording] = useState(false);
  const [audioLevel, setAudioLevel] = useState(0);
  const [elapsedSeconds, setElapsedSeconds] = useState(0);
  const [errorMessage, setErrorMessage] = useState("");
  const [recordingPath, setRecordingPath] = useState("");
  const [transcriptPath, setTranscriptPath] = useState("");
  const [isTranscribing, setIsTranscribing] = useState(false);
  const [storagePath, setStoragePath] = useState("");
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [isSavingSettings, setIsSavingSettings] = useState(false);
  const [systemAudioReady, setSystemAudioReady] = useState(false);
  const [modelOptions, setModelOptions] = useState<ModelOption[]>([]);
  const [selectedModelId, setSelectedModelId] = useState("");
  const [modelDownloaded, setModelDownloaded] = useState(false);
  const [isDownloadingModel, setIsDownloadingModel] = useState(false);
  const selectedModel = modelOptions.find((model) => model.id === selectedModelId);
  const modelReady = Boolean(selectedModel && modelDownloaded);

  useEffect(() => {
    if (!isRecording) return;

    const timer = window.setInterval(() => {
      setElapsedSeconds((current) => current + 1);
    }, 1000);

    return () => window.clearInterval(timer);
  }, [isRecording]);

  useEffect(() => {
    if (!isRecording) {
      setAudioLevel(0);
      return;
    }

    const meter = window.setInterval(() => {
      invoke<number>("audio_level")
        .then((level) => setAudioLevel(Math.min(1, Math.max(0, level * 2.2))))
        .catch(() => setAudioLevel(0));
    }, 100);

    return () => window.clearInterval(meter);
  }, [isRecording]);

  useEffect(() => {
    invoke<{ storagePath: string }>("get_storage_settings")
      .then((settings) => setStoragePath(settings.storagePath))
      .catch((error) => setErrorMessage(String(error)));
  }, []);

  useEffect(() => {
    invoke<{ systemAudio: boolean }>("capture_status")
      .then((status) => setSystemAudioReady(status.systemAudio))
      .catch((error) => setErrorMessage(String(error)));
  }, []);

  useEffect(() => {
    invoke<ModelStatus>("get_model_status")
      .then((status) => {
        setModelOptions(status.options);
        setSelectedModelId(status.selectedId);
        setModelDownloaded(status.downloaded);
      })
      .catch((error) => setErrorMessage(String(error)));
  }, []);

  const formattedTime = new Date(elapsedSeconds * 1000)
    .toISOString()
    .slice(11, 19);

  async function toggleRecording() {
    if (isRecording) {
      try {
        const path = await invoke<string>("stop_recording");
        setRecordingPath(path);
        setTranscriptPath("");
        setIsRecording(false);
      } catch (error) {
        setErrorMessage(String(error));
      }
      return;
    }

    setErrorMessage("");
    setRecordingPath("");
    setTranscriptPath("");
    try {
      await invoke("start_recording");
      setElapsedSeconds(0);
      setIsRecording(true);
    } catch (error) {
      setErrorMessage(String(error));
    }
  }

  async function transcribeRecording() {
    setErrorMessage("");
    setIsTranscribing(true);
    try {
      const path = await invoke<string>("transcribe_recording", { recordingPath });
      setTranscriptPath(path);
    } catch (error) {
      setErrorMessage(String(error));
    } finally {
      setIsTranscribing(false);
    }
  }

  async function chooseStorageFolder() {
    const selected = await open({ directory: true, multiple: false, title: "Choose Memvro storage folder" });
    if (typeof selected === "string") setStoragePath(selected);
  }

  async function saveStorageSettings() {
    setIsSavingSettings(true);
    setErrorMessage("");
    try {
      const settings = await invoke<{ storagePath: string }>("set_storage_settings", { storagePath });
      setStoragePath(settings.storagePath);
      setIsSettingsOpen(false);
    } catch (error) {
      setErrorMessage(String(error));
    } finally {
      setIsSavingSettings(false);
    }
  }

  async function selectModel(modelId: string) {
    setErrorMessage("");
    try {
      const status = await invoke<ModelStatus>("set_model", { modelId });
      setSelectedModelId(status.selectedId);
      setModelDownloaded(status.downloaded);
    } catch (error) {
      setErrorMessage(String(error));
    }
  }

  async function downloadSelectedModel() {
    setErrorMessage("");
    setIsDownloadingModel(true);
    try {
      const status = await invoke<ModelStatus>("download_model", { modelId: selectedModelId });
      setSelectedModelId(status.selectedId);
      setModelDownloaded(status.downloaded);
    } catch (error) {
      setErrorMessage(String(error));
    } finally {
      setIsDownloadingModel(false);
    }
  }

  return (
    <main className="app-shell">
      <header className="topbar">
        <div className="brand-lockup">
          <span className="brand-mark">M</span>
          <span>Memvro</span>
        </div>
        <div className="privacy-badge"><span className="status-dot" /> Offline by design</div>
      </header>

      <div className="workspace">
        <aside className="sidebar">
          <div className="sidebar-label">Workspace</div>
          <button className="nav-item active" type="button"><span className="nav-icon">+</span> New recording</button>
          <button className="nav-item" type="button"><span className="nav-icon">□</span> Transcripts <span className="nav-count">0</span></button>

          <div className="sidebar-footer">
            <div className="sidebar-label">Capture status</div>
            <div className="status-card">
              <div className="status-row"><span className={`status-dot ${systemAudioReady ? "" : "unavailable"}`} /> System audio <strong className={systemAudioReady ? "" : "unavailable-text"}>{systemAudioReady ? "Ready" : "Next"}</strong></div>
              <div className="status-row"><span className="status-dot" /> Microphone <strong>Ready</strong></div>
            </div>
            <button className="settings-button" type="button" onClick={() => setIsSettingsOpen(true)} disabled={isRecording}><span className="nav-icon">*</span> Settings</button>
          </div>
        </aside>

        <section className="content">
          <div className="content-heading">
            <div>
              <p className="eyebrow">Local capture</p>
              <h1>Ready when you are.</h1>
              <p className="subtitle">Record your meeting audio, then turn it into a transcript on this device.</p>
            </div>
            <div className={`model-chip ${modelReady ? "model-chip-ready" : "model-chip-required"}`} title={selectedModel?.description || "Choose and download a transcription model in Settings."}>
              <span className={`chip-dot ${modelReady ? "" : "chip-dot-required"}`} />
              <span className="model-chip-copy">
                <strong>{modelReady ? selectedModel?.name : "Model required"}</strong>
                <small>{modelReady ? selectedModel?.description : "Choose and download in Settings"}</small>
              </span>
            </div>
          </div>

          <div className={`recording-panel ${isRecording ? "is-recording" : ""}`}>
            <div className="panel-topline"><span>{isRecording ? "Recording in progress" : "New recording"}</span><span>{formattedTime}</span></div>
            <div className="orbital-control">
              <div className="meter-halo" style={{ "--audio-level": audioLevel } as CSSProperties} aria-hidden="true" />
              <div className="waveform" aria-hidden="true"><i /><i /><i /><i /><i /><i /><i /><i /><i /></div>
              <button className="record-button" type="button" onClick={toggleRecording} aria-label={isRecording ? "Stop recording" : "Start recording"}>
                <span className={isRecording ? "stop-icon" : "mic-icon"}>{isRecording ? "" : "●"}</span>
              </button>
              <p>{isRecording ? `Listening · ${Math.round(audioLevel * 100)}% level` : "Click to start"}</p>
            </div>
            <div className="capture-sources">
              <span><span className="source-icon">◉</span> System audio</span>
              <span><span className="source-icon">◌</span> Microphone</span>
            </div>
          </div>
          {recordingPath && !isRecording && (
            <div className="recording-result">
              <span>Recording saved locally</span>
              <button type="button" onClick={transcribeRecording} disabled={isTranscribing}>
                {isTranscribing ? "Transcribing..." : "Transcribe locally"}
              </button>
            </div>
          )}
          {transcriptPath && <p className="success-message">Transcript saved to {transcriptPath}</p>}
          {errorMessage && <p className="error-message">{errorMessage}</p>}

          {isSettingsOpen && (
            <div className="settings-panel">
              <div>
                <p className="eyebrow">Settings</p>
                <h2>Models and storage</h2>
              </div>
              <div className="model-picker">
                <label htmlFor="model-select">Transcription model</label>
                <select id="model-select" value={selectedModelId} onChange={(event) => selectModel(event.target.value)} disabled={isDownloadingModel}>
                  {modelOptions.map((model) => <option key={model.id} value={model.id}>{model.name} ({model.size})</option>)}
                </select>
                <p className="settings-description">{modelOptions.find((model) => model.id === selectedModelId)?.description}</p>
                <button className="model-download-button" type="button" onClick={downloadSelectedModel} disabled={isDownloadingModel || !selectedModelId || modelDownloaded}>
                  {isDownloadingModel ? "Downloading model..." : modelDownloaded ? "Model downloaded" : "Download model"}
                </button>
              </div>
              <div className="storage-settings">
                <h3>Storage location</h3>
                <p className="settings-description">Each recording creates a timestamped folder here containing its audio and transcript.</p>
              </div>
              <div className="storage-picker">
                <input aria-label="Storage location" value={storagePath} readOnly />
                <button type="button" onClick={chooseStorageFolder}>Choose folder</button>
              </div>
              <div className="settings-actions">
                <button type="button" onClick={() => setIsSettingsOpen(false)}>Cancel</button>
                <button type="button" onClick={saveStorageSettings} disabled={isSavingSettings || !storagePath}>{isSavingSettings ? "Saving..." : "Save location"}</button>
              </div>
            </div>
          )}

          <div className="info-grid">
            <article className="info-block"><span className="info-number">01</span><div><h2>Capture</h2><p>Audio stays on your computer while you meet in Teams, Slack, Zoom, or any other app.</p></div></article>
            <article className="info-block"><span className="info-number">02</span><div><h2>Transcribe</h2><p>When you stop, Memvro detects English or Spanish and processes the recording locally.</p></div></article>
            <article className="info-block"><span className="info-number">03</span><div><h2>Export</h2><p>Save a clean, UTF-8 text file to the folder you choose. Temporary audio is removed.</p></div></article>
          </div>
        </section>
      </div>
    </main>
  );
}

export default App;
