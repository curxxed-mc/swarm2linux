//! The default microphone, read with `pw-record`, cut into utterances by loudness.

use std::collections::VecDeque;
use std::io::{self, Read};
use std::process::{Child, Command, Stdio};

/// Samples per second; what the speech model takes.
pub const RATE: usize = 16_000;
/// 20 ms frames.
const FRAME: usize = RATE / 50;

/// `pw-record` on a source (by node name), or on the default one (following the default when that changes), as
/// 16 kHz mono 16-bit samples.
pub struct Microphone {
    child: Child,
}

impl Microphone {
    pub fn open(source: Option<&str>) -> io::Result<Self> {
        let mut command = Command::new("pw-record");
        if let Some(source) = source {
            command.args(["--target", source]);
        }
        let child = command
            .args(["--raw", "--rate", "16000", "--channels", "1", "--format", "s16", "--latency", "20ms"])
            .args(["-P", r#"{ media.name = "Voice commands" node.name = "stealth-voice" }"#, "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        Ok(Self { child })
    }

    /// The next frame, or an error when `pw-record` stopped.
    pub fn frame(&mut self, out: &mut Vec<i16>) -> io::Result<()> {
        let mut bytes = [0u8; FRAME * 2];
        self.child.stdout.as_mut().ok_or_else(|| io::Error::other("no output"))?.read_exact(&mut bytes)?;
        out.clear();
        out.extend(bytes.as_chunks::<2>().0.iter().map(|&b| i16::from_le_bytes(b)));
        Ok(())
    }
}

impl Drop for Microphone {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn rms(frame: &[i16]) -> f32 {
    let sum: f64 = frame.iter().map(|&s| f64::from(s) * f64::from(s)).sum();
    (sum / frame.len().max(1) as f64).sqrt() as f32
}

/// Cuts speech out of a stream of frames: it starts when the level rises well above the background, and ends after
/// a pause.
pub struct Segmenter {
    /// The background level, followed while nobody speaks.
    floor: f32,
    /// Frames from before speech started, so the first word is whole.
    before: VecDeque<Vec<i16>>,
    speech: Vec<i16>,
    loud: usize,
    quiet: usize,
    speaking: bool,
    /// Frames seen so far, up to `WARM_UP`: the first ones only measure the background.
    seen: usize,
}

const WARM_UP: usize = 25; // 500 ms
const PRE_ROLL: usize = 15; // 300 ms
const START_FRAMES: usize = 3; // 60 ms above the threshold
const END_FRAMES: usize = 35; // 700 ms of quiet
const MIN_SPEECH: usize = RATE * 2 / 5; // 400 ms
const MAX_SPEECH: usize = RATE * 6; // 6 s

impl Default for Segmenter {
    fn default() -> Self {
        Self { floor: 100.0, before: VecDeque::new(), speech: Vec::new(), loud: 0, quiet: 0, speaking: false, seen: 0 }
    }
}

impl Segmenter {
    /// Twice the background: a noisy laptop microphone's background was half as loud as speech.
    fn threshold(&self) -> f32 {
        (self.floor * 2.0).max(250.0)
    }

    /// Feed one frame; returns an utterance once it ends.
    pub fn push(&mut self, frame: &[i16]) -> Option<Vec<i16>> {
        let level = rms(frame);
        if self.seen < WARM_UP {
            self.seen += 1;
            self.floor = if self.seen == 1 { level } else { self.floor + (level - self.floor) / self.seen as f32 };
            return None;
        }
        let threshold = self.threshold();
        if !self.speaking {
            // the floor follows the background slowly, and drops at once in a quieter room
            self.floor = if level < self.floor { level.max(20.0) } else { self.floor * 0.98 + level * 0.02 };
            self.before.push_back(frame.to_vec());
            if self.before.len() > PRE_ROLL {
                self.before.pop_front();
            }
            self.loud = if level > threshold { self.loud + 1 } else { 0 };
            if self.loud >= START_FRAMES {
                self.speaking = true;
                self.quiet = 0;
                self.speech = self.before.drain(..).flatten().collect();
            }
            return None;
        }
        self.speech.extend_from_slice(frame);
        self.quiet = if level < threshold * 0.7 { self.quiet + 1 } else { 0 };
        if self.quiet >= END_FRAMES || self.speech.len() >= MAX_SPEECH {
            self.speaking = false;
            self.loud = 0;
            let speech = std::mem::take(&mut self.speech);
            return (speech.len() >= MIN_SPEECH + END_FRAMES * FRAME).then_some(speech);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(level: i16, frames: usize) -> Vec<Vec<i16>> {
        (0..frames).map(|_| (0..FRAME).map(|i| if i % 2 == 0 { level } else { -level }).collect()).collect()
    }

    #[test]
    fn cuts_out_speech_between_pauses() {
        let mut s = Segmenter::default();
        let mut out = Vec::new();
        for frame in tone(30, 50).into_iter().chain(tone(3000, 50)).chain(tone(30, 60)) {
            out.extend(s.push(&frame));
        }
        assert_eq!(out.len(), 1);
        // the pre-roll, the speech, and the pause that ended it
        let len = out[0].len() / FRAME;
        assert!((50..=50 + PRE_ROLL + END_FRAMES).contains(&len), "{len} frames");
    }

    #[test]
    fn ignores_clicks_and_steady_noise() {
        let mut s = Segmenter::default();
        let frames = tone(30, 50).into_iter().chain(tone(3000, 2)).chain(tone(30, 60));
        assert!(frames.filter_map(|f| s.push(&f)).next().is_none(), "a click");
        let mut s = Segmenter::default();
        assert!(tone(600, 500).into_iter().filter_map(|f| s.push(&f)).next().is_none(), "a fan");
        // speech over the fan still counts
        assert!(tone(4000, 50).into_iter().chain(tone(600, 60)).filter_map(|f| s.push(&f)).next().is_some());
    }
}
