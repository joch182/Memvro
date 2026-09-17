use hound::WavReader;
use std::path::{Path, PathBuf};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub fn transcribe_wav(
    microphone_path: &Path,
    system_path: Option<&Path>,
    model_path: &Path,
) -> Result<PathBuf, String> {
    if !model_path.is_file() {
        return Err(format!(
            "Whisper model not found at {}",
            model_path.display()
        ));
    }

    let (microphone_audio, microphone_rate) = load_wav(microphone_path)?;
    let mut audio = resample_mono(&microphone_audio, microphone_rate, 16_000);
    if let Some(system_path) = system_path.filter(|path| path.is_file()) {
        let (system_audio, system_rate) = load_wav(system_path)?;
        let system_audio = resample_mono(&system_audio, system_rate, 16_000);
        audio.resize(audio.len().max(system_audio.len()), 0.0);
        for (index, sample) in system_audio.into_iter().enumerate() {
            audio[index] = (audio[index] * 0.5 + sample * 0.5).clamp(-1.0, 1.0);
        }
    }
    if audio.is_empty() {
        return Err("The recording contains no audio".to_string());
    }

    let context = WhisperContext::new_with_params(
        model_path.to_string_lossy().as_ref(),
        WhisperContextParameters::default(),
    )
    .map_err(|error| format!("Unable to load Whisper model: {error}"))?;
    let mut state = context
        .create_state()
        .map_err(|error| format!("Unable to create Whisper state: {error}"))?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some("en"));
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    state
        .full(params, &audio)
        .map_err(|error| format!("Unable to transcribe recording: {error}"))?;

    let transcript = state
        .as_iter()
        .map(|segment| segment.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let transcript_path = microphone_path.with_extension("txt");
    std::fs::write(&transcript_path, format!("{transcript}\n"))
        .map_err(|error| format!("Unable to write transcript: {error}"))?;
    Ok(transcript_path)
}

fn load_wav(path: &Path) -> Result<(Vec<f32>, u32), String> {
    let mut reader =
        WavReader::open(path).map_err(|error| format!("Unable to open recording: {error}"))?;
    let spec = reader.spec();
    if spec.channels != 1
        || spec.sample_format != hound::SampleFormat::Int
        || spec.bits_per_sample != 16
    {
        return Err("Recording must be a 16-bit mono WAV file".to_string());
    }
    let samples: Vec<i16> = reader
        .samples::<i16>()
        .collect::<Result<_, _>>()
        .map_err(|error| format!("Unable to read recording samples: {error}"))?;
    let mut audio = vec![0.0_f32; samples.len()];
    whisper_rs::convert_integer_to_float_audio(&samples, &mut audio)
        .map_err(|error| format!("Unable to convert recording audio: {error}"))?;
    Ok((audio, spec.sample_rate))
}

fn resample_mono(samples: &[f32], source_rate: u32, target_rate: u32) -> Vec<f32> {
    if source_rate == target_rate {
        return samples.to_vec();
    }
    let output_len = ((samples.len() as u64 * target_rate as u64) / source_rate as u64) as usize;
    (0..output_len)
        .map(|index| {
            let source_position = index as f64 * source_rate as f64 / target_rate as f64;
            let lower = source_position.floor() as usize;
            let upper = (lower + 1).min(samples.len() - 1);
            let fraction = source_position - lower as f64;
            samples[lower] * (1.0 - fraction as f32) + samples[upper] * fraction as f32
        })
        .collect()
}
