//! Hlavní aplikace: stav, fronta úloh, navigace, rámec okna.
use crate::{
    convert, ffmpeg,
    formats::*,
    hw::SysInfo,
    job::*,
    opts::*,
    probe::Probe,
    stats::{HistoryEntry, Stats},
    theme::*,
    tools::Tools,
    ui_info::InfoPage,
    ui_pdf::PdfPage,
    ui_tools::{AudioPage, TracksPage},
    util::*,
};
use egui::{vec2, Color32, CornerRadius, Margin, RichText};
use egui_phosphor::regular as ph;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc, Arc, Condvar, Mutex,
    },
    thread,
    time::Instant,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Upload,
    Queue,
    Tools,
    Info,
    Stats,
    Settings,
    About,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToolTab {
    Pdf,
    Audio,
    Tracks,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub dark: bool,
    pub same_dir: bool,
    pub out_dir: String,
    pub hw: HwMode,
    pub hw_decode: bool,
    pub parallel: usize,
    pub tray: bool,
    pub shortcut_done: bool,
    pub lang: String,
    pub accent: [u8; 3],
    pub advanced: bool,
    pub autostart: bool,
    pub start_hidden: bool,
    pub check_updates: bool,
    pub avx512: bool,
    pub context_menu: bool,
    pub open_with: bool,
    /// Proběhl úvodní průvodce nastavením?
    pub setup_done: bool,
    /// Aktualizace se po nalezení samy stáhnou a nainstalují (až když appka nic nepřevádí).
    pub auto_update: bool,
    /// Verze formátu nastavení (kvůli jednorázovým migracím).
    #[serde(default)]
    pub ver: u32,
}

const SETTINGS_VER: u32 = 2;

impl Default for Settings {
    fn default() -> Self {
        Settings { dark: true, same_dir: true, out_dir: String::new(), hw: HwMode::Auto, hw_decode: true, parallel: 0,
            tray: true,
            shortcut_done: false,
            lang: "en".into(),
            accent: [168, 85, 247],
            advanced: false,
            autostart: false,
            start_hidden: true,
            check_updates: true,
            avx512: true,
            context_menu: false,
            open_with: false,
            setup_done: false,
            auto_update: true,
            ver: SETTINGS_VER,
        }
    }
}

impl Settings {
    fn path() -> PathBuf {
        app_dir().join("settings.json")
    }
    pub fn load() -> Settings {
        let mut s: Settings = std::fs::read_to_string(Self::path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        if s.ver < 2 {
            // do 2.1 byla výchozí hodnota napevno 2 souběžné převody; nově "Auto" (podle jader procesoru)
            s.parallel = 0;
            s.ver = SETTINGS_VER;
        }
        s
    }
    pub fn save(&self) {
        if let Ok(t) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(Self::path(), t);
        }
    }
}

pub enum Bg {
    Ready(Tools, SysInfo),
    BootNote(String),
    Probe(PathBuf, Result<Probe, String>),
    Pages(PathBuf, Option<u32>),
    Done(u64, Res<Outcome>, f64),
    Shortcut(Result<PathBuf, String>),
    Update(Result<Option<crate::update::UpdateInfo>, String>),
    UpdateProgress(f32),
    /// Pravidelná (každých pár hodin) kontrola aktualizací, i když appka dlouho leží v tray.
    UpdateTick,
    UpdateDownloaded(Result<PathBuf, String>),
    Inbox(Vec<PathBuf>, bool),
}

#[derive(Clone)]
pub struct TaskMeta {
    pub name: String,
    pub from: String,
    pub to: String,
    pub kind: String,
}

pub enum TaskState {
    Running,
    Done(Outcome, f64),
    Failed(String),
    Cancelled,
}

pub struct Task {
    pub shared: Arc<Shared>,
    pub meta: TaskMeta,
    pub state: TaskState,
}

impl Task {
    pub fn running(&self) -> bool {
        matches!(self.state, TaskState::Running)
    }
    pub fn elapsed(&self) -> f64 {
        self.shared.started().map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0)
    }
}

/// Plánovač úloh. V režimu Auto má každá úloha váhu podle toho, kolik procesoru si vezme
/// (obrázek/zvuk = 1, video na GPU = 3, video na CPU = polovina vláken) a běží tolik úloh,
/// kolik se jich vejde do počtu vláken. Dřív to byly napevno 2 úlohy, takže spousta rychlých převodů
/// využívala jen pár procent CPU.
struct Slots {
    used: Mutex<usize>,
    cv: Condvar,
    cap: Arc<AtomicUsize>,
    auto: Arc<AtomicBool>,
}

