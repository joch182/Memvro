# Memvro

Memvro is a local-first desktop meeting transcription app. It captures system audio and microphone input, transcribes it locally, and exports a `.txt` file. It does not use meeting-provider APIs.

The current version contains the desktop UI, native microphone and system-audio capture, local Whisper transcription, and text export.

Recordings and transcripts use the storage location selected in **Settings**. Each session creates a local-time folder like `2026-09-16_21-58-04` containing `recording.wav`, `system_audio.wav` on supported platforms, and `recording.txt`. The location is saved in the app settings and reused on the next launch.

## Run the UI preview

Requires Node.js and npm.

```bash
npm install
npm run dev
```

Open the local URL printed by Vite. This preview lets you inspect the interface, but native recording commands only work inside the Tauri desktop app.

## Run the desktop app

The native Tauri app requires Rust, CMake, and the full Xcode installation on macOS. The full Xcode SDK is needed by ScreenCaptureKit; Command Line Tools alone are not sufficient. Install Rust if `cargo` is not available:

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

Then start the desktop development build:

```bash
npm install
RUSTFLAGS='-C link-arg=-Wl,-rpath,/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift-5.5/macosx' npm run tauri dev
```

The `RUSTFLAGS` value adds the Swift runtime path required by the current Xcode toolchain when running the local debug binary.

## Troubleshooting desktop startup

If Vite reports that port `1420` is already in use, stop the stale development server and retry:

```bash
PIDS=$(lsof -tiTCP:1420 -sTCP:LISTEN)
if [[ -n "$PIDS" ]]; then kill $PIDS; fi
```

If the executable reports `Library not loaded: @rpath/libswift_Concurrency.dylib`, confirm that full Xcode is selected and start the app with the `RUSTFLAGS` command above. A successful `cargo check` does not guarantee that the native executable can launch; this Swift runtime lookup happens at runtime.

## Validate the frontend

```bash
npm run build
```

This runs the TypeScript check and creates the production frontend bundle in `dist/`.

## Current limitations

- The native prototype captures the default microphone and system audio.
- macOS system audio capture uses ScreenCaptureKit and requires Screen Recording permission. Windows system audio capture uses shared-mode WASAPI loopback on the default output device.
- Microphone and system tracks are resampled and mixed before local transcription.
- Audio and transcripts are saved locally in timestamped session folders.
- The English `ggml-base.en.bin` model is bundled with the Tauri app. During development, `MEMVRO_WHISPER_MODEL_PATH` can override its location.
- Windows runtime validation still requires building and running the app on Windows 10/11.
- Rust is required to run the native Tauri application.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
