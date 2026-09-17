#[cfg(target_os = "macos")]
use hound::{WavSpec, WavWriter};
#[cfg(target_os = "macos")]
use screencapturekit::prelude::*;
#[cfg(target_os = "macos")]
use std::fs::File;
#[cfg(target_os = "macos")]
use std::io::BufWriter;
#[cfg(target_os = "macos")]
use std::path::PathBuf;
#[cfg(target_os = "windows")]
use std::path::PathBuf;
#[cfg(target_os = "macos")]
use std::sync::{Arc, Mutex};

#[cfg(target_os = "macos")]
type SharedWriter = Arc<Mutex<Option<WavWriter<BufWriter<File>>>>>;

#[cfg(target_os = "macos")]
pub struct SystemAudioCapture {
    stream: SCStream,
    writer: SharedWriter,
    path: PathBuf,
}

#[cfg(target_os = "macos")]
impl SystemAudioCapture {
    pub fn start(path: PathBuf) -> Result<Self, String> {
        let content = SCShareableContent::get()
            .map_err(|error| format!("Unable to access macOS shareable content: {error}"))?;
        let display =
            content.displays().into_iter().next().ok_or_else(|| {
                "No macOS display is available for system audio capture".to_string()
            })?;
        let filter = SCContentFilter::create()
            .with_display(&display)
            .with_excluding_windows(&[])
            .build();
        let config = SCStreamConfiguration::new()
            .with_captures_audio(true)
            .with_sample_rate(16_000)
            .with_channel_count(1);
        let writer = Arc::new(Mutex::new(Some(
            WavWriter::create(
                &path,
                WavSpec {
                    channels: 1,
                    sample_rate: 16_000,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                },
            )
            .map_err(|error| format!("Unable to create system audio file: {error}"))?,
        )));
        let callback_writer = Arc::clone(&writer);
        let mut stream = SCStream::new(&filter, &config);
        stream.add_output_handler(
            move |sample: CMSampleBuffer, output_type: SCStreamOutputType| {
                if !matches!(output_type, SCStreamOutputType::Audio) {
                    return;
                }
                let Some(buffers) = sample.audio_buffer_list() else {
                    return;
                };
                let Ok(mut writer) = callback_writer.lock() else {
                    return;
                };
                let Some(writer) = writer.as_mut() else {
                    return;
                };
                for buffer in buffers.iter() {
                    for bytes in buffer.data().chunks_exact(4) {
                        let value = f32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                        let sample = (value.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                        let _ = writer.write_sample(sample);
                    }
                }
            },
            SCStreamOutputType::Audio,
        );
        stream
            .start_capture()
            .map_err(|error| format!("Unable to start macOS system audio capture: {error}"))?;
        Ok(Self {
            stream,
            writer,
            path,
        })
    }

    pub fn stop(self) -> Result<PathBuf, String> {
        self.stream
            .stop_capture()
            .map_err(|error| format!("Unable to stop macOS system audio capture: {error}"))?;
        let writer = Arc::try_unwrap(self.writer)
            .map_err(|_| {
                "Unable to finish system audio while audio is still being written".to_string()
            })?
            .into_inner()
            .map_err(|_| "Unable to lock system audio file".to_string())?
            .ok_or_else(|| "System audio file was already closed".to_string())?;
        writer
            .finalize()
            .map_err(|error| format!("Unable to finalize system audio file: {error}"))?;
        Ok(self.path)
    }
}

#[cfg(not(target_os = "macos"))]
#[cfg(not(target_os = "windows"))]
pub struct SystemAudioCapture;

#[cfg(not(target_os = "macos"))]
#[cfg(not(target_os = "windows"))]
impl SystemAudioCapture {
    pub fn start(_path: std::path::PathBuf) -> Result<Self, String> {
        Err("System audio capture is not implemented for this platform yet".to_string())
    }