impl Slots {
    fn weight(&self, w: usize) -> usize {
        if self.auto.load(Ordering::Relaxed) {
            w.clamp(1, self.cap.load(Ordering::Relaxed).max(1))
        } else {
            1
        }
    }
    /// Počká na volné místo; vrací skutečně zabranou váhu (nebo `None`, když byla úloha zrušena).
    fn acquire(&self, shared: &Shared, weight: usize) -> Option<usize> {
        let mut g = self.used.lock().unwrap();
        loop {
            if shared.cancelled() {
                return None;
            }
            let w = self.weight(weight);
            if *g == 0 || *g + w <= self.cap.load(Ordering::Relaxed).max(1) {
                *g += w;
                return Some(w);
            }
            g = self.cv.wait_timeout(g, std::time::Duration::from_millis(200)).unwrap().0;
        }
    }
    fn is_idle(&self) -> bool {
        *self.used.lock().unwrap() == 0
    }
    fn release(&self, w: usize) {
        let mut g = self.used.lock().unwrap();
        *g = g.saturating_sub(w);
        self.cv.notify_all();
    }
}

pub struct Item {
    pub id: u64,
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub kind: Kind,
    pub in_ext: String,
    pub targets: Vec<Fmt>,
    pub sel: usize,
    pub opts: Opts,
    pub out_dir: Option<PathBuf>,
    pub open: bool,
    pub task: Option<u64>,
    pub probe: Option<Probe>,
    pub filter: String,
}

impl Item {
    pub fn fmt(&self) -> Option<&Fmt> {
        self.targets.get(self.sel)
    }
}

