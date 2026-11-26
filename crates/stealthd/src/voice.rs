//! Voice commands (experimental): runs `stealth-voice` while they are on, restarts it if it stops, and passes on
//! what it says it is doing.

use std::io::{BufRead, BufReader};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

const PROGRAM: &str = "stealth-voice";
const RESTART_AFTER: Duration = Duration::from_secs(5);

/// Whether voice commands are on, and the microphone to listen on ("" for the system's default).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    pub on: bool,
    pub microphone: String,
}

/// Changes the voice commands' settings.
#[derive(Clone)]
pub struct Switch(mpsc::Sender<Config>);

impl Switch {
    pub fn set(&self, config: Config) {
        let _ = self.0.send(config);
    }
}

pub fn channel() -> (Switch, mpsc::Receiver<Config>) {
    let (tx, rx) = mpsc::channel();
    (Switch(tx), rx)
}

/// `stealth-voice` next to this program, or on `PATH`.
fn find_program() -> Option<PathBuf> {
    let beside = std::env::current_exe().ok()?.parent()?.join(PROGRAM);
    if beside.is_file() {
        return Some(beside);
    }
    std::env::split_paths(&std::env::var_os("PATH")?).map(|dir| dir.join(PROGRAM)).find(|p| p.is_file())
}

fn spawn(program: &PathBuf, microphone: &str) -> std::io::Result<Child> {
    let mut command = Command::new(program);
    if !microphone.is_empty() {
        command.args(["--microphone", microphone]);
    }
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::inherit());
    // stop with stealthd, however it ends
    unsafe {
        command.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
    command.spawn()
}

/// The latest settings that came in within `wait`, or `config` if none did; `None` once stealthd is shutting down.
fn latest(rx: &mpsc::Receiver<Config>, mut config: Config, wait: Duration) -> Option<Config> {
    match rx.recv_timeout(wait) {
        Ok(v) => config = v,
        Err(RecvTimeoutError::Timeout) => return Some(config),
        Err(RecvTimeoutError::Disconnected) => return None,
    }
    loop {
        match rx.try_recv() {
            Ok(v) => config = v,
            Err(mpsc::TryRecvError::Empty) => return Some(config),
            Err(mpsc::TryRecvError::Disconnected) => return None,
        }
    }
}

/// Run until stealthd ends; `report` gets the status shown in the apps.
pub fn run(rx: mpsc::Receiver<Config>, mut config: Config, report: impl Fn(&str) + Send + Sync + Clone + 'static) {
    loop {
        if !config.on {
            report("off");
            match rx.recv() {
                Ok(v) => config = v,
                Err(_) => return,
            }
            continue;
        }
        let Some(program) = find_program() else {
            report("stealth-voice is not installed: run ./install.sh with cmake and a C++ compiler present");
            match rx.recv() {
                Ok(v) => config = v,
                Err(_) => return,
            }
            continue;
        };
        report("starting");
        let running = config.clone();
        let mut child = match spawn(&program, &running.microphone) {
            Ok(child) => child,
            Err(e) => {
                report(&format!("cannot start stealth-voice: {e}"));
                match latest(&rx, config, RESTART_AFTER) {
                    Some(v) => config = v,
                    None => return,
                }
                continue;
            }
        };
        let reader = child.stdout.take().map(|out| {
            let report = report.clone();
            thread::spawn(move || {
                for line in BufReader::new(out).lines().map_while(Result::ok) {
                    match line.strip_prefix("status: ") {
                        Some(status) => report(status),
                        None => eprintln!("stealth-voice: {line}"),
                    }
                }
            })
        });
        let ended = loop {
            match latest(&rx, config.clone(), Duration::from_millis(250)) {
                Some(v) => config = v,
                None => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return;
                }
            }
            // off, or another microphone: stop it (and start it again on the new one)
            if config != running {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            if let Ok(Some(status)) = child.try_wait() {
                break Some(status);
            }
        };
        // the reader ends with the program; join it so its last status comes before ours
        if let Some(reader) = reader {
            let _ = reader.join();
        }
        if let Some(status) = ended {
            eprintln!("stealthd: stealth-voice stopped ({status})");
            report("stopped; starting again shortly");
            match latest(&rx, config, RESTART_AFTER) {
                Some(v) => config = v,
                None => return,
            }
        }
    }
}
