//! IPC server and client. Newline-delimited JSON; Unix socket on Unix, loopback TCP with a token
//! file on Windows. One thread per connection; long operations hold the engine lock so they are
//! serialised with the daemon loop.

use std::io::{BufRead, BufReader, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;

use anyhow::{Context, Result};
use diskgc_core::model::{Plan, Progress};
use diskgc_core::protocol::{Event, Hello, Request, RequestBody, Response, VERSION};
use diskgc_core::{paths, Engine};

type Subscribers = Arc<Mutex<Vec<Sender<String>>>>;

pub struct Server {
    subscribers: Subscribers,
    stop: Arc<AtomicBool>,
}

impl Server {
    pub fn start(
        engine: Arc<RwLock<Engine>>,
        stop: Arc<AtomicBool>,
        busy: Arc<AtomicBool>,
        run_lock: Arc<Mutex<()>>,
    ) -> Result<Server> {
        let subscribers: Subscribers = Arc::new(Mutex::new(Vec::new()));
        let listener = transport::bind()?;
        let subs = subscribers.clone();
        let stop2 = stop.clone();
        thread::Builder::new()
            .name("diskgc-ipc".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    if stop2.load(Ordering::Relaxed) {
                        break;
                    }
                    let Ok(stream) = stream else { continue };
                    let engine = engine.clone();
                    let subs = subs.clone();
                    let busy = busy.clone();
                    let stop = stop2.clone();
                    let run_lock = run_lock.clone();
                    thread::spawn(move || {
                        if let Err(e) = handle(stream, engine, subs, busy, stop, run_lock) {
                            tracing::debug!("ipc connection ended: {e}");
                        }
                    });
                }
            })?;
        Ok(Server { subscribers, stop })
    }

    /// A sink the daemon loop uses to fan events out to subscribed clients.
    pub fn broadcaster(&self) -> diskgc_core::daemon::EventSink {
        let subs = self.subscribers.clone();
        Arc::new(move |ev: Event| broadcast(&subs, &ev))
    }

    pub fn shutdown(&self) {
        self.stop.store(true, Ordering::Relaxed);
        transport::cleanup();
    }
}

fn broadcast(subs: &Subscribers, ev: &Event) {
    let Ok(line) = serde_json::to_string(ev) else {
        return;
    };
    let mut s = subs.lock().unwrap();
    s.retain(|tx| tx.send(line.clone()).is_ok());
}

