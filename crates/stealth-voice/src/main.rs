//! `stealth-voice`: experimental voice commands. It listens on the default microphone for "Stealth, ..." and
//! controls media players ("skip this song", "pause", "resume", "go back"), the default output's volume
//! ("volume up/down") and the headset's noise cancellation.
//!
//! Speech is recognised on this computer with whisper.cpp; nothing leaves it. The model (`ggml-base.en-q5_1.bin`,
//! 57 MB) is downloaded from Hugging Face on first use into `~/.local/share/swarm2linux/` and checked against its
//! SHA-256.
//!
//! stealthd starts it while voice commands are on, and reads its standard output: `status: <text>` lines say what
//! it is doing, `heard: <text>` lines what it understood.

mod actions;
mod audio;
mod commands;

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command as Process;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::Parser;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState};

use crate::audio::{Microphone, RATE, Segmenter};

/// The fewest encoder positions (of 1500 for 30 s) to recognise with.
const MIN_AUDIO_CTX: usize = 768;
/// 30 s of frames.
const SILENT_FRAMES: usize = 1500;
const SILENT_STATUS: &str =
    "listening, but the microphone has been silent for 30 s: is it muted, or its sensitivity at 0?";

const MODEL_FILE: &str = "ggml-base.en-q5_1.bin";
const MODEL_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en-q5_1.bin";
const MODEL_SHA256: &str = "4baf70dd0d7c4247ba2b81fafd9c01005ac77c2f9ef064e00dcf195d0e2fdd2f";
/// Biases the model towards the wake word and the commands' words.
const PROMPT: &str = "Stealth, skip this song. Stealth, pause. Stealth, resume.";

#[derive(Parser)]
#[command(version, about = "Experimental voice commands for the default microphone: \"Stealth, skip this song\"")]
struct Args {
    /// The whisper.cpp model to use instead of the downloaded one
    #[arg(long)]
    model: Option<PathBuf>,
    /// The microphone to listen on: a PipeWire node name (see `wpctl status`); the default one without it
    #[arg(long, value_name = "NODE")]
    microphone: Option<String>,
    /// Print what is heard and understood, but do nothing
    #[arg(long)]
    dry_run: bool,
    /// Transcribe a file of raw 16 kHz mono 16-bit samples, print the text and the command, and exit
    #[arg(long, value_name = "FILE")]
    file: Option<PathBuf>,
}

fn status(text: &str) {
    println!("status: {text}");
    let _ = io::stdout().flush();
}

fn main() -> Result<()> {
    let args = Args::parse();
    whisper_rs::install_logging_hooks();
    let model = match args.model {
        Some(path) => path,
        None => ensure_model()?,
    };
    status("loading the speech model");
    let ctx = WhisperContext::new_with_params(&model, WhisperContextParameters::default())
        .with_context(|| format!("loading {}", model.display()))?;
    let mut state = ctx.create_state().context("starting the speech model")?;

    if let Some(file) = args.file {
        let bytes = std::fs::read(&file).with_context(|| format!("reading {}", file.display()))?;
        let samples: Vec<i16> = bytes.as_chunks::<2>().0.iter().map(|&b| i16::from_le_bytes(b)).collect();
        let started = Instant::now();
        let text = transcribe(&mut state, &samples)?;
        println!("heard: {text:?} in {} ms", started.elapsed().as_millis());
        println!("command: {:?}", commands::parse(&text));
        return Ok(());
    }

    // the recogniser takes utterances one at a time; while it is busy, new ones wait (two at most)
    let (utterances, queue) = mpsc::sync_channel::<Vec<i16>>(2);
    let microphone = args.microphone.clone();
    thread::Builder::new().name("microphone".into()).spawn(move || listen(microphone.as_deref(), utterances))?;

    let mut actions = actions::Actions::new()?;
    for speech in queue {
        let text = match transcribe(&mut state, &speech) {
            Ok(text) => text,
            Err(e) => {
                eprintln!("stealth-voice: {e:#}");
                continue;
            }
        };
        let Some(command) = commands::parse(&text) else {
            // what was heard, for working out why a command was missed (stealthd puts it in its journal)
            if !text.is_empty() {
                println!("ignored: {text}");
                let _ = io::stdout().flush();
            }
            continue;
        };
        println!("heard: {text} -> {command:?}");
        let _ = io::stdout().flush();
        if args.dry_run {
            continue;
        }
        match actions.run(command) {
            Ok(()) => actions.popup(command.done(), &text),
            Err(e) => {
                eprintln!("stealth-voice: {command:?}: {e:#}");
                actions.popup(&format!("{} failed", command.done()), &format!("{e:#}"));
            }
        }
    }
    bail!("the microphone thread stopped")
}

