# Memvro

![Status: early development](https://img.shields.io/badge/status-early%20development-orange)
![License: MIT](https://img.shields.io/badge/license-MIT-green)

> A local-first desktop meeting recorder and transcription tool for macOS and Windows.

Memvro is a local-first desktop app for recording and transcribing online meetings. It is designed for meetings held in Teams, Slack, Zoom, web browsers, and other desktop applications without requiring a separate integration with each meeting provider.

When you start a recording, Memvro captures two audio sources on the computer:

- **Microphone audio:** your voice and any other sound received by the selected microphone.
- **System audio:** the sound played by the computer, such as the voices of other meeting participants and shared media.

When you stop recording, the app saves these audio tracks locally, prepares them for transcription, and combines them into a single audio stream. The selected Whisper model automatically detects English or Spanish and processes the audio on your computer. The resulting transcript is written to a plain-text `recording.txt` file in the recording folder.

The entire recording and transcription workflow runs locally. Memvro does not connect to Teams, Slack, Zoom, or another meeting-provider API, and it does not upload audio or transcript data to a cloud service. This makes the app suitable for workflows where keeping meeting data on the local device is important. You should still obtain any consent required by your organization or local laws before recording a meeting.

Memvro is currently an early-stage project. The macOS development path is available, while the Windows implementation still needs runtime validation on Windows 10/11.

The current version contains the desktop UI, native microphone and system-audio capture, local Whisper transcription, and text export.

Recordings and transcripts use the storage location selected in **Settings**. Each session creates a local-time folder like `2026-09-16_21-58-04` containing `recording.wav`, `system_audio.wav` on supported platforms, and `recording.txt`. The location is saved in the app settings and reused on the next launch.

## Contents

- [Features](#features)
- [Privacy and consent](#privacy-and-consent)
- [Platform status](#platform-status)
- [Getting started](#getting-started)
- [Storage](#storage)
- [Development](#development)
- [Architecture](#architecture)
- [Limitations](#limitations)
- [Contributing](#contributing)
- [Security and privacy reports](#security-and-privacy-reports)
- [License](#license)

## Features

- Capture the default microphone and computer playback audio as separate WAV tracks.
- Record meetings from Teams, Slack, Zoom, browsers, or other desktop applications without provider APIs.
- Choose and download a Whisper model from Settings, including multilingual English/Spanish detection.
- Resample and mix the audio tracks before transcription.
- Choose a persistent local storage directory from the app settings.
- Export a plain-text transcript for each recording.

## Privacy and consent

Audio capture, transcription, and transcript generation happen on the local device. Memvro does not require an account, API key, meeting-provider integration, or cloud transcription service, and it does not intentionally upload recordings or transcripts.

Local processing does not replace consent requirements. Before recording, follow applicable laws, workplace policies, and the rules of the meeting or organization.

## Platform Status

| Platform | Status | System audio |
| --- | --- | --- |
| macOS | Development path available; needs broader runtime and audio validation | ScreenCaptureKit |
| Windows 10/11 | Implementation present; runtime validation pending | WASAPI loopback |

The project currently targets developers and testers rather than end users. Packaged installers and release artifacts are not published yet.

## Getting Started

### Clone the repository

```bash
git clone https://github.com/joch182/Memvro.git
cd Memvro
```

### Run the UI preview

Requires Node.js and npm.

```bash
npm install
npm run dev
```

Open the local URL printed by Vite. This preview lets you inspect the interface, but native recording commands only work inside the Tauri desktop app.

### Test the desktop app on macOS

Use this workflow to test microphone capture, system-audio capture, local transcription, and transcript export on a Mac.

#### 1. Install the prerequisites

The native Tauri app requires Node.js, npm, Rust, CMake, and the full Xcode installation. The full Xcode SDK is required by ScreenCaptureKit; Xcode Command Line Tools alone are not sufficient.

Install Rust and CMake if needed:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
brew install cmake
```

Select Xcode and complete its first-launch setup:

```bash
sudo xcode-select -s /Applications/Xcode.app/Contents/Developer
sudo xcodebuild -runFirstLaunch
xcodebuild -version
```

#### 2. Start Memvro

From the project directory, run this command:

```bash
npm install
RUSTFLAGS='-C link-arg=-Wl,-rpath,/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift-5.5/macosx' npm run tauri dev
```

The `RUSTFLAGS` value adds the Swift runtime path required by the current Xcode toolchain. This is the recommended macOS development command because it avoids the `libswift_Concurrency.dylib` startup error.

You can use the same command in two readable steps:

```bash
export RUSTFLAGS='-C link-arg=-Wl,-rpath,/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift-5.5/macosx'
npm run tauri dev
```

If your Xcode and Swift runtime are already configured in your shell, this shorter command may work:

```bash
npm run tauri dev
```

If it fails with `Library not loaded: @rpath/libswift_Concurrency.dylib`, use the recommended command with `RUSTFLAGS` above.

#### 3. Allow macOS permissions

When Memvro starts, allow the application that launched it to access the microphone and screen recording:

1. Open **System Settings > Privacy & Security > Microphone** and enable the terminal or VS Code used to launch Memvro.
2. Open **System Settings > Privacy & Security > Screen Recording** and enable the same application for system-audio capture.
3. Fully quit and restart Memvro after changing permissions.

#### 4. Download a transcription model

Open **Settings**, choose a model, and select **Download model**. The model download requires an internet connection. Recording and transcription run locally after the download completes.

#### 5. Test a recording

1. Start a meeting or play audio through the Mac.
2. Click the round record button in Memvro.
3. Speak into the microphone and play system audio for a few seconds.
4. Stop the recording.
5. Select **Transcribe locally**.

The session folder contains the microphone track, system-audio track, and resulting `recording.txt` transcript.

### Download a transcription model

Models are not bundled with the application. Open **Settings**, choose a model, and select **Download model**. The model is saved in Memvro's local application data directory and reused for future recordings. An internet connection is required only while downloading a model; recording and transcription remain local.

## Troubleshooting

If Vite reports that port `1420` is already in use, stop the stale development server and retry:

```bash
PIDS=$(lsof -tiTCP:1420 -sTCP:LISTEN)
if [[ -n "$PIDS" ]]; then kill $PIDS; fi
```

If the executable reports `Library not loaded: @rpath/libswift_Concurrency.dylib`, confirm that full Xcode is selected and start the app with the `RUSTFLAGS` command above. A successful `cargo check` does not guarantee that the native executable can launch; this Swift runtime lookup happens at runtime.

## Storage

Choose the root storage folder from **Settings**. Memvro remembers the setting and creates one folder per recording:

```text
<storage folder>/<YYYY-MM-DD_HH-MM-SS>/
	recording.wav
	system_audio.wav
	recording.txt
```

Downloaded models are stored in the app's local data directory. During development, `MEMVRO_WHISPER_MODEL_PATH` can override the selected model path.

## Development

### Validate the frontend

```bash
npm run build
```

This runs the TypeScript check and creates the production frontend bundle in `dist/`.

### Validate the Rust backend

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --manifest-path src-tauri/Cargo.toml
```

## Architecture

- **Frontend:** React, TypeScript, and Vite
- **Desktop shell:** Tauri 2
- **Microphone capture:** CPAL
- **macOS system audio:** ScreenCaptureKit
- **Windows system audio:** WASAPI loopback
- **Audio files:** WAV via Hound
- **Local transcription:** Whisper through `whisper-rs`
- **Settings:** Local JSON configuration and user-selected filesystem storage

## Limitations

- The native prototype captures the default microphone and system audio.
- macOS system audio capture uses ScreenCaptureKit and requires Screen Recording permission. Windows system audio capture uses shared-mode WASAPI loopback on the default output device.
- Microphone and system tracks are resampled and mixed before local transcription.
- Audio and transcripts are saved locally in timestamped session folders.
- The multilingual model detects English or Spanish automatically; a model must be downloaded before transcription.
- Windows runtime validation still requires building and running the app on Windows 10/11.
- No packaged desktop releases are available yet.

## Contributing

Issues and pull requests are welcome. Please include the operating system, audio devices, reproduction steps, and relevant logs when reporting capture or transcription problems.

Before opening a pull request:

1. Keep platform-specific Rust code behind the appropriate target configuration.
2. Run `npm run build`, `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`, and `cargo check --manifest-path src-tauri/Cargo.toml`.
3. Keep the local-first privacy model intact. Discuss telemetry, cloud services, or provider-specific integrations before implementing them.

## Security and privacy reports

Do not include meeting recordings, transcripts, API keys, or other sensitive information in public issues. For a suspected security or privacy vulnerability, contact the repository maintainer privately through GitHub before creating a public issue.

## License

Memvro is available under the [MIT License](LICENSE). Third-party dependencies and downloaded Whisper models may have additional license terms; review their notices before redistribution.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