fn handle(
    stream: transport::Stream,
    engine: Arc<RwLock<Engine>>,
    subs: Subscribers,
    busy: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    run_lock: Arc<Mutex<()>>,
) -> Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let (tx, rx) = channel::<String>();
    let mut writer = stream;
    thread::spawn(move || {
        for line in rx {
            if writer
                .write_all(line.as_bytes())
                .and_then(|_| writer.write_all(b"\n"))
                .is_err()
            {
                break;
            }
        }
    });
    transport::authenticate(&mut reader)?;
    let mut cached_plan: Option<Plan> = None;
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let req: Request = match serde_json::from_str(line.trim()) {
            Ok(r) => r,
            Err(e) => {
                let _ = tx.send(serde_json::to_string(&Response::error(
                    0,
                    format!("bad request: {e}"),
                ))?);
                continue;
            }
        };
        let id = req.id;
        let progress_tx = tx.clone();
        let progress = |p: Progress| {
            if let Ok(s) = serde_json::to_string(&Event::Progress(p)) {
                let _ = progress_tx.send(s);
            }
        };
        let resp = match req.body {
            RequestBody::Hello => Response::ok(
                id,
                &Hello {
                    protocol: VERSION,
                    daemon_version: env!("CARGO_PKG_VERSION").into(),
                    pid: std::process::id(),
                },
            ),
            RequestBody::Ping => Response::ok(id, &"pong"),
            RequestBody::Status => Response::ok(id, &engine.read().unwrap().status()),
            RequestBody::Report { refresh } => {
                let _run = run_lock.lock().unwrap();
                let eng = engine.read().unwrap();
                let step = |name: &str| {
                    progress(Progress::Scanning {
                        rule: name.to_string(),
                    })
                };
                let r = if refresh {
                    eng.report(step)
                } else {
                    eng.report_cached(step)
                };
                Response::ok(id, &r)
            }
            RequestBody::Plan { level } => {
                let _run = run_lock.lock().unwrap();
                let eng = engine.read().unwrap();
                let level = level.unwrap_or_else(|| eng.auto_level());
                let plan = eng.plan(level, progress);
                let _ = tx.send(serde_json::to_string(&Event::PlanReady(plan.clone()))?);
                cached_plan = Some(plan.clone());
                Response::ok(id, &plan)
            }
            RequestBody::Execute {
                action_ids,
                dry_run,
            } => match &cached_plan {
                None => Response::error(id, "no plan on this connection; call `plan` first"),
                Some(plan) => {
                    let _run = run_lock.lock().unwrap();
                    busy.store(true, Ordering::Relaxed);
                    let summary = engine.read().unwrap().execute(
                        plan,
                        action_ids.as_deref(),
                        dry_run,
                        progress,
                    );
                    busy.store(false, Ordering::Relaxed);
                    broadcast(&subs, &Event::RunFinished(summary.clone()));
                    Response::ok(id, &summary)
                }
            },
            RequestBody::Clean { level, dry_run } => {
                let _run = run_lock.lock().unwrap();
                busy.store(true, Ordering::Relaxed);
                let eng = engine.read().unwrap();
                let level = level.unwrap_or_else(|| eng.auto_level());
                let (plan, summary) = eng.clean(level, dry_run, progress);
                busy.store(false, Ordering::Relaxed);
                broadcast(&subs, &Event::RunFinished(summary.clone()));
                Response::ok(id, &serde_json::json!({ "plan": plan, "summary": summary }))
            }
            RequestBody::Stats => Response::ok(id, &engine.read().unwrap().stats()),
            RequestBody::History { limit } => {
                Response::ok(id, &engine.read().unwrap().history().runs(limit))
            }
            RequestBody::Memory => Response::ok(id, &engine.read().unwrap().memory()),
            RequestBody::KillOrphans => Response::ok(id, &engine.read().unwrap().kill_orphans()),
            RequestBody::GetConfig => match engine.read().unwrap().config_toml() {
                Ok(t) => Response::ok(id, &serde_json::json!({ "toml": t })),
                Err(e) => Response::error(id, e.to_string()),
            },
            RequestBody::SetConfig { toml } => {
                match engine.write().unwrap().replace_config(&toml) {
                    Ok(()) => Response::ok(id, &"saved"),
                    Err(e) => Response::error(id, format!("{e:#}")),
                }
            }
            RequestBody::Subscribe => {
                subs.lock().unwrap().push(tx.clone());
                Response::ok(id, &"subscribed")
            }
            RequestBody::Shutdown => {
                stop.store(true, Ordering::Relaxed);
                Response::ok(id, &"stopping")
            }
        };
        tx.send(serde_json::to_string(&resp)?)?;
    }
}

/// Minimal client used by the CLI (`ui` launcher) and by tests.
#[allow(dead_code)]
pub struct Client {
    reader: BufReader<transport::Stream>,
    writer: transport::Stream,
    next_id: u64,
}

impl Client {
    pub fn connect() -> Result<Client> {
        let stream = transport::connect()?;
        let mut writer = stream.try_clone()?;
        transport::send_auth(&mut writer)?;
        Ok(Client {
            reader: BufReader::new(stream),
            writer,
            next_id: 1,
        })
    }

    #[allow(dead_code)]
    pub fn call(&mut self, body: RequestBody) -> Result<Response> {
        let id = self.next_id;
        self.next_id += 1;
        let req = Request { id, body };
        self.writer
            .write_all(serde_json::to_string(&req)?.as_bytes())?;
        self.writer.write_all(b"\n")?;
        let mut line = String::new();
        loop {
            line.clear();
            anyhow::ensure!(self.reader.read_line(&mut line)? > 0, "connection closed");
            // Skip events interleaved with the response.
            if let Ok(resp) = serde_json::from_str::<Response>(line.trim()) {
                if resp.id == id {
                    return Ok(resp);
                }
            }
        }
    }
}

#[cfg(unix)]
mod transport {
    use super::*;
    use std::os::unix::net::{UnixListener, UnixStream};

    pub type Stream = UnixStream;
    pub type Listener = UnixListener;

    pub fn bind() -> Result<Listener> {
        let path = paths::ipc_endpoint();
        std::fs::create_dir_all(path.parent().unwrap())?;
        if path.exists() {
            if UnixStream::connect(&path).is_ok() {
                anyhow::bail!(
                    "another diskgc daemon is already listening on {}",
                    path.display()
                );
            }
            std::fs::remove_file(&path)?;
        }
        let l = UnixListener::bind(&path).with_context(|| format!("binding {}", path.display()))?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        Ok(l)
    }

