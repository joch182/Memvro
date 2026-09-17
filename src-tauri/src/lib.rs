mod audio;
mod system_audio;
mod transcription;

use audio::RecordingController;
use chrono::Local;
use cpal::traits::HostTrait;
use serde::Serialize;
use std::sync::Mutex;
use tauri::Manager;

struct AppState {
    recording: Mutex<bool>,
    audio: RecordingController,
}

#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaptureStatus {
    system_audio: bool,
    microphone: bool,
    recording: bool,
}

#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct StorageSettings {
    storage_path: String,
}

fn settings_file(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|path| path.join("settings.json"))
        .map_err(|error| format!("Unable to locate settings directory: {error}"))
}

fn default_storage_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join("Recordings"))
        .map_err(|error| format!("Unable to locate default recording directory: {error}"))
}

fn load_storage_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let file = settings_file(app)?;
    if let Ok(contents) = std::fs::read_to_string(file) {
        if let Ok(settings) = serde_json::from_str::<StorageSettings>(&contents) {
            return Ok(std::path::PathBuf::from(settings.storage_path));
        }
    }
    default_storage_path(app)
}

#[tauri::command]
fn capture_status(state: tauri::State<'_, AppState>) -> CaptureStatus {
    CaptureStatus {
        system_audio: cfg!(any(target_os = "macos", target_os = "windows")),
        microphone: cpal::default_host().default_input_device().is_some(),
        recording: state
            .recording
            .lock()
            .map(|recording| *recording)
            .unwrap_or(false),
    }
}

#[tauri::command]
fn start_recording(app: tauri::AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let mut recording = state
        .recording
        .lock()
        .map_err(|_| "Unable to access recording state".to_string())?;
    if *recording {
        return Err("A recording is already in progress".to_string());
    }
    let session_dir =
        load_storage_path(&app)?.join(Local::now().format("%Y-%m-%d_%H-%M-%S").to_string());
    std::fs::create_dir_all(&session_dir)
        .map_err(|error| format!("Unable to create local recording directory: {error}"))?;
    let path = session_dir.join("recording.wav");
    state.audio.start(path)?;
    *recording = true;
    Ok(())
}

#[tauri::command]
fn get_storage_settings(app: tauri::AppHandle) -> Result<StorageSettings, String> {
    Ok(StorageSettings {
        storage_path: load_storage_path(&app)?.display().to_string(),
    })
}

#[tauri::command]
fn set_storage_settings(
    app: tauri::AppHandle,
    storage_path: String,
) -> Result<StorageSettings, String> {
    let path = std::path::PathBuf::from(storage_path.trim());
    if path.as_os_str().is_empty() {
        return Err("Choose a folder for recordings and transcripts".to_string());
    }
    std::fs::create_dir_all(&path)
        .map_err(|error| format!("Unable to create storage directory: {error}"))?;
    let settings = StorageSettings {
        storage_path: path.display().to_string(),
    };
    let file = settings_file(&app)?;
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Unable to create settings directory: {error}"))?;
    }
    std::fs::write(
        &file,
        serde_json::to_vec_pretty(&settings)
            .map_err(|error| format!("Unable to encode settings: {error}"))?,
    )
    .map_err(|error| format!("Unable to save settings: {error}"))?;
    Ok(settings)
}

#[tauri::command]
fn stop_recording(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let mut recording = state
        .recording
        .lock()
        .map_err(|_| "Unable to access recording state".to_string())?;
    if !*recording {
        return Err("No recording is in progress".to_string());
    }
    let path = state.audio.stop()?;
    *recording = false;
    Ok(path.display().to_string())
}

#[tauri::command]
fn transcribe_recording(app: tauri::AppHandle, recording_path: String) -> Result<String, String> {
    let model_path = std::env::var_os("MEMVRO_WHISPER_MODEL_PATH")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let resource_path = app
                .path()
                .resource_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."))
                .join("models")
                .join("ggml-base.en.bin");
            if resource_path.is_file() {
                resource_path
            } else {
                std::path::PathBuf::from("src-tauri/models/ggml-base.en.bin")
            }
        });
    let recording = std::path::Path::new(&recording_path);
    let system_audio = recording.with_file_name("system_audio.wav");
    let transcript_path =
        transcription::transcribe_wav(recording, Some(&system_audio), &model_path)?;
    Ok(transcript_path.display().to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            recording: Mutex::new(false),
            audio: RecordingController::new(),
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            capture_status,
            start_recording,
            stop_recording,
            transcribe_recording,
            get_storage_settings,
            set_storage_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
