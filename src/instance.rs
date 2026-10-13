use crate::util::app_dir;
use std::{
    fs,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, SystemTime},
};

#[derive(PartialEq, Eq, Debug)]
pub enum Role {
    Primary,
    Secondary,
}

/// Zámek drží první instance po celou dobu běhu. Systém ho sám uvolní i při pádu,
/// takže nepotřebujeme žádný "tep" (dřív se soubor přepisoval každou sekundu).
static LOCK: Mutex<Option<fs::File>> = Mutex::new(None);

fn lock_path() -> PathBuf {
    app_dir().join("instance.lock")
}
fn inbox() -> PathBuf {
    let d = app_dir().join("inbox");
    let _ = fs::create_dir_all(&d);
    d
}

/// Zjistí, jestli už jiná instance běží. První se stane `Primary` (drží zámek souboru).
pub fn acquire() -> Role {
    let f = match fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(lock_path()) {
        Ok(f) => f,
        Err(_) => return Role::Primary, // nelze zjistit -> raději aplikaci spustit
    };
    match f.try_lock() {
        Ok(()) => {
            use std::io::Write;
            let _ = (&f).write_all(std::process::id().to_string().as_bytes());
            *LOCK.lock().unwrap() = Some(f);
            Role::Primary
        }
        Err(fs::TryLockError::WouldBlock) => Role::Secondary,
        Err(_) => Role::Primary,
    }
}

/// Při ukončení zámek uvolní.
pub fn release() {
    *LOCK.lock().unwrap() = None;
}

/// Druhá instance: zapíše cesty do schránky první instance. Prázdný seznam = jen přines okno do popředí.
pub fn hand_over(files: &[PathBuf]) -> PathBuf {
    let name = format!("{}-{}.txt", SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0), std::process::id());
    let body = if files.is_empty() { "@show".to_string() } else { files.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>().join("\n") };
    let tmp = inbox().join(format!("{name}.tmp"));
    let target = inbox().join(&name);
    if fs::write(&tmp, body).is_ok() {
        let _ = fs::rename(&tmp, &target);
    }
    crate::tray::show_window();
    target
}

/// Čeká, až první instance zprávu vyzvedne. Když ne (běžela jen "mrtvý" zámek po pádu), vrátí false.
pub fn wait_consumed(msg: &std::path::Path, max: Duration) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < max {
        if !msg.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = fs::remove_file(msg);
    false
}

/// Vlákno první instance: vyzvedává předané soubory i když je okno skryté (tray).
pub fn spawn_inbox_watcher(on_msg: impl Fn(Vec<PathBuf>, bool) + Send + 'static) {
    std::thread::spawn(move || loop {
        let (files, show) = take_inbox();
        if !files.is_empty() || show {
            on_msg(files, show);
        }
        std::thread::sleep(Duration::from_millis(600));
    });
}

/// První instance: vyzvedne předané cesty. Vrací (soubory, požadavek na zobrazení okna).
pub fn take_inbox() -> (Vec<PathBuf>, bool) {
    let mut files = Vec::new();
    let mut show = false;
    let Ok(rd) = fs::read_dir(inbox()) else { return (files, show) };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().map(|x| x == "txt").unwrap_or(false) {
            if let Ok(t) = fs::read_to_string(&p) {
                for l in t.lines() {
                    if l == "@show" {
                        show = true;
                    } else if !l.trim().is_empty() {
                        files.push(PathBuf::from(l));
                        show = true;
                    }
                }
            }
            let _ = fs::remove_file(&p);
        }
    }
    (files, show)
}