    pub fn connect() -> Result<Stream> {
        Ok(UnixStream::connect(paths::ipc_endpoint())?)
    }

    pub fn cleanup() {
        let _ = std::fs::remove_file(paths::ipc_endpoint());
    }

    pub fn authenticate(_r: &mut BufReader<Stream>) -> Result<()> {
        Ok(()) // the socket file is mode 0600: the filesystem is the authentication
    }

    pub fn send_auth(_w: &mut Stream) -> Result<()> {
        Ok(())
    }
}

#[cfg(windows)]
mod transport {
    use super::*;
    use std::net::{TcpListener, TcpStream};

    pub type Stream = TcpStream;
    pub type Listener = TcpListener;

    fn token_file() -> std::path::PathBuf {
        paths::ipc_endpoint()
    }

    fn random_token() -> String {
        let mut t = String::new();
        for _ in 0..4 {
            let n = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
                ^ (std::process::id() as u128) << 64;
            t.push_str(&format!("{n:x}"));
            std::thread::sleep(std::time::Duration::from_micros(3));
        }
        t
    }

    pub fn bind() -> Result<Listener> {
        let l = TcpListener::bind("127.0.0.1:0")?;
        let port = l.local_addr()?.port();
        let token = random_token();
        let f = token_file();
        std::fs::create_dir_all(f.parent().unwrap())?;
        std::fs::write(&f, format!("{port}\n{token}\n"))?;
        Ok(l)
    }

    fn read_endpoint() -> Result<(u16, String)> {
        let text = std::fs::read_to_string(token_file())
            .context("daemon not running (no endpoint file)")?;
        let mut it = text.lines();
        let port: u16 = it.next().unwrap_or("").trim().parse()?;
        let token = it.next().unwrap_or("").trim().to_string();
        Ok((port, token))
    }

    pub fn connect() -> Result<Stream> {
        let (port, _) = read_endpoint()?;
        Ok(TcpStream::connect(("127.0.0.1", port))?)
    }

    pub fn cleanup() {
        let _ = std::fs::remove_file(token_file());
    }

    pub fn authenticate(r: &mut BufReader<Stream>) -> Result<()> {
        let (_, token) = read_endpoint()?;
        let mut line = String::new();
        r.read_line(&mut line)?;
        anyhow::ensure!(line.trim() == token, "bad token");
        Ok(())
    }

    pub fn send_auth(w: &mut Stream) -> Result<()> {
        let (_, token) = read_endpoint()?;
        w.write_all(format!("{token}\n").as_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diskgc_core::config::Config;
    use diskgc_core::protocol::ResponseBody;

    #[test]
    fn server_answers_hello_status_and_plan() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("DISKGC_RUNTIME_DIR", dir.path());
        std::env::set_var("DISKGC_DATA_DIR", dir.path());
        let mut cfg = Config::default();
        cfg.scan.roots = vec![dir.path().join("projects").display().to_string()];
        cfg.scan.extra_target_roots = vec![];
        cfg.docker.enable = false;
        cfg.compress.enable = false;
        let engine = Arc::new(RwLock::new(Engine::with_config(cfg)));
        let stop = Arc::new(AtomicBool::new(false));
        let busy = Arc::new(AtomicBool::new(false));
        let server = Server::start(engine, stop, busy, Arc::new(Mutex::new(()))).unwrap();

        let mut c = Client::connect().unwrap();
        let r = c.call(RequestBody::Hello).unwrap();
        match r.body {
            ResponseBody::Ok(v) => assert_eq!(v["protocol"], VERSION),
            ResponseBody::Error { message } => panic!("{message}"),
        }
        let r = c
            .call(RequestBody::Execute {
                action_ids: None,
                dry_run: true,
            })
            .unwrap();
        assert!(
            matches!(r.body, ResponseBody::Error { .. }),
            "execute without plan must fail"
        );
        let r = c.call(RequestBody::Plan { level: None }).unwrap();
        assert!(matches!(r.body, ResponseBody::Ok(_)));
        let r = c
            .call(RequestBody::Execute {
                action_ids: Some(vec![]),
                dry_run: true,
            })
            .unwrap();
        match r.body {
            ResponseBody::Ok(v) => assert_eq!(v["action_count"], 0),
            ResponseBody::Error { message } => panic!("{message}"),
        }
        let r = c.call(RequestBody::Status).unwrap();
        assert!(matches!(r.body, ResponseBody::Ok(_)));
        server.shutdown();
    }
}
