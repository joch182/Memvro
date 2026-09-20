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
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "macos")]
use std::sync::{Arc, Mutex};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use crate::audio::SharedLevel;

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
    pub fn start(path: PathBuf, level: SharedLevel) -> Result<Self, String> {
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
            .with_sample_rate(48_000)
            .with_channel_count(2);
        let writer = Arc::new(Mutex::new(Some(
            WavWriter::create(
                &path,
                WavSpec {
                    channels: 1,
                    sample_rate: 48_000,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                },
            )
            .map_err(|error| format!("Unable to create system audio file: {error}"))?,
        )));
        let callback_writer = Arc::clone(&writer);
        let callback_level = Arc::clone(&level);
        let format_logged = Arc::new(AtomicBool::new(false));
        let callback_format_logged = Arc::clone(&format_logged);
        let mut stream = SCStream::new(&filter, &config);
        stream.add_output_handler(
            move |sample: CMSampleBuffer, output_type: SCStreamOutputType| {
                if !matches!(output_type, SCStreamOutputType::Audio) {
                    return;
                }
                let Some(buffers) = sample.audio_buffer_list() else {
                    return;
                };
                let format = sample.format_description();
                let bits_per_sample = format
                    .as_ref()
                    .and_then(|description| description.audio_bits_per_channel())
                    .unwrap_or(32);
                let is_float = format
                    .as_ref()
                    .is_some_and(|description| description.audio_is_float());
                let bytes_per_sample = (bits_per_sample / 8) as usize;
                if bytes_per_sample == 0 {
                    return;
                }
                if !callback_format_logged.swap(true, Ordering::AcqRel) {
                    let details = format.as_ref().map(|description| {
                        format!(
                            "rate={:?}, channels={:?}, bits={bits_per_sample}, bytes/frame={:?}, flags={:?}, float={is_float}",
                            description.audio_sample_rate(),
                            description.audio_channel_count(),
                            description.audio_bytes_per_frame(),
                            description.audio_format_flags()
                        )
                    });
                    let first_bytes = buffers
                        .iter()
                        .next()
                        .map(|buffer| {
                            buffer
                                .data()
                                .iter()
                                .take(16)
                                .map(|byte| format!("{byte:02x}"))
                                .collect::<Vec<_>>()
                                .join(" ")
                        });
                    eprintln!("ScreenCaptureKit audio format: {details:?}; first bytes: {first_bytes:?}");
                }
                let Ok(mut writer) = callback_writer.lock() else {
                    return;
                };
                let Some(writer) = writer.as_mut() else {
                    return;
                };
                for buffer in buffers.iter() {
                    let channels = buffer.number_channels().max(1) as usize;
                    let frame_size = bytes_per_sample * channels;
                    for frame in buffer.data().chunks_exact(frame_size) {
                        let value = frame
                            .chunks_exact(bytes_per_sample)
                            .map(|bytes| decode_pcm_sample(bytes, bits_per_sample, is_float))
                            .sum::<f32>()
                            / channels as f32;
                        crate::audio::update_level(&callback_level, value);
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
        let writer = self
            .writer
            .lock()
            .map_err(|_| "Unable to lock system audio file".to_string())?
            .take()
            .ok_or_else(|| "System audio file was already closed".to_string())?;
        writer
            .finalize()
            .map_err(|error| format!("Unable to finalize system audio file: {error}"))?;
        Ok(self.path)
    }
}

#[cfg(target_os = "macos")]
fn decode_pcm_sample(bytes: &[u8], bits_per_sample: u32, is_float: bool) -> f32 {
    match (bits_per_sample, is_float) {
        (32, true) if bytes.len() >= 4 => f32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        (32, false) if bytes.len() >= 4 => {
            i32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f32 / i32::MAX as f32
        }
        (16, false) if bytes.len() >= 2 => {
            i16::from_ne_bytes([bytes[0], bytes[1]]) as f32 / i16::MAX as f32
        }
        _ => 0.0,
    }
}

#[cfg(not(target_os = "macos"))]
#[cfg(not(target_os = "windows"))]
pub struct SystemAudioCapture;

#[cfg(not(target_os = "macos"))]
#[cfg(not(target_os = "windows"))]
impl SystemAudioCapture {
    pub fn start(_path: std::path::PathBuf, _level: crate::audio::SharedLevel) -> Result<Self, String> {
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
    pub fn start(path: PathBuf, level: SharedLevel) -> Result<Self, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker_path = path.clone();
        let (ready_tx, ready_rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let result = capture_windows_loopback(worker_path, worker_stop, ready_tx, level);
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
    level: SharedLevel,
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
            crate::audio::update_level(&level, value);
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
