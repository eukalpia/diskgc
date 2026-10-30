//! `diskgc` — command line, daemon and IPC server. All the logic lives in `diskgc-core`.

mod ipc;
mod logging;
mod output;

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use diskgc_core::model::{Level, Progress};
use diskgc_core::{paths, platform, Engine};

#[derive(Parser)]
#[command(name = "diskgc", version, about = "Smart background disk cleaner for developer machines", long_about = None)]
struct Cli {
    /// Machine-readable JSON output instead of text.
    #[arg(long, global = true)]
    json: bool,
    /// Configuration file (default: platform config dir).
    #[arg(long, global = true, value_name = "FILE")]
    config: Option<PathBuf>,
    /// Verbose logging to stderr.
    #[arg(short, long, global = true)]
    verbose: bool,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// What occupies the disk: categories and the biggest offenders.
    Report,
    /// Show what a clean would do, with the reason for every action. Nothing is touched.
    Plan {
        /// normal | low | critical (default: derived from free space)
        #[arg(long)]
        level: Option<Level>,
    },
    /// Clean now.
    Clean {
        /// Print the plan and exit without touching anything (same as `plan`).
        #[arg(short = 'n', long)]
        dry_run: bool,
        #[arg(long)]
        level: Option<Level>,
    },
    /// Run the daemon in the foreground (autostart uses this).
    Daemon,
    /// Daemon state, free space, forecast, last run.
    Status,
    /// Growth per day, category trends, repeat offenders.
    Stats,
    /// Recent runs and what they removed.
    History {
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Memory: who uses RAM, orphaned helper processes.
    Mem {
        /// Terminate orphaned helper processes (SIGTERM).
        #[arg(long)]
        kill_orphans: bool,
        /// Terminate automation browsers (Playwright, headless shells) older than one hour.
        #[arg(long)]
        kill_browsers: bool,
    },
    /// Register the daemon to start at login and start it now.
    Install,
    /// Stop the daemon and remove it from autostart.
    Uninstall,
    /// Print the config path, or open the file in $EDITOR with `--edit`.
    Config {
        #[arg(long)]
        edit: bool,
        /// Print the effective configuration.
        #[arg(long)]
        show: bool,
    },
    /// Open the terminal dashboard (diskgc-ui), starting the daemon if needed.
    Ui,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    let cli = Cli::parse();
    let config_path = cli.config.clone().unwrap_or_else(paths::config_file);
    let cmd = cli.cmd.unwrap_or(Cmd::Status);
    let is_daemon = matches!(cmd, Cmd::Daemon);
    logging::init(cli.verbose, is_daemon)?;
    let engine = Engine::load_from(config_path).context("loading configuration")?;
    let json = cli.json;

    match cmd {
        Cmd::Report => {
            let r = engine.report(|step| {
                if !json {
                    eprint!("\rscanning {step:<24}")
                }
            });
            if !json {
                eprint!("\r{:40}\r", "");
            }
            output::report(&r, json);
        }
        Cmd::Plan { level } => {
            let level = level.unwrap_or_else(|| engine.auto_level());
            let plan = engine.plan(level, |p| progress_line(&p, json));
            output::plan(&plan, json);
        }
        Cmd::Clean { dry_run, level } => {
            let level = level.unwrap_or_else(|| engine.auto_level());
            let (plan, summary) = engine.clean(level, dry_run, |p| progress_line(&p, json));
            output::run(&plan, &summary, json);
        }
        Cmd::Daemon => daemon(engine)?,
        Cmd::Status => output::status(&engine.status(), json),
        Cmd::Stats => output::stats(&engine.stats(), json),
        Cmd::History { limit } => output::history(&engine.history().runs(limit), json),
        Cmd::Mem {
            kill_orphans,
            kill_browsers,
        } => {
            let rep = engine.memory();
            output::memory(&rep, json);
            if kill_orphans {
                let killed = engine.kill_orphans();
                if json {
                    println!("{}", serde_json::json!({ "killed": killed }));
                } else {
                    println!("terminated {} orphaned process(es)", killed.len());
                }
            }
            if kill_browsers {
                let killed = engine.kill_automation_browsers(1.0);
                if json {
                    println!("{}", serde_json::json!({ "killed_browsers": killed }));
                } else {
                    println!("terminated {} automation browser(s)", killed.len());
                }
            }
        }
        Cmd::Install => {
            let exe = std::env::current_exe()?.canonicalize()?;
            platform::autostart_install(&exe).context("registering autostart")?;
            let st = platform::autostart_status();
            if json {
                println!("{}", serde_json::to_string_pretty(&st)?);
            } else {
                println!(
                    "daemon installed and started ({}). log: {}",
                    st.detail,
                    paths::log_file().display()
                );
            }
        }
        Cmd::Uninstall => {
            platform::autostart_uninstall()?;
            if !json {
                println!("daemon stopped and removed from autostart");
            }
        }
        Cmd::Config { edit, show } => {
            let p = engine.config_path();
            if show {
                print!("{}", engine.config_toml()?);
            } else if edit {
                let editor = std::env::var("VISUAL")
                    .or_else(|_| std::env::var("EDITOR"))
                    .unwrap_or_else(|_| {
                        if cfg!(windows) {
                            "notepad".into()
                        } else {
                            "vi".into()
                        }
                    });
                std::process::Command::new(editor).arg(p).status()?;
            } else if json {
                println!("{}", serde_json::json!({ "path": p }));
            } else {
                println!("{}", p.display());
            }
        }
        Cmd::Ui => launch_ui()?,
    }
    Ok(())
}

fn progress_line(p: &Progress, json: bool) {
    if json {
        return;
    }
    match p {
        Progress::Scanning { rule } => eprint!("\rscanning {rule:<28}"),
        Progress::Planned { .. } => eprint!("\r{:40}\r", ""),
        Progress::Executing {
            index,
            total,
            action,
        } => eprintln!(
            "[{index}/{total}] {} {}",
            action.kind.verb(),
            action.path.display()
        ),
        _ => {}
    }
}

fn daemon(engine: Engine) -> Result<()> {
    let engine = Arc::new(RwLock::new(engine));
    let run_lock = Arc::new(Mutex::new(()));
    let stop = Arc::new(AtomicBool::new(false));
    let busy = Arc::new(AtomicBool::new(false));
    let server = ipc::Server::start(engine.clone(), stop.clone(), busy.clone(), run_lock.clone())
        .context("starting IPC server")?;
    let events = server.broadcaster();
    {
        let stop = stop.clone();
        ctrlc_handler(move || stop.store(true, std::sync::atomic::Ordering::Relaxed));
    }
    let d = diskgc_core::daemon::Daemon {
        engine,
        events,
        stop,
        busy,
        run_lock,
    };
    d.run();
    server.shutdown();
    Ok(())
}

#[cfg(unix)]
fn ctrlc_handler(f: impl Fn() + Send + Sync + 'static) {
    // A tiny signal thread avoids pulling in a signal-handling crate.
    use std::sync::OnceLock;
    static HANDLER: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();
    let _ = HANDLER.set(Box::new(f));
    extern "C" fn on_signal(_: libc::c_int) {
        if let Some(h) = HANDLER.get() {
            h();
        }
    }
    unsafe {
        let handler = on_signal as extern "C" fn(libc::c_int) as *const () as usize;
        libc::signal(libc::SIGINT, handler);
        libc::signal(libc::SIGTERM, handler);
    }
}

#[cfg(windows)]
fn ctrlc_handler(_f: impl Fn() + Send + Sync + 'static) {}

fn launch_ui() -> Result<()> {
    let exe = std::env::current_exe()?;
    let candidates = [exe.with_file_name(if cfg!(windows) {
        "diskgc-ui.exe"
    } else {
        "diskgc-ui"
    })];
    let ui = candidates
        .iter()
        .find(|p| p.is_file())
        .cloned()
        .or_else(|| platform::which("diskgc-ui").then(|| PathBuf::from("diskgc-ui")));
    let Some(ui) = ui else {
        anyhow::bail!("diskgc-ui is not installed next to diskgc or on PATH. Build it from ui/ (see README) or use the CLI commands.");
    };
    if ipc::Client::connect().is_err() {
        // No daemon: start one detached so the UI has something to talk to.
        let mut cmd = std::process::Command::new(&exe);
        cmd.arg("daemon")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        cmd.spawn().context("starting daemon")?;
        for _ in 0..20 {
            std::thread::sleep(std::time::Duration::from_millis(150));
            if ipc::Client::connect().is_ok() {
                break;
            }
        }
    }
    let status = std::process::Command::new(ui)
        .status()
        .context("running diskgc-ui")?;
    std::process::exit(status.code().unwrap_or(1));
}