/// Read the microphone and send utterances; reopen it when `pw-record` stops.
fn listen(microphone: Option<&str>, utterances: mpsc::SyncSender<Vec<i16>>) {
    let mut frame = Vec::new();
    loop {
        let mut mic = match Microphone::open(microphone) {
            Ok(mic) => mic,
            Err(e) => {
                status(&format!("cannot record: {e}"));
                thread::sleep(Duration::from_secs(5));
                continue;
            }
        };
        status("listening");
        let mut segmenter = Segmenter::default();
        // frames in a row with no sound at all: a muted microphone, or one turned all the way down
        let mut silent = 0usize;
        let error = loop {
            if let Err(e) = mic.frame(&mut frame) {
                break e;
            }
            if audio::rms(&frame) < 3.0 {
                silent += 1;
                if silent == SILENT_FRAMES {
                    status(SILENT_STATUS);
                }
            } else {
                if silent >= SILENT_FRAMES {
                    status("listening");
                }
                silent = 0;
            }
            if let Some(speech) = segmenter.push(&frame) {
                // drop it if the recogniser is behind; a command said long ago is not wanted any more
                match utterances.try_send(speech) {
                    Ok(()) | Err(mpsc::TrySendError::Full(_)) => {}
                    Err(mpsc::TrySendError::Disconnected(_)) => return,
                }
            }
        };
        status(&format!("the microphone stopped ({error}); opening it again"));
        thread::sleep(Duration::from_secs(1));
    }
}

/// The text of an utterance; empty when the model thinks it is not speech.
fn transcribe(state: &mut WhisperState, samples: &[i16]) -> Result<String> {
    let mut audio: Vec<f32> = samples.iter().map(|&s| f32::from(s) / 32768.0).collect();
    // whisper.cpp wants at least a second
    audio.resize(audio.len().max(RATE * 11 / 10), 0.0);
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some("en"));
    params.set_n_threads(thread::available_parallelism().map_or(4, |n| (n.get() / 2).clamp(1, 8)) as i32);
    // the encoder works on 30 s by default (1500 positions, about 1.3 s here); a command is a few seconds, so
    // encode less. Much less than half garbles short commands ("Stealth" came out as "Third" at 384).
    let positions = ((audio.len() * 50).div_ceil(RATE) + 64).max(MIN_AUDIO_CTX);
    params.set_audio_ctx(positions.min(1500) as i32);
    params.set_no_context(true);
    params.set_single_segment(true);
    params.set_no_timestamps(true);
    params.set_suppress_blank(true);
    params.set_suppress_nst(true);
    params.set_initial_prompt(PROMPT);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    state.full(params, &audio).context("recognising speech")?;
    let mut text = String::new();
    for segment in state.as_iter() {
        if segment.no_speech_probability() > 0.6 {
            continue;
        }
        text.push_str(&segment.to_str_lossy().map(|s| s.into_owned()).unwrap_or_default());
    }
    Ok(text.trim().to_string())
}

/// `~/.local/share/swarm2linux`, or its place under `$XDG_DATA_HOME`.
fn data_dir() -> Option<PathBuf> {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))?;
    Some(data.join("swarm2linux"))
}

/// The model's path, downloading it first if it is not there.
fn ensure_model() -> Result<PathBuf> {
    let dir = data_dir().context("no home directory")?;
    let path = dir.join(MODEL_FILE);
    if path.exists() {
        return Ok(path);
    }
    std::fs::create_dir_all(&dir)?;
    status("downloading the speech model (57 MB)");
    let partial = path.with_extension("bin.part");
    let ok = Process::new("curl")
        .args(["-fsSL", "--retry", "2", "-o"])
        .arg(&partial)
        .arg(MODEL_URL)
        .status()
        .context("running curl")?
        .success();
    if !ok {
        let _ = std::fs::remove_file(&partial);
        bail!("downloading the speech model failed");
    }
    if let Err(e) = check_sha256(&partial) {
        let _ = std::fs::remove_file(&partial);
        return Err(e);
    }
    std::fs::rename(&partial, &path)?;
    Ok(path)
}

fn check_sha256(path: &Path) -> Result<()> {
    let out = Process::new("sha256sum").arg(path).output().context("running sha256sum")?;
    let text = String::from_utf8_lossy(&out.stdout);
    if text.split_whitespace().next() != Some(MODEL_SHA256) {
        bail!("the downloaded speech model is not the expected file");
    }
    Ok(())
}