pub struct App {
    pub egui: egui::Context,
    pub tab: Tab,
    pub tool_tab: ToolTab,
    pub settings: Settings,
    pub tools: Arc<Tools>,
    pub si: Arc<SysInfo>,
    pub ready: bool,
    pub boot_note: String,
    pub rx: mpsc::Receiver<Bg>,
    pub tx: mpsc::Sender<Bg>,
    slots: Arc<Slots>,
    max_parallel: Arc<AtomicUsize>,
    auto_parallel: Arc<AtomicBool>,
    pub tasks: HashMap<u64, Task>,
    next_id: u64,
    pub items: Vec<Item>,
    pub stats: Stats,
    pub pdf: PdfPage,
    pub audio: AudioPage,
    pub tracks: TracksPage,
    pub info: InfoPage,
    pub toast: Option<(String, Instant)>,
    pub merge_name: String,
    pub merge_task: Option<u64>,
    pub batch_filter: String,
    pub update: crate::update::UpdateState,
    pending: Vec<PathBuf>,
    start_min: bool,
    pub tray: Option<crate::tray::Tray>,
    tray_rx: mpsc::Receiver<crate::tray::TrayMsg>,
    quitting: bool,
    /// Úvodní průvodce (jen při prvním spuštění).
    pub wizard: Option<crate::ui_setup::Wizard>,
    /// Lištu s novou verzí uživatel zavřel.
    pub update_dismissed: bool,
    /// Je okno vidět? (kvůli automatické aktualizaci na pozadí)
    visible: Arc<AtomicBool>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, startup: crate::Startup) -> App {
        let ctx = cc.egui_ctx.clone();
        setup_fonts(&ctx);
        let settings = Settings::load();
        crate::i18n::set_english(settings.lang != "cs");
        set_accent(settings.accent);
        apply_visuals(&ctx, settings.dark);
        let (tx, rx) = mpsc::channel();
        // příprava nástrojů a detekce HW na pozadí
        {
            let tx = tx.clone();
            let ctx = ctx.clone();
            thread::spawn(move || {
                let txn = tx.clone();
                let ctxn = ctx.clone();
                let tools = Tools::prepare(move |m| {
                    let _ = txn.send(Bg::BootNote(m.to_string()));
                    ctxn.request_repaint();
                });
                let _ = tx.send(Bg::BootNote(tr!("Detekuji CPU, GPU a AMF…", "Detecting CPU, GPU and encoders…").into()));
                ctx.request_repaint();
                let si = crate::hw::detect(&tools);
                let _ = tx.send(Bg::Ready(tools, si));
                ctx.request_repaint();
            });
        }
        {
            let tx = tx.clone();
            let ctx = ctx.clone();
            crate::instance::spawn_inbox_watcher(move |files, show| {
                if show {
                    crate::tray::show_window();
                }
                let _ = tx.send(Bg::Inbox(files, show));
                ctx.request_repaint();
            });
        }
        {
            // appka může týdny ležet v tray -> kontrola aktualizací nesmí být jen při startu
            let tx = tx.clone();
            let ctx = ctx.clone();
            thread::spawn(move || loop {
                thread::sleep(std::time::Duration::from_secs(6 * 3600));
                let _ = tx.send(Bg::UpdateTick);
                ctx.request_repaint();
            });
        }
        let (tray_tx, tray_rx) = mpsc::channel();
        let tray = if settings.tray { crate::tray::create(ctx.clone(), tray_tx) } else { None };
        let start_min = startup.hidden && !(cfg!(windows) && settings.tray);
        // stejná podmínka jako v main.rs (`with_visible(false)`): okno je opravdu schované jen s tray
        let start_hidden = startup.hidden && crate::tray::supported() && settings.tray;
        let max_parallel = Arc::new(AtomicUsize::new(settings.parallel.max(1)));
        let auto_parallel = Arc::new(AtomicBool::new(settings.parallel == 0));
        App {
            egui: ctx,
            tab: Tab::Upload,
            tool_tab: ToolTab::Pdf,
            slots: Arc::new(Slots { used: Mutex::new(0), cv: Condvar::new(), cap: max_parallel.clone(), auto: auto_parallel.clone() }),
            max_parallel,
            auto_parallel,
            settings,
            tools: Arc::new(Tools::default()),
            si: Arc::new(SysInfo::default()),
            ready: false,
            boot_note: tr!("Spouštím…", "Starting…").into(),
            rx,
            tx,
            tasks: HashMap::new(),
            next_id: 1,
            items: Vec::new(),
            stats: Stats::load(),
            pdf: PdfPage::default(),
            audio: AudioPage::default(),
            tracks: TracksPage::default(),
            info: InfoPage::default(),
            toast: None,
            merge_name: tr!("Sloučené", "Merged").into(),
            merge_task: None,
            batch_filter: String::new(),
            update: crate::update::UpdateState::Idle,
            pending: startup.files,
            start_min,
            tray,
            tray_rx,
            quitting: false,
            wizard: None,
            update_dismissed: false,
            visible: Arc::new(AtomicBool::new(!start_hidden)),
        }
    }

    pub fn max_parallel_store(&self) {
        let auto = self.settings.parallel == 0;
        self.auto_parallel.store(auto, Ordering::Relaxed);
        let cap = if auto { self.si.threads.clamp(2, 12) } else { self.settings.parallel };
        self.max_parallel.store(cap, Ordering::Relaxed);
    }

    /// Kolik "místa" úloha zabere (viz `Slots`).
    fn job_weight(&self, job: &ConvertJob) -> usize {
        match job.fmt.class {
            Class::Video => {
                if ffmpeg::pick_gpu(&self.si, &job.opts, &job.fmt.ext).is_some() {
                    3
                } else {
                    (self.si.threads / 2).max(2)
                }
            }
            Class::AnimImage => 2,
            _ => 1,
        }
    }

    pub fn apply_theme(&self) {
        set_accent(self.settings.accent);
        apply_visuals(&self.egui, self.settings.dark);
    }

    /// Zeptá se GitHubu na nejnovější vydání (jen na požádání).
    pub fn check_update(&mut self) {
        self.update = crate::update::UpdateState::Checking;
        let tx = self.tx.clone();
        let ctx = self.egui.clone();
        thread::spawn(move || {
            let _ = tx.send(Bg::Update(crate::update::check(env!("CARGO_PKG_VERSION"))));
            ctx.request_repaint();
        });
    }

    /// Stáhne (a ověří) novou verzi na pozadí.
    pub fn start_update_download(&mut self) {
        let crate::update::UpdateState::Available(info) = self.update.clone() else { return };
        if info.mode == crate::update::InstallMode::Manual || info.asset.is_none() {
            open_url(&info.url);
            return;
        }
        self.update = crate::update::UpdateState::Downloading(info.clone(), 0.0);
        let tx = self.tx.clone();
        let ctx = self.egui.clone();
        thread::spawn(move || {
            let last = std::cell::Cell::new(-1.0_f32);
            let r = crate::update::download(&info, &|f| {
                if f - last.get() >= 0.01 {
                    last.set(f);
                    let _ = tx.send(Bg::UpdateProgress(f));
                    ctx.request_repaint();
                }
            });
            let _ = tx.send(Bg::UpdateDownloaded(r));
            ctx.request_repaint();
        });
    }

    /// Nainstaluje stažený update (appka se ukončí a spustí znovu).
    pub fn apply_update(&mut self) {
        let crate::update::UpdateState::Ready(info, path) = self.update.clone() else { return };
        if self.any_running() {
            self.toast(tr!("Nejdřív nech doběhnout převody.", "Let the conversions finish first."));
            return;
        }
        self.settings.save();
        self.update = crate::update::UpdateState::Installing;
        match crate::update::apply(&info, &path, false) {
            Ok(()) => {}
            Err(e) => self.update = crate::update::UpdateState::Error(e),
        }
    }

    /// Když je okno schované v oznamovací oblasti a nic se nepřevádí, nainstaluje stažený update samo.
    fn spawn_auto_apply(&self, info: crate::update::UpdateInfo, path: PathBuf) {
        let slots = self.slots.clone();
        let visible = self.visible.clone();
        thread::spawn(move || loop {
            thread::sleep(std::time::Duration::from_secs(5));
            if visible.load(Ordering::Relaxed) {
                break; // okno je otevřené: uživatel uvidí tlačítko a rozhodne sám
            }
            if slots.is_idle() {
                let _ = crate::update::apply(&info, &path, true);
                break;
            }
        });
    }

    /// Vytvoří zástupce v nabídce Start (na pozadí).
    pub fn make_shortcut(&self) {
        let tx = self.tx.clone();
        let ctx = self.egui.clone();
        thread::spawn(move || {
            let _ = tx.send(Bg::Shortcut(crate::shell::create_start_shortcut()));
            ctx.request_repaint();
        });
    }

    pub fn toast(&mut self, s: impl Into<String>) {
        self.toast = Some((s.into(), Instant::now()));
    }

    pub fn default_opts(&self) -> Opts {
        Opts { hw: self.settings.hw, hw_decode: self.settings.hw_decode, avx512: self.settings.avx512, ..Opts::default() }
    }

    pub fn out_dir_for(&self, input: &std::path::Path) -> PathBuf {
        if !self.settings.same_dir && !self.settings.out_dir.is_empty() {
            return PathBuf::from(&self.settings.out_dir);
        }
        input.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."))
    }

    // ───────────── úlohy ─────────────

    pub fn start_task<F>(&mut self, meta: TaskMeta, f: F) -> u64
    where
        F: FnOnce(&JobCtx) -> Res<Outcome> + Send + 'static,
    {
        self.start_task_w(meta, 1, f)
    }

    pub fn start_task_w<F>(&mut self, meta: TaskMeta, weight: usize, f: F) -> u64
    where
        F: FnOnce(&JobCtx) -> Res<Outcome> + Send + 'static,
    {
        let id = self.next_id;
        self.next_id += 1;
        let shared = Shared::new();
        let ctx = JobCtx { shared: shared.clone(), si: self.si.clone(), tools: self.tools.clone() };
        let tx = self.tx.clone();
        let egui_ctx = self.egui.clone();
        let slots = self.slots.clone();
        let sh = shared.clone();
        thread::spawn(move || {
            let res = if let Some(w) = slots.acquire(&sh, weight) {
                sh.mark_started();
                let t0 = Instant::now();
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&ctx))).unwrap_or_else(|_| Err(tr!("Interní chyba (panic)", "Internal error (panic)").into()));
                slots.release(w);
                (r, t0.elapsed().as_secs_f64())
            } else {
                (Err(CANCELLED.into()), 0.0)
            };
            let _ = tx.send(Bg::Done(id, res.0, res.1));
            egui_ctx.request_repaint();
        });
        self.tasks.insert(id, Task { shared, meta, state: TaskState::Running });
        id
    }

    fn poll(&mut self) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Bg::BootNote(s) => self.boot_note = s,
                Bg::Ready(t, si) => {
                    self.tools = Arc::new(t);
                    self.si = Arc::new(si);
                    self.ready = true;
                    if cfg!(windows) && !self.settings.shortcut_done {
                        self.make_shortcut();
                    }
                    if self.settings.check_updates {
                        self.check_update();
                    }
                    let files = std::mem::take(&mut self.pending);
                    if !files.is_empty() {
                        self.add_files(files);
                    }
                    // ladicí zkratky (nepovinné): TC_TAB=queue|tools|info|stats|settings|about, TC_FILES=cesta1;cesta2
                    if let Ok(files) = std::env::var("TC_FILES") {
                        self.add_files(files.split(';').filter(|s| !s.is_empty()).map(PathBuf::from).collect());
                    }
                    if std::env::var("TC_OPEN").is_ok() {
                        for it in self.items.iter_mut() {
                            it.open = true;
                        }
                    }
                    if let Ok(tab) = std::env::var("TC_TAB") {
                        self.tab = match tab.as_str() {
                            "queue" => Tab::Queue,
                            "tools" => Tab::Tools,
                            "info" => Tab::Info,
                            "stats" => Tab::Stats,
                            "settings" => Tab::Settings,
                            "about" => Tab::About,
                            _ => self.tab,
                        };
                    }
                    if let Ok(t) = std::env::var("TC_TOOL") {
                        self.tool_tab = match t.as_str() {
                            "audio" => ToolTab::Audio,
                            "tracks" => ToolTab::Tracks,
                            _ => ToolTab::Pdf,
                        };
                    }
                    if let Ok(f) = std::env::var("TC_INFO") {
                        self.info_load(PathBuf::from(f));
                    }
                    if let Ok(f) = std::env::var("TC_TRACKS") {
                        self.tracks_load(PathBuf::from(f));
                    }
                }
                Bg::Probe(path, r) => {
                    let (probe, err) = match r {
                        Ok(p) => (Some(p), None),
                        Err(e) => (None, Some(e)),
                    };
                    if let Some(it) = self.items.iter_mut().find(|i| i.path == path) {
                        it.probe = probe.clone();
                    }
                    self.info.on_probe(&path, &probe, err);
                    self.tracks.on_probe(&path, probe.clone());
                    self.audio.on_probe(&path, probe);
                }
                Bg::Pages(path, n) => self.pdf.on_pages(&path, n),
                Bg::Inbox(files, show) => {
                    if !files.is_empty() {
                        if self.ready {
                            self.add_files(files);
                        } else {
                            self.pending.extend(files);
                        }
                    }
                    if show {
                        self.visible.store(true, Ordering::Relaxed);
                        self.egui.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                        self.egui.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                        self.egui.send_viewport_cmd(egui::ViewportCommand::Focus);
                        self.egui.request_repaint();
                    }
                }
                Bg::Update(r) => {
                    self.update = match r {
                        Ok(Some(u)) => crate::update::UpdateState::Available(u),
                        Ok(None) => crate::update::UpdateState::UpToDate,
                        Err(e) => crate::update::UpdateState::Error(e),
                    };
                    if self.settings.auto_update {
                        if let crate::update::UpdateState::Available(u) = &self.update {
                            if u.mode != crate::update::InstallMode::Manual && u.asset.is_some() {
                                self.start_update_download();
                            }
                        }
                    }
                }
                Bg::UpdateTick => {
                    let idle = matches!(self.update, crate::update::UpdateState::Idle | crate::update::UpdateState::UpToDate | crate::update::UpdateState::Error(_));
                    if self.settings.check_updates && self.ready && idle {
                        self.check_update();
                    }
                }
                Bg::UpdateProgress(f) => {
                    if let crate::update::UpdateState::Downloading(u, _) = &self.update {
                        self.update = crate::update::UpdateState::Downloading(u.clone(), f);
                    }
                }
                Bg::UpdateDownloaded(r) => {
                    let info = match &self.update {
                        crate::update::UpdateState::Downloading(u, _) => Some(u.clone()),
                        _ => None,
                    };
                    match (r, info) {
                        (Ok(p), Some(u)) => {
                            self.update = crate::update::UpdateState::Ready(u.clone(), p.clone());
                            if self.settings.auto_update {
                                self.spawn_auto_apply(u, p);
                            }
                        }
                        (Err(e), _) => self.update = crate::update::UpdateState::Error(e),
                        _ => {}
                    }
                }
                Bg::Shortcut(r) => {
                    self.settings.shortcut_done = true;
                    self.settings.save();
                    match r {
                        Ok(_) => self.toast(tr!("Přidáno do nabídky Start / aplikací – najdeš ho ve vyhledávání systému jako TomasekConvert", "Added to the Start menu / app launcher – find it in system search as TomasekConvert")),
                        Err(e) => self.toast(trf!("Zástupce se nepodařilo vytvořit: {e}", "Could not create the shortcut: {e}")),
                    }
                }
                Bg::Done(id, res, secs) => {
                    let Some(t) = self.tasks.get_mut(&id) else { continue };
                    match res {
                        Ok(o) => {
                            self.stats.record(HistoryEntry {
                                when: unix_now(),
                                name: t.meta.name.clone(),
                                from: t.meta.from.clone(),
                                to: t.meta.to.clone(),
                                kind: t.meta.kind.clone(),
                                engine: o.engine,
                                secs,
                                bytes_in: o.bytes_in,
                                bytes_out: o.bytes_out,
                                output: o.outputs.first().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
                            });
                            t.state = TaskState::Done(o, secs);
                        }
                        Err(e) if e == CANCELLED => t.state = TaskState::Cancelled,
                        Err(e) => {
                            self.stats.record_failure();
                            t.state = TaskState::Failed(e);
                        }
                    }
                }
            }
        }
    }

    pub fn spawn_probe(&self, path: PathBuf) {
        let tools = self.tools.clone();
        let tx = self.tx.clone();
        let ctx = self.egui.clone();
        thread::spawn(move || {
            let r = crate::probe::run(&tools, &path);
            let _ = tx.send(Bg::Probe(path, r));
            ctx.request_repaint();
        });
    }

    pub fn spawn_pages(&self, path: PathBuf) {
        let tools = self.tools.clone();
        let tx = self.tx.clone();
        let ctx = self.egui.clone();
        thread::spawn(move || {
            let n = crate::pdfops::page_count(&tools, &path, "");
            let _ = tx.send(Bg::Pages(path, n));
            ctx.request_repaint();
        });
    }

    // ───────────── fronta souborů ─────────────

    pub fn add_files(&mut self, paths: Vec<PathBuf>) {
        let mut added = 0;
        for p in paths {
            if self.items.iter().any(|i| i.path == p && i.task.is_none()) {
                continue;
            }
            let is_dir = p.is_dir();
            let ext = if is_dir { String::new() } else { crate::util::full_ext(&p) };
            let kind = if is_dir { Kind::Folder } else { detect_kind(&ext, &self.si) };
            let targets = targets_for(kind, &ext, &self.si);
            let sel = pick_default(kind, &ext, &targets);
            let id = self.next_id;
            self.next_id += 1;
            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let mut opts = self.default_opts();
            if kind == Kind::Audio {
                opts.audio_track = -1;
            }
            self.items.push(Item {
                id,
                size: path_size(&p),
                path: p.clone(),
                name,
                kind,
                in_ext: ext,
                targets,
                sel,
                opts,
                out_dir: None,
                open: false,
                task: None,
                probe: None,
                filter: String::new(),
            });
            if matches!(kind, Kind::Video | Kind::Audio) {
                self.spawn_probe(p);
            }
            added += 1;
        }
        if added > 0 {
            self.tab = Tab::Queue;
        }
    }

    pub fn convert_item(&mut self, idx: usize) {
        let Some(it) = self.items.get(idx) else { return };
        let Some(fmt) = it.fmt().cloned() else { return };
        let out_dir = it.out_dir.clone().unwrap_or_else(|| self.out_dir_for(&it.path));
        let _ = std::fs::create_dir_all(&out_dir);
        let job = ConvertJob { input: it.path.clone(), in_ext: it.in_ext.clone(), kind: it.kind, fmt: fmt.clone(), opts: it.opts.clone(), out_dir };
        let meta = TaskMeta {
            name: it.name.clone(),
            from: if it.in_ext.is_empty() { tr!("složka", "folder").into() } else { it.in_ext.clone() },
            to: fmt.ext.clone(),
            kind: it.kind.label().to_string(),
        };
        let weight = self.job_weight(&job);
        let id = self.start_task_w(meta, weight, move |ctx| convert::run_convert(ctx, &job));
        self.items[idx].task = Some(id);
    }

    pub fn convert_all(&mut self) {
        for i in 0..self.items.len() {
            let runnable = match self.items[i].task.and_then(|t| self.tasks.get(&t)) {
                None => true,
                Some(t) => matches!(t.state, TaskState::Failed(_) | TaskState::Cancelled),
            };
            if runnable && self.items[i].fmt().is_some() {
                self.convert_item(i);
            }
        }
    }

    pub fn cancel_all(&mut self) {
        for t in self.tasks.values() {
            if t.running() {
                t.shared.cancel();
            }
        }
    }

    pub fn any_running(&self) -> bool {
        self.tasks.values().any(|t| t.running())
    }
}

