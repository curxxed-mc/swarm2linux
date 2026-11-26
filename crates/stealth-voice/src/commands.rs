//! What a heard sentence asks for: the wake word "Stealth", then a command.

/// A voice command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Next,
    Previous,
    Pause,
    Play,
    PlayPause,
    VolumeUp,
    VolumeDown,
    /// Active noise cancellation on the headset, through stealthd.
    Anc(bool),
}

impl Command {
    /// What was done, for the pop-up.
    pub fn done(self) -> &'static str {
        match self {
            Command::Next => "Skipped",
            Command::Previous => "Previous track",
            Command::Pause => "Paused",
            Command::Play => "Resumed",
            Command::PlayPause => "Play/pause",
            Command::VolumeUp => "Volume up",
            Command::VolumeDown => "Volume down",
            Command::Anc(true) => "Noise cancellation on",
            Command::Anc(false) => "Noise cancellation off",
        }
    }
}

/// How the speech model writes "Stealth", seen and likely.
const WAKE: &[&str] = &["stealth", "stelth", "stealths", "stealthy", "steelth", "stealf"];
/// What it also writes for "Stealth" (on a noisy laptop microphone, "Third, skip this song" every time), but which
/// are ordinary words: they count only as the first word.
const WAKE_FIRST: &[&str] = &["third", "steal"];

/// The words of a sentence: lowercase, without punctuation.
fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .map(|w| w.trim_matches('\''))
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// The command in a sentence such as "Stealth, skip this song.", or `None` without the wake word or a command
/// after it.
pub fn parse(text: &str) -> Option<Command> {
    let words = words(text);
    let wake = match words.first() {
        Some(first) if WAKE_FIRST.contains(&first.as_str()) => 0,
        _ => words.iter().position(|w| WAKE.contains(&w.as_str()))?,
    };
    let rest = &words[wake + 1..];
    let has = |w: &str| rest.iter().any(|x| x == w);
    let phrase = |p: &str| rest.join(" ").contains(p);

    if has("volume") || has("louder") || has("quieter") || has("softer") || phrase("turn it") {
        if has("up") || has("louder") || has("raise") || has("increase") {
            return Some(Command::VolumeUp);
        }
        if has("down") || has("quieter") || has("softer") || has("lower") || has("decrease") {
            return Some(Command::VolumeDown);
        }
        return None;
    }
    if phrase("noise cancel") || has("anc") {
        if has("off") || has("disable") || has("stop") {
            return Some(Command::Anc(false));
        }
        if has("on") || has("enable") || has("start") {
            return Some(Command::Anc(true));
        }
        return None;
    }
    if has("previous") || has("back") || has("rewind") || phrase("last song") || phrase("last track") {
        return Some(Command::Previous);
    }
    if has("skip") || has("next") {
        return Some(Command::Next);
    }
    let pause = has("pause") || has("stop") || has("hold");
    let play = has("resume") || has("play") || has("continue") || has("unpause") || has("start");
    match (pause, play) {
        (true, true) => Some(Command::PlayPause),
        (true, false) => Some(Command::Pause),
        (false, true) => Some(Command::Play),
        _ if has("toggle") => Some(Command::PlayPause),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_commands() {
        assert_eq!(parse("Stealth, skip this song."), Some(Command::Next));
        assert_eq!(parse(" stealth next track"), Some(Command::Next));
        assert_eq!(parse("Stealth, pause this video."), Some(Command::Pause));
        assert_eq!(parse("Stealth, stop."), Some(Command::Pause));
        assert_eq!(parse("Stealth, resume this video!"), Some(Command::Play));
        assert_eq!(parse("Stealth, play."), Some(Command::Play));
        assert_eq!(parse("Stealth, pause/resume."), Some(Command::PlayPause));
        assert_eq!(parse("Stealth, go back."), Some(Command::Previous));
        assert_eq!(parse("Stealth, skip back."), Some(Command::Previous));
        assert_eq!(parse("Stealth, previous song."), Some(Command::Previous));
    }

    #[test]
    fn volume_and_headset() {
        assert_eq!(parse("Stealth, volume up."), Some(Command::VolumeUp));
        assert_eq!(parse("Stealth, turn it down"), Some(Command::VolumeDown));
        assert_eq!(parse("Stealth, louder"), Some(Command::VolumeUp));
        assert_eq!(parse("Stealth, noise cancelling on."), Some(Command::Anc(true)));
        assert_eq!(parse("Stealth, turn off noise cancellation"), Some(Command::Anc(false)));
        assert_eq!(parse("Stealth, volume"), None);
    }

    #[test]
    fn needs_the_wake_word_first() {
        assert_eq!(parse("skip this song"), None);
        assert_eq!(parse("pause the video, stealth"), None);
        assert_eq!(parse("Stealth."), None);
        assert_eq!(parse("Stealth, what's the weather?"), None);
        assert_eq!(parse("Stelth, next."), Some(Command::Next));
        assert_eq!(parse("Third, skip this song."), Some(Command::Next));
        assert_eq!(parse("Play the third song, then skip it"), None);
    }
}
