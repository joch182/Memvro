mod audio;
mod system_audio;
mod transcription;

use audio::RecordingController;
use chrono::Local;
use cpal::traits::HostTrait;
use reqwest::blocking::Client;
use serde::Serialize;
use std::io::Write;
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

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelOption {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    filename: &'static str,
    size: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelStatus {
    selected_id: String,
    options: Vec<ModelOption>,
    downloaded: bool,
}

#[derive(serde::Deserialize, Serialize)]
struct ModelSettings {
    selected_id: String,
}

const MODEL_OPTIONS: &[ModelOption] = &[
    ModelOption {
        id: "large-v3-turbo",
        name: "Whisper Large v3 Turbo",
        description: "Best balance of local accuracy and speed; multilingual, with about 809 MB of storage.",
        filename: "ggml-large-v3-turbo.bin",
        size: "809 MB",
    },
    ModelOption {
        id: "large-v3",
        name: "Whisper Large v3",
        description: "Highest local accuracy; multilingual, but requires about 3.1 GB of storage and more memory.",
        filename: "ggml-large-v3.bin",
        size: "3.1 GB",
    },
    ModelOption {
        id: "base-multilingual",
        name: "Whisper Base Multilingual",
        description: "Automatic language detection, including English and Spanish.",
        filename: "ggml-base.bin",
        size: "142 MB",
    },
    ModelOption {
        id: "base-english",
        name: "Whisper Base English",
        description: "English-only transcription with a slightly smaller model.",
        filename: "ggml-base.en.bin",
        size: "142 MB",
    },
];

fn model_option(id: &str) -> Result<&'static ModelOption, String> {
    MODEL_OPTIONS
        .iter()
        .find(|model| model.id == id)
        .ok_or_else(|| format!("Unknown transcription model: {id}"))
}

fn model_download_url(model: &ModelOption) -> String {
    format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}?download=true",
        model.filename
    )
}

fn models_directory(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join("models"))
        .map_err(|error| format!("Unable to locate model directory: {error}"))
}

fn model_settings_file(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|path| path.join("model.json"))
        .map_err(|error| format!("Unable to locate model settings: {error}"))
}

fn selected_model_id(app: &tauri::AppHandle) -> String {
    model_settings_file(app)
        .ok()
        .and_then(|file| std::fs::read_to_string(file).ok())
        .and_then(|contents| serde_json::from_str::<ModelSettings>(&contents).ok())
        .map(|settings| settings.selected_id)
        .filter(|id| model_option(id).is_ok())
        .unwrap_or_else(|| "base-multilingual".to_string())
}

fn model_path(app: &tauri::AppHandle, model: &ModelOption) -> Result<std::path::PathBuf, String> {
    Ok(models_directory(app)?.join(model.filename))
}

fn model_is_ready(path: &std::path::Path) -> bool {
    path.metadata()
        .map(|metadata| metadata.is_file() && metadata.len() > 1_000_000)
        .unwrap_or(false)
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
fn audio_level(state: tauri::State<'_, AppState>) -> f32 {
    state.audio.level()
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
fn get_model_status(app: tauri::AppHandle) -> Result<ModelStatus, String> {
    let selected_id = selected_model_id(&app);
    let selected = model_option(&selected_id)?;
    let downloaded = model_is_ready(&model_path(&app, selected)?);
    Ok(ModelStatus {
        selected_id,
        options: MODEL_OPTIONS.to_vec(),
        downloaded,
    })
}

#[tauri::command]
fn set_model(app: tauri::AppHandle, model_id: String) -> Result<ModelStatus, String> {
    model_option(&model_id)?;
    let file = model_settings_file(&app)?;
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Unable to create model settings directory: {error}"))?;
    }
    std::fs::write(
        &file,
        serde_json::to_vec_pretty(&ModelSettings {
            selected_id: model_id,
        })
        .map_err(|error| format!("Unable to encode model settings: {error}"))?,
    )
    .map_err(|error| format!("Unable to save model settings: {error}"))?;
    get_model_status(app)
}

#[tauri::command]
fn download_model(app: tauri::AppHandle, model_id: String) -> Result<ModelStatus, String> {
    let model = model_option(&model_id)?;
    let directory = models_directory(&app)?;
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("Unable to create model directory: {error}"))?;
    let path = model_path(&app, model)?;
    if !model_is_ready(&path) {
        let temporary_path = path.with_extension("download");
        let client = Client::builder()
            .build()
            .map_err(|error| format!("Unable to prepare model download: {error}"))?;
        let mut response = client
            .get(model_download_url(model))
            .send()
            .and_then(|response| response.error_for_status())
            .map_err(|error| format!("Unable to download model: {error}"))?;
        let mut file = std::fs::File::create(&temporary_path)
            .map_err(|error| format!("Unable to create model download: {error}"))?;
        if let Err(error) = std::io::copy(&mut response, &mut file)
            .and_then(|_| file.flush())
            .map_err(|error| format!("Unable to save model download: {error}"))
        {
            let _ = std::fs::remove_file(&temporary_path);
            return Err(error);
        }
        std::fs::rename(&temporary_path, &path)
            .map_err(|error| format!("Unable to install downloaded model: {error}"))?;
    }
    if !model_is_ready(&path) {
        return Err("Downloaded model is incomplete or invalid".to_string());
    }
    set_model(app, model_id)
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
            let selected = model_option(&selected_model_id(&app))
                .expect("selected model should always be valid");
            model_path(&app, selected).unwrap_or_else(|_| std::path::PathBuf::from("models"))
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
            audio_level,
            start_recording,
            stop_recording,
            transcribe_recording,
            get_storage_settings,
            set_storage_settings,
            get_model_status,
            set_model,
            download_model
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