pub fn pick_default(kind: Kind, in_ext: &str, targets: &[Fmt]) -> usize {
    let prefs: &[&str] = match kind {
        Kind::Video => &["mp4", "mkv", "webm"],
        Kind::Audio => &["mp3", "flac", "wav"],
        Kind::Image => &["png", "jpg", "webp"],
        Kind::Document => &["pdf", "docx", "md"],
        Kind::Pdf => &["png", "txt"],
        Kind::Archive => &["folder", "zip"],
        Kind::Folder => &["zip", "7z"],
        Kind::Subtitle => &["srt", "ass", "vtt"],
        Kind::Other => &[],
    };
    for p in prefs {
        if *p != in_ext {
            if let Some(i) = targets.iter().position(|t| t.ext == *p) {
                return i;
            }
        }
    }
    0
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.start_min {
            self.start_min = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
        self.poll();

        // Nic se nepřekresluje "pro jistotu". Okno se probouzí jen když přijde událost (myš, klávesa,
        // zpráva z vlákna, tray…) nebo když běží převod. Nečinná aplikace tak nespotřebuje skoro žádný CPU.

        // tray: zobrazit / ukončit
        while let Ok(m) = self.tray_rx.try_recv() {
            match m {
                crate::tray::TrayMsg::Show => {
                    self.visible.store(true, Ordering::Relaxed);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    ctx.request_repaint();
                }
                crate::tray::TrayMsg::Quit => {
                    self.quitting = true;
                    self.cancel_all();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        // křížek = schovat do tray (převody dál běží)
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting && self.settings.tray && self.tray.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            self.visible.store(false, Ordering::Relaxed);
        }

        // Okno schované v tray (nebo minimalizované): nic nekreslíme a o žádný další snímek NEžádáme.
        // (Dřív tu bylo `request_repaint_after(Duration::MAX)` – to nic nedělá, ale v eframe 0.33 navíc
        // spadne na přetečení `Instant + Duration`; a `request_repaint_after(250 ms)` při převodu
        // by schované okno zbytečně budilo.) Probudí ho jen skutečná událost: tray, předaný soubor,
        // dokončený převod / aktualizace z vlákna.
        let minimized = ctx.input(|i| i.viewport().minimized.unwrap_or(false));
        if !self.visible.load(Ordering::Relaxed) || minimized {
            return;
        }

        paint_background(ctx, self.settings.dark);

        // soubory přetažené do okna
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        if !dropped.is_empty() && self.ready {
            crate::ui_tools::route_drop(self, dropped);
        }
        if self.any_running() {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }

        egui::TopBottomPanel::top("nav").frame(egui::Frame::NONE.inner_margin(Margin { left: 12, right: 12, top: 16, bottom: 6 })).show(ctx, |ui| {
            self.nav(ui);
        });
        egui::TopBottomPanel::bottom("footer")
            .frame(egui::Frame::new().fill(footer_bg()).stroke(egui::Stroke::new(1.0_f32, border())).inner_margin(Margin::symmetric(16, 10)))
            .show(ctx, |ui| self.footer(ui));

        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
            if !self.ready {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() * 0.35);
                    title(ui, "TomasekConvert", 34.0);
                    ui.label(muted("Beautifully crafted, insanely fast and local conversion."));
                    ui.add_space(14.0);
                    ui.spinner();
                    ui.label(muted(self.boot_note.clone()));
                    ui.label(muted(tr!("Při prvním spuštění se vložené nástroje rozbalí (pár sekund).", "On first launch the bundled tools are unpacked (a few seconds).")).size(12.0));
                });
                return;
            }
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let avail = ui.available_width();
                let w = avail.min(1180.0) - 24.0;
                let pad = ((avail - w) / 2.0).max(8.0);
                ui.horizontal_top(|ui| {
                    ui.add_space(pad);
                    ui.vertical(|ui| {
                        ui.set_width(w.max(300.0));
                        ui.set_max_width(w.max(300.0));
                        ui.add_space(8.0);
                        crate::ui_setup::update_banner(self, ui);
                        match self.tab {
                            Tab::Upload => crate::ui_home::show(self, ui),
                            Tab::Queue => crate::ui_queue::show(self, ui),
                            Tab::Tools => crate::ui_tools::show(self, ui),
                            Tab::Info => crate::ui_info::show(self, ui),
                            Tab::Stats => crate::ui_stats::show(self, ui),
                            Tab::Settings => crate::ui_settings::show(self, ui),
                            Tab::About => crate::ui_settings::about(self, ui),
                        }
                        ui.add_space(30.0);
                    });
                });
            });
        });

        if self.ready && !self.settings.setup_done && self.wizard.is_none() {
            self.wizard = Some(crate::ui_setup::Wizard { step: 0 });
        }
        if self.wizard.is_some() {
            crate::ui_setup::show(self, ctx);
        }
        if hovering {
            let rect = ctx.content_rect();
            let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("dropveil")));
            p.rect_filled(rect, CornerRadius::ZERO, Color32::from_rgba_unmultiplied(255, 130, 235, 40));
            p.text(rect.center(), egui::Align2::CENTER_CENTER, tr!("Pusť soubory sem", "Drop files here"), bold(34.0), Color32::WHITE);
        }
        if let Some((msg, t)) = &self.toast {
            if t.elapsed().as_secs() < 4 {
                egui::Area::new(egui::Id::new("toast")).anchor(egui::Align2::CENTER_BOTTOM, vec2(0.0, -60.0)).show(ctx, |ui| {
                    card_frame(Color32::from_rgb(34, 30, 44)).show(ui, |ui| ui.label(RichText::new(msg.clone()).color(Color32::from_rgb(244, 240, 248))));
                });
                ctx.request_repaint_after(std::time::Duration::from_millis(500));
            } else {
                self.toast = None;
            }
        }
    }
}

