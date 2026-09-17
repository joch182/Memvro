import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import "./App.css";

function App() {
  const [isRecording, setIsRecording] = useState(false);
  const [elapsedSeconds, setElapsedSeconds] = useState(0);
  const [errorMessage, setErrorMessage] = useState("");
  const [recordingPath, setRecordingPath] = useState("");
  const [transcriptPath, setTranscriptPath] = useState("");
  const [isTranscribing, setIsTranscribing] = useState(false);
  const [storagePath, setStoragePath] = useState("");
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [isSavingSettings, setIsSavingSettings] = useState(false);
  const [systemAudioReady, setSystemAudioReady] = useState(false);

  useEffect(() => {
    if (!isRecording) return;

    const timer = window.setInterval(() => {
      setElapsedSeconds((current) => current + 1);
    }, 1000);

    return () => window.clearInterval(timer);
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
            <div className="model-chip"><span className="chip-dot" /> English model <strong>Base</strong></div>
          </div>

          <div className={`recording-panel ${isRecording ? "is-recording" : ""}`}>
            <div className="panel-topline"><span>{isRecording ? "Recording in progress" : "New recording"}</span><span>{formattedTime}</span></div>
            <div className="orbital-control">
              <div className="waveform" aria-hidden="true"><i /><i /><i /><i /><i /><i /><i /><i /><i /></div>
              <button className="record-button" type="button" onClick={toggleRecording} aria-label={isRecording ? "Stop recording" : "Start recording"}>
                <span className={isRecording ? "stop-icon" : "mic-icon"}>{isRecording ? "" : "●"}</span>
              </button>
              <p>{isRecording ? "Click to stop" : "Click to start"}</p>
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
                <h2>Storage location</h2>
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
            <article className="info-block"><span className="info-number">02</span><div><h2>Transcribe</h2><p>When you stop, Memvro processes the recording locally with its built-in English model.</p></div></article>
            <article className="info-block"><span className="info-number">03</span><div><h2>Export</h2><p>Save a clean, UTF-8 text file to the folder you choose. Temporary audio is removed.</p></div></article>
          </div>
        </section>
      </div>
    </main>
  );
}

export default App;
