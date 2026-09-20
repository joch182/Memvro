use hound::{WavReader, WavSpec, WavWriter};
use std::fs::File;
use std::io::Read;
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
    if is_git_lfs_pointer(model_path) {
        return Err(format!(
            "Whisper model at {} is not downloaded. Open Settings and download a model",
            model_path.display()
        ));
    }

    let (microphone_audio, microphone_rate) = load_wav(microphone_path)?;
    let microphone_audio = resample_mono(&microphone_audio, microphone_rate, 16_000);
    let mut system_audio = None;
    if let Some(system_path) = system_path.filter(|path| path.is_file()) {
        let (samples, system_rate) = load_wav(system_path)?;
        system_audio = Some(resample_mono(&samples, system_rate, 16_000));
    }
    let mut audio = combine_audio(&microphone_audio, system_audio.as_deref());
    if !has_audio_signal(&audio) {
        return Err("The recording contains no audio".to_string());
    }
    normalize_audio(&mut audio);
    let mixed_path = microphone_path.with_file_name("mixed.wav");
    write_wav(&mixed_path, &audio, 16_000)?;

    let context = WhisperContext::new_with_params(
        model_path.to_string_lossy().as_ref(),
        WhisperContextParameters::default(),
    )
    .map_err(|error| format!("Unable to load Whisper model: {error}"))?;
    let mut state = context
        .create_state()
        .map_err(|error| format!("Unable to create Whisper state: {error}"))?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(None);
    params.set_detect_language(true);
    params.set_no_speech_thold(1.0);
    params.set_temperature_inc(0.2);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    state
        .full(params, &audio)
        .map_err(|error| format!("Unable to transcribe recording: {error}"))?;
    if state.full_n_segments() == 0
        && whisper_rs::get_lang_str(state.full_lang_id_from_state()) == Some("es")
    {
        let mut retry_params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        retry_params.set_language(Some("es"));
        retry_params.set_detect_language(false);
        retry_params.set_no_speech_thold(1.0);
        retry_params.set_temperature_inc(0.2);
        retry_params.set_print_special(false);
        retry_params.set_print_progress(false);
        retry_params.set_print_realtime(false);
        retry_params.set_print_timestamps(false);
        state
            .full(retry_params, &audio)
            .map_err(|error| format!("Unable to transcribe recording: {error}"))?;
    }

    let transcript = state
        .as_iter()
        .map(|segment| segment.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let transcript_path = microphone_path.with_extension("txt");
    if transcript.trim().is_empty() {
        let diagnostic = format!(
            "[No speech detected]\n\nThe recording contains audio, but Whisper did not identify spoken words. The exact audio sent to Whisper was saved to {}.\n",
            mixed_path.display()
        );
        std::fs::write(&transcript_path, diagnostic)
            .map_err(|error| format!("Unable to write transcript diagnostic: {error}"))?;
        return Err(
            format!(
                "No speech was detected. A diagnostic file was saved to {}.",
                transcript_path.display()
            ),
        );
    }
    std::fs::write(&transcript_path, format!("{transcript}\n"))
        .map_err(|error| format!("Unable to write transcript: {error}"))?;
    Ok(transcript_path)
}

fn is_git_lfs_pointer(path: &Path) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let mut header = [0_u8; 40];
    let Ok(bytes_read) = file.read(&mut header) else {
        return false;
    };
    header[..bytes_read].starts_with(b"version https://git-lfs.github.com/spec/v1")
}

fn has_audio_signal(samples: &[f32]) -> bool {
    samples.iter().any(|sample| sample.abs() > 0.001)
}

fn normalize_audio(samples: &mut [f32]) {
    if samples.is_empty() {
        return;
    }
    let rms = (samples.iter().map(|sample| sample * sample).sum::<f32>()
        / samples.len() as f32)
        .sqrt();
    if rms > 0.0 {
        let gain = (0.12 / rms).min(8.0);
        for sample in samples {
            *sample = (*sample * gain).clamp(-1.0, 1.0);
        }
    }
}

fn combine_audio(microphone: &[f32], system: Option<&[f32]>) -> Vec<f32> {
    let Some(system) = system.filter(|samples| has_audio_signal(samples)) else {
        return microphone.to_vec();
    };
    let mut combined = vec![0.0; microphone.len().max(system.len())];
    for index in 0..combined.len() {
        let microphone_sample = microphone.get(index).copied().unwrap_or(0.0);
        let system_sample = system.get(index).copied().unwrap_or(0.0);
        combined[index] = (microphone_sample * 0.5 + system_sample * 0.5).clamp(-1.0, 1.0);
    }
    combined
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

fn write_wav(path: &Path, samples: &[f32], sample_rate: u32) -> Result<(), String> {
    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = WavWriter::create(path, spec)
        .map_err(|error| format!("Unable to create mixed audio file: {error}"))?;
    for sample in samples {
        writer
            .write_sample((sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
            .map_err(|error| format!("Unable to write mixed audio file: {error}"))?;
    }
    writer
        .finalize()
        .map_err(|error| format!("Unable to finalize mixed audio file: {error}"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_audio_is_used_when_microphone_is_silent() {
        let combined = combine_audio(&[0.0, 0.0, 0.0], Some(&[0.4, -0.2, 0.1]));

        assert_eq!(combined, vec![0.2, -0.1, 0.05]);
    }

    #[test]
    fn quiet_audio_is_normalized_without_unbounded_gain() {
        let mut audio = vec![0.1, -0.05];

        normalize_audio(&mut audio);

        let rms = (audio.iter().map(|sample| sample * sample).sum::<f32>()
            / audio.len() as f32)
            .sqrt();
        assert!((rms - 0.12).abs() < 0.0001);
    }

}