impl App {
    fn nav(&mut self, ui: &mut egui::Ui) {
        // `vertical_centered` nevycentruje prvek, jehož šířku ještě neznáme (ten skončí vlevo).
        // Proto si šířku panelu pamatujeme z minulého snímku a vyplníme levý okraj sami.
        let width_id = egui::Id::new("nav-width");
        let known: f32 = ui.ctx().data(|d| d.get_temp(width_id)).unwrap_or(0.0);
        let avail = ui.available_width();
        // v úzkém okně mají neaktivní záložky jen ikonu (popisek je v tooltipu)
        let compact = avail < 1020.0;
        let frame = egui::Frame::new().fill(card_c()).stroke(egui::Stroke::new(1.0_f32, border())).corner_radius(CornerRadius::same(20)).inner_margin(Margin::symmetric(10, 8));
        let mut measured = known;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.add_space(((avail - known) / 2.0).max(0.0));
            let r = frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.add(egui::Button::new(RichText::new("TOMASEK").font(bold(22.0)).color(accent_text())).fill(accent()).corner_radius(CornerRadius::same(14)).min_size(vec2(104.0, 44.0)));
                    let tabs = [
                        (Tab::Upload, ph::UPLOAD_SIMPLE, tr!("Nahrát", "Upload")),
                        (Tab::Queue, ph::ARROWS_CLOCKWISE, tr!("Konvertovat", "Convert")),
                        (Tab::Tools, ph::WRENCH, tr!("Nástroje", "Tools")),
                        (Tab::Info, ph::INFO, "Info"),
                        (Tab::Stats, ph::CHART_BAR, tr!("Statistiky", "Statistics")),
                        (Tab::Settings, ph::GEAR, tr!("Nastavení", "Settings")),
                        (Tab::About, ph::QUESTION, tr!("O aplikaci", "About")),
                    ];
                    for (t, icon, label) in tabs {
                        let active = self.tab == t;
                        let count = if t == Tab::Queue && !self.items.is_empty() { format!(" ({})", self.items.len()) } else { String::new() };
                        let text = if compact && !active { format!("{icon}{count}") } else { format!("{icon}  {label}{count}") };
                        let b = egui::Button::new(RichText::new(text).font(bold(14.5)))
                            .fill(if active { card_hi() } else { Color32::TRANSPARENT })
                            .corner_radius(CornerRadius::same(12))
                            .min_size(vec2(0.0, 40.0));
                        let resp = ui.add(b);
                        if compact && !active {
                            resp.clone().on_hover_text(label);
                        }
                        if resp.clicked() {
                            self.tab = t;
                        }
                    }
                    ui.separator();
                    let moon = if self.settings.dark { ph::MOON } else { ph::SUN };
                    let tip = if self.settings.dark { tr!("Přepnout na světlý vzhled", "Switch to light theme") } else { tr!("Přepnout na tmavý vzhled", "Switch to dark theme") };
                    if ui.add(egui::Button::new(RichText::new(moon).size(18.0)).fill(Color32::TRANSPARENT).min_size(vec2(40.0, 40.0))).on_hover_text(tip).clicked() {
                        self.settings.dark = !self.settings.dark;
                        self.apply_theme();
                        self.settings.save();
                    }
                });
            });
            measured = r.response.rect.width();
        });
        if (measured - known).abs() > 0.5 {
            // nejvýš pár opakování pro jednu šířku okna: kdyby se hodnota nikdy neustálila, smyčka
            // "změř → překresli" by jinak běžela donekonečna a držela CPU na 100 % jednoho jádra
            let tries_id = egui::Id::new("nav-width-tries");
            let (last_avail, tries): (f32, u8) = ui.ctx().data(|d| d.get_temp(tries_id)).unwrap_or((avail, 0));
            let tries = if (last_avail - avail).abs() > 1.0 { 0 } else { tries };
            if tries < 4 {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(width_id, measured);
                    d.insert_temp(tries_id, (avail, tries + 1));
                });
                ui.ctx().request_repaint();
            } else {
                ui.ctx().data_mut(|d| d.insert_temp(width_id, measured));
            }
        }
    }

    fn footer(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            let dim = |s: String| RichText::new(s).color(muted_c()).size(13.0);
            ui.label(dim(format!("TomasekConvert  ·  {}", env!("CARGO_PKG_VERSION"))));
            if self.ready {
                ui.label(dim(format!("{}  CPU: {} ({}C/{}T{})", ph::CPU, self.si.cpu, self.si.cores, self.si.threads, if self.si.avx512 { ", AVX-512" } else { "" })));
                ui.label(dim(format!("{}  GPU: {}", ph::GRAPHICS_CARD, self.si.gpu_label())));
                let bs: Vec<&str> = self.si.backends().iter().map(|b| b.short()).collect();
                ui.label(dim(if bs.is_empty() { tr!("Hardwarové enkodéry: nenalezeny", "Hardware encoders: none found").to_string() } else { format!("{}: {}", tr!("Hardwarové enkodéry", "Hardware encoders"), bs.join(", ")) }));
                // NVIDIA karta je, ale NVENC nejede, protože ovladač je starý -> řekni to rovnou
                if self.si.has_nvidia() && self.si.failures_of(crate::gpu::Backend::Nvenc).iter().any(|(_, f)| matches!(f, crate::gpu::Fail::DriverTooOld(_))) {
                    ui.label(RichText::new(format!("{}  {}", ph::WARNING, tr!("NVENC: aktualizuj ovladač NVIDIA (Nastavení)", "NVENC: update the NVIDIA driver (see Settings)"))).color(warn()).size(13.0));
                }
                if let Some(d) = self.si.hw_decoder() {
                    ui.label(dim(format!("{}: {d}", tr!("HW dekódování", "HW decoding"))));
                }
            }
        });
    }
}
