use crate::system_audio::SystemAudioCapture;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat, SizedSample, Stream, StreamConfig};
use dasp_sample::FromSample;
use hound::{WavSpec, WavWriter};
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

type SharedWriter = Arc<Mutex<Option<WavWriter<BufWriter<File>>>>>;

pub type SharedLevel = Arc<AtomicU32>;

pub fn read_level(level: &SharedLevel) -> f32 {
    f32::from_bits(level.swap(0.0_f32.to_bits(), Ordering::AcqRel))
}

pub fn update_level(level: &SharedLevel, sample: f32) {
    let value = sample.abs().clamp(0.0, 1.0);
    let mut current = level.load(Ordering::Relaxed);
    while value > f32::from_bits(current) {
        match level.compare_exchange_weak(
            current,
            value.to_bits(),
            Ordering::AcqRel,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(next) => current = next,
        }
    }
}

pub struct RecordingController {
    commands: Sender<Command>,
    level: SharedLevel,
}

enum Command {
    Start(PathBuf, Sender<Result<(), String>>),
    Stop(Sender<Result<PathBuf, String>>),
}

impl RecordingController {
    pub fn new() -> Self {
        let (commands, receiver) = mpsc::channel();
        let level = Arc::new(AtomicU32::new(0.0_f32.to_bits()));
        let worker_level = Arc::clone(&level);
        thread::spawn(move || recording_worker(receiver, worker_level));
        Self { commands, level }
    }

    pub fn start(&self, path: PathBuf) -> Result<(), String> {
        let (response, receiver) = mpsc::channel();
        self.commands
            .send(Command::Start(path, response))
            .map_err(|_| "Audio worker is not available".to_string())?;
        receiver
            .recv()
            .map_err(|_| "Audio worker stopped unexpectedly".to_string())?
    }

    pub fn stop(&self) -> Result<PathBuf, String> {
        let (response, receiver) = mpsc::channel();
        self.commands
            .send(Command::Stop(response))
            .map_err(|_| "Audio worker is not available".to_string())?;
        receiver
            .recv()
            .map_err(|_| "Audio worker stopped unexpectedly".to_string())?
    }

    pub fn level(&self) -> f32 {
        read_level(&self.level)
    }
}

fn recording_worker(receiver: Receiver<Command>, level: SharedLevel) {
    let mut recording: Option<Recording> = None;
    while let Ok(command) = receiver.recv() {
        match command {
            Command::Start(path, response) => {
                let result = if recording.is_some() {
                    Err("A recording is already in progress".to_string())
                } else {
                    match Recording::start(path, Arc::clone(&level)) {
                        Ok(new_recording) => {
                            recording = Some(new_recording);
                            Ok(())
                        }
                        Err(error) => Err(error),
                    }
                };
                let _ = response.send(result);
            }
            Command::Stop(response) => {
                let result = recording
                    .take()
                    .ok_or_else(|| "No recording is in progress".to_string())
                    .and_then(Recording::stop);
                let _ = response.send(result);
            }
        }
    }
}

struct Recording {
    stream: Stream,
    system_audio: SystemAudioCapture,
    writer: SharedWriter,
    path: PathBuf,
}

impl Recording {
    fn start(path: PathBuf, level: SharedLevel) -> Result<Self, String> {
        let system_audio_path = path.with_file_name("system_audio.wav");
        let system_audio = SystemAudioCapture::start(system_audio_path, Arc::clone(&level))?;
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| "No microphone input device is available".to_string())?;
        let supported_config = device
            .default_input_config()
            .map_err(|error| format!("Unable to read microphone configuration: {error}"))?;
        let sample_format = supported_config.sample_format();
        let config: StreamConfig = supported_config.into();
        let channels = config.channels as usize;
        let spec = WavSpec {
            channels: 1,
            sample_rate: config.sample_rate.0,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let writer = Arc::new(Mutex::new(Some(
            WavWriter::create(&path, spec)
                .map_err(|error| format!("Unable to create recording file: {error}"))?,
        )));
        let callback_writer = Arc::clone(&writer);
        let callback_level = Arc::clone(&level);
        let error_callback = |error| eprintln!("microphone stream error: {error}");
        let stream = match sample_format {
            SampleFormat::I8 => {
                build_stream::<i8>(&device, &config, channels, callback_writer, callback_level, error_callback)
            }
            SampleFormat::I16 => {
                build_stream::<i16>(&device, &config, channels, callback_writer, callback_level, error_callback)
            }
            SampleFormat::I32 => {
                build_stream::<i32>(&device, &config, channels, callback_writer, callback_level, error_callback)
            }
            SampleFormat::F32 => {
                build_stream::<f32>(&device, &config, channels, callback_writer, callback_level, error_callback)
            }
            format => Err(format!("Unsupported microphone sample format: {format:?}")),
        };
        let stream = match stream {
            Ok(stream) => stream,
            Err(error) => {
                let _ = system_audio.stop();
                return Err(error);
            }
        };
        stream
            .play()
            .map_err(|error| format!("Unable to start microphone: {error}"))?;
        Ok(Self {
            stream,
            system_audio,
            writer,
            path,
        })
    }

    fn stop(self) -> Result<PathBuf, String> {
        drop(self.stream);
        self.system_audio.stop()?;
        let writer = self
            .writer
            .lock()
            .map_err(|_| "Unable to lock recording file".to_string())?
            .take()
            .ok_or_else(|| "Recording file was already closed".to_string())?;
        writer
            .finalize()
            .map_err(|error| format!("Unable to finalize recording file: {error}"))?;
        Ok(self.path)
    }
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    channels: usize,
    writer: SharedWriter,
    level: SharedLevel,
    error_callback: impl Fn(cpal::StreamError) + Send + 'static,
) -> Result<Stream, String>
where
    T: Sample + SizedSample,
    f32: FromSample<T>,
{
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                let Ok(mut writer) = writer.lock() else {
                    return;
                };
                let Some(writer) = writer.as_mut() else {
                    return;
                };
                for frame in data.chunks(channels) {
                    let average = frame
                        .iter()
                        .map(|sample| sample.to_sample::<f32>())
                        .sum::<f32>()
                        / frame.len() as f32;
                    update_level(&level, average);
                    let sample = (average.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                    let _ = writer.write_sample(sample);
                }
            },
            error_callback,
            None,
        )
        .map_err(|error| format!("Unable to open microphone stream: {error}"))
}