    pub fn stop(self) -> Result<std::path::PathBuf, String> {
        Err("System audio capture is not implemented for this platform yet".to_string())
    }
}

#[cfg(target_os = "windows")]
use hound::{WavSpec, WavWriter};
#[cfg(target_os = "windows")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "windows")]
use std::sync::mpsc;
#[cfg(target_os = "windows")]
use std::sync::Arc;
#[cfg(target_os = "windows")]
use std::thread::{self, JoinHandle};
#[cfg(target_os = "windows")]
use wasapi::*;

#[cfg(target_os = "windows")]
pub struct SystemAudioCapture {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<PathBuf, String>>>,
    path: PathBuf,
}

#[cfg(target_os = "windows")]
impl SystemAudioCapture {
    pub fn start(path: PathBuf) -> Result<Self, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker_path = path.clone();
        let (ready_tx, ready_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let result = capture_windows_loopback(worker_path, worker_stop, ready_tx);
            result
        });
        ready_rx
            .recv()
            .map_err(|_| "Windows system audio worker stopped unexpectedly".to_string())??;
        Ok(Self {
            stop,
            worker: Some(worker),
            path,
        })
    }

    pub fn stop(mut self) -> Result<PathBuf, String> {
        self.stop.store(true, Ordering::Release);
        let worker = self
            .worker
            .take()
            .ok_or_else(|| "Windows system audio worker was already stopped".to_string())?;
        worker
            .join()
            .map_err(|_| "Windows system audio worker panicked".to_string())??;
        Ok(self.path)
    }
}

#[cfg(target_os = "windows")]
fn capture_windows_loopback(
    path: PathBuf,
    stop: Arc<AtomicBool>,
    ready_tx: mpsc::Sender<Result<(), String>>,
) -> Result<PathBuf, String> {
    let com_result = initialize_mta();
    if com_result.is_err() {
        return Err(format!(
            "Unable to initialize Windows audio runtime: {com_result:?}"
        ));
    }
    let enumerator = DeviceEnumerator::new()
        .map_err(|error| format!("Unable to enumerate Windows audio devices: {error}"))?;
    let device = enumerator
        .get_default_device(&Direction::Render)
        .map_err(|error| format!("Unable to find the default Windows output device: {error}"))?;
    let mut audio_client = device
        .get_iaudioclient()
        .map_err(|error| format!("Unable to open the Windows audio client: {error}"))?;
    let format = WaveFormat::new(32, 32, &SampleType::Float, 16_000, 1, None);
    let (_, minimum_period) = audio_client
        .get_device_period()
        .map_err(|error| format!("Unable to read Windows audio period: {error}"))?;
    let mode = StreamMode::EventsShared {
        autoconvert: true,
        buffer_duration_hns: minimum_period,
    };
    audio_client
        .initialize_client(&format, &Direction::Render, &mode)
        .map_err(|error| format!("Unable to initialize Windows loopback capture: {error}"))?;
    let event = audio_client
        .set_get_eventhandle()
        .map_err(|error| format!("Unable to create Windows audio event: {error}"))?;
    let capture = audio_client
        .get_audiocaptureclient()
        .map_err(|error| format!("Unable to open Windows capture buffer: {error}"))?;
    let mut sample_queue = std::collections::VecDeque::new();
    let mut writer = WavWriter::create(
        &path,
        WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .map_err(|error| format!("Unable to create system audio file: {error}"))?;
    audio_client
        .start_stream()
        .map_err(|error| format!("Unable to start Windows loopback capture: {error}"))?;
    let _ = ready_tx.send(Ok(()));
    while !stop.load(Ordering::Acquire) {
        capture
            .read_from_device_to_deque(&mut sample_queue)
            .map_err(|error| format!("Unable to read Windows system audio: {error}"))?;
        while sample_queue.len() >= 4 {
            let value = f32::from_le_bytes([
                sample_queue.pop_front().unwrap(),
                sample_queue.pop_front().unwrap(),
                sample_queue.pop_front().unwrap(),
                sample_queue.pop_front().unwrap(),
            ]);
            let _ = writer.write_sample((value.clamp(-1.0, 1.0) * i16::MAX as f32) as i16);
        }
        let _ = event.wait_for_event(100);
    }
    let _ = audio_client.stop_stream();
    writer
        .finalize()
        .map_err(|error| format!("Unable to finalize system audio file: {error}"))?;
    Ok(path)
}
