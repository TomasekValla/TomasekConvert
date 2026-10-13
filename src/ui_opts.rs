//! Panel nastavení převodu (kodek, kvalita, GPU / CPU...). Použito ve frontě i v nástrojích.
//! Základní režim ukazuje jen to podstatné, pokročilý všechny hodnoty (s nápovědou (i)).
use crate::{
    ffmpeg::pick_gpu,
    formats::*,
    gpu::Backend,
    hw::SysInfo,
    opts::*,
    probe::Probe,
    theme::*,
    ui_common::*,
};
use egui::{Color32, RichText, Ui};
use egui_phosphor::regular as ph;

const X_PRESETS: [&str; 10] = ["ultrafast", "superfast", "veryfast", "faster", "fast", "medium", "slow", "slower", "veryslow", "placebo"];

/// Hodnota ze zdroje vedle pole ("zdroj: 1920×1080").
fn hint(ui: &mut Ui, text: impl Into<String>) {
    ui.label(RichText::new(format!("{}: {}", tr!("zdroj", "source"), text.into())).color(accent_fg()).size(12.5));
}

fn fps_text(f: f64) -> String {
    format!("{:.3}", f).trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Přehled zdroje (kontejner, video, audio) – ať víš, z čeho vycházíš.
pub fn source_strip(ui: &mut Ui, probe: Option<&Probe>) {
    let Some(p) = probe else { return };
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(tr!("Zdroj", "Source")).font(bold(13.0)).color(accent_fg()));
        if !p.container.is_empty() {
            pill_tag(ui, &p.container, muted_c());
        }
        if p.duration > 0.0 {
            pill_tag(ui, &crate::util::fmt_dur(p.duration), muted_c());
        }
        if p.bit_rate > 0 {
            pill_tag(ui, &trf!("celkem {}", "total {}", crate::probe::fmt_bitrate(p.bit_rate)), muted_c());
        }
        if let Some(v) = p.video() {
            let mut t = format!("{} {}×{}", v.codec.to_uppercase(), v.width, v.height);
            if v.fps > 0.0 {
                t.push_str(&format!(" · {} fps", fps_text(v.fps)));
            }
            if !v.pix_fmt.is_empty() {
                t.push_str(&format!(" · {} ({}-bit)", v.pix_fmt, v.bit_depth));
            }
            if v.bit_rate > 0 {
                t.push_str(&format!(" · {}", crate::probe::fmt_bitrate(v.bit_rate)));
            }
            if !v.hdr.is_empty() {
                t.push_str(&format!(" · {}", v.hdr));
            }
            pill_tag(ui, &t, Color32::from_rgb(255, 140, 160));
        }
        for a in p.audio_streams().take(4) {
            let mut t = a.codec.to_uppercase();
            if a.sample_rate > 0 {
                t.push_str(&format!(" · {} Hz", a.sample_rate));
            }
            if a.channels > 0 {
                t.push_str(&format!(" · {} {}", a.channels, tr!("kan.", "ch")));
            }
            if !a.depth_label().is_empty() {
                t.push_str(&format!(" · {}", a.depth_label()));
            }
            if a.bit_rate > 0 {
                t.push_str(&format!(" · {}", crate::probe::fmt_bitrate(a.bit_rate)));
            }
            if !a.lang.is_empty() {
                t.push_str(&format!(" · {}", a.lang));
            }
            pill_tag(ui, &t, Color32::from_rgb(190, 150, 255));
        }
    });
}

pub fn mode_switch(ui: &mut Ui, adv: &mut bool) {
    ui.horizontal(|ui| {
        if chip(ui, !*adv, tr!("Základní", "Basic")).on_hover_text(tr!("Jen nejdůležitější volby.", "Only the most important options.")).clicked() {
            *adv = false;
        }
        if chip(ui, *adv, tr!("Pokročilé", "Advanced")).on_hover_text(tr!("Všechny hodnoty do detailu.", "Every value, in detail.")).clicked() {
            *adv = true;
        }
    });
}

pub fn audio_track_combo(ui: &mut Ui, o: &mut Opts, probe: Option<&Probe>) {
    let Some(p) = probe else { return };
    let n = p.count(crate::probe::StreamKind::Audio);
    if n < 2 {
        return;
    }
    row_t(ui, tr!("Audio stopa", "Audio track"), tr!("Která zvuková stopa se použije.", "Which audio track to use."), |ui| {
        let label = |i: i32| -> String {
            if i < 0 {
                return tr!("Všechny (video) / první (audio)", "All (video) / first (audio)").to_string();
            }
            p.audio_streams()
                .nth(i as usize)
                .map(|a| format!("#{} · {} · {}{}", i + 1, a.codec.to_uppercase(), if a.lang.is_empty() { "?" } else { &a.lang }, if a.title.is_empty() { String::new() } else { format!(" · {}", a.title) }))
                .unwrap_or_default()
        };
        egui::ComboBox::from_id_salt("atrack").selected_text(label(o.audio_track)).show_ui(ui, |ui| {
            ui.selectable_value(&mut o.audio_track, -1, label(-1));
            for i in 0..n as i32 {
                ui.selectable_value(&mut o.audio_track, i, label(i));
            }
        });
    });
}

pub fn trim_rows(ui: &mut Ui, o: &mut Opts) {
    row_t(ui, tr!("Střih (od – do)", "Trim (from – to)"), tr!("Odřízne začátek a/nebo konec. Zadej hh:mm:ss nebo sekundy; prázdné = celé.", "Cut the start and/or end. Enter hh:mm:ss or seconds; empty = everything."), |ui| {
        ui.add(egui::TextEdit::singleline(&mut o.trim_start).hint_text("00:00:00").desired_width(90.0));
        ui.label("–");
        ui.add(egui::TextEdit::singleline(&mut o.trim_end).hint_text(tr!("konec", "end")).desired_width(90.0));
    });
}

fn apply_cpu_preset(o: &mut Opts) {
    let (x, svt, vp9) = match o.preset {
        Preset::Quality => ("slow", 4, 1),
        Preset::Balanced => ("medium", 6, 2),
        Preset::Speed => ("veryfast", 10, 5),
    };
    o.x_preset = x.into();
    o.svt_preset = svt;
    o.vp9_speed = vp9;
}

/// Řádky "Kodér": GPU / CPU, výběr backendu, HW dekódování.
fn device_rows(ui: &mut Ui, o: &mut Opts, si: &SysInfo, ext: &str) {
    let gpu = pick_gpu(si, o, ext);
    row_t(
        ui,
        tr!("Kodér", "Encoder"),
        tr!("GPU = hardwarový enkodér grafické karty (rychlý, o něco větší soubory). CPU = procesor (pomalejší, nejlepší poměr kvalita/velikost).", "GPU = your graphics card's hardware encoder (fast, slightly larger files). CPU = the processor (slower, best quality per size)."),
        |ui| {
            if chip(ui, o.hw == HwMode::Auto, "GPU").clicked() {
                o.hw = HwMode::Auto;
            }
            if chip(ui, o.hw == HwMode::Cpu, "CPU").clicked() {
                o.hw = HwMode::Cpu;
            }
            let bs = si.backends();
            if o.hw == HwMode::Auto && bs.len() > 1 {
                let cur = Backend::from_id(&o.backend).map(|b| b.label().to_string()).unwrap_or_else(|| tr!("Automaticky", "Automatic").to_string());
                egui::ComboBox::from_id_salt("backend").selected_text(cur).show_ui(ui, |ui| {
                    ui.selectable_value(&mut o.backend, "auto".into(), tr!("Automaticky", "Automatic"));
                    for b in bs {
                        ui.selectable_value(&mut o.backend, b.id().to_string(), b.label());
                    }
                });
            }
            match gpu {
                Some((b, enc)) => {
                    ui.label(RichText::new(format!("{}  GPU: {} ({enc})", ph::LIGHTNING, b.label())).color(good()));
                }
                None => {
                    let want = o.pix_fmt.trim();
                    let why = if o.hw == HwMode::Cpu {
                        tr!("zvoleno CPU", "CPU selected")
                    } else if si.hw_encoders.is_empty() {
                        tr!("GPU enkodér nenalezen", "no GPU encoder found")
                    } else if !want.is_empty() && !matches!(want, "yuv420p" | "nv12" | "yuv420p10le" | "p010le") {
                        tr!("tenhle pixel format GPU neumí", "the GPU can't encode this pixel format")
                    } else {
                        tr!("pro tenhle kodek není GPU enkodér", "no GPU encoder for this codec")
                    };
                    ui.label(muted(format!("{}  CPU ({why})", ph::CPU)));
                }
            }
        },
    );
    if si.hw_decoder().is_some() {
        row_t(ui, tr!("Dekódování", "Decoding"), tr!("Zdroj se dekóduje na GPU. Vypni, pokud se objeví artefakty.", "Decode the source on the GPU too. Turn off if you see glitches."), |ui| {
            ui.checkbox(&mut o.hw_decode, format!("{} ({})", tr!("Hardwarové", "Hardware"), si.hw_decoder().unwrap_or("")));
        });
    }
}

pub fn audio_rows(ui: &mut Ui, o: &mut Opts, ext: &str, si: &SysInfo, probe: Option<&Probe>, video_ctx: bool, adv: bool) {
    let src = probe.and_then(|p| if o.audio_track >= 0 { p.audio_streams().nth(o.audio_track as usize) } else { p.audio_streams().next() });
    let choices: Vec<&'static str> = if video_ctx {
        ACODECS.iter().filter(|c| si.has_encoder(c.enc)).map(|c| c.id).collect()
    } else {
        audio_choices(ext).into_iter().filter(|id| acodec(id).map(|c| si.has_encoder(c.enc)).unwrap_or(false)).collect()
    };
    row_t(ui, tr!("Audio kodek", "Audio codec"), tr!("Formát zvuku. AAC a MP3 jsou kompatibilní skoro všude, FLAC je bezeztrátový, Opus má nejlepší kvalitu při nízkém bitrate.", "Audio format. AAC and MP3 play almost everywhere, FLAC is lossless, Opus gives the best quality at low bitrates."), |ui| {
        let cur = if o.acodec == "auto" { "Auto".to_string() } else { acodec(&o.acodec).map(|c| c.label.to_string()).unwrap_or_default() };
        egui::ComboBox::from_id_salt("acodec").selected_text(cur).show_ui(ui, |ui| {
            if video_ctx || choices.len() > 1 {
                ui.selectable_value(&mut o.acodec, "auto".into(), if video_ctx { tr!("Auto (beze změny, pokud to kontejner umí)", "Auto (unchanged if the container allows)") } else { "Auto" });
            }
            for id in &choices {
                if let Some(c) = acodec(id) {
                    ui.selectable_value(&mut o.acodec, id.to_string(), c.label);
                }
            }
        });
        if let Some(a) = src {
            hint(ui, a.codec.to_uppercase());
        }
    });
    let eff = if o.acodec == "auto" { if video_ctx { video_container_default_audio(ext) } else { default_audio_for_ext(ext) } } else { o.acodec.as_str() };
    let lossless = acodec(eff).map(|c| c.lossless).unwrap_or(false);
    if lossless {
        if eff == "flac" {
            row_t(ui, tr!("Komprese FLAC", "FLAC compression"), tr!("Bezeztrátové – mění jen rychlost a velikost souboru, ne kvalitu. 0 = rychle, 12 = nejmenší.", "Lossless – only changes speed and file size, never quality. 0 = fast, 12 = smallest."), |ui| {
                ui.add(egui::Slider::new(&mut o.flac_level, 0..=12));
            });
        } else {
            ui.label(muted(tr!("Bezeztrátový kodek – bez volby bitrate.", "Lossless codec – no bitrate setting.")).size(12.5));
        }
        if adv && (eff.starts_with("pcm") || matches!(eff, "flac" | "alac" | "wavpack")) {
            row_t(ui, tr!("Bitů na vzorek", "Bits per sample"), tr!("Bitová hloubka zvuku. 16 = CD, 24 = studio, 32 float = bez ztráty při úpravách.", "Audio bit depth. 16 = CD, 24 = studio, 32 float = no loss while editing."), |ui| {
                let mut sel: u32 = if o.afloat { 1 } else { o.abits };
                let src_depth = src.map(|a| if a.depth_label().is_empty() { a.sample_fmt.clone() } else { a.depth_label() }).filter(|t| !t.is_empty());
                let mut opts: Vec<(u32, String)> = vec![(0, src_depth.clone().unwrap_or_else(|| tr!("Zdroj", "Source").to_string()))];
                opts.extend([(8u32, "8"), (16, "16"), (24, "24"), (32, "32"), (1, "32 float")].map(|(v, l)| (v, l.to_string())));
                for (v, l) in &opts {
                    if chip(ui, sel == *v, l).clicked() {
                        sel = *v;
                    }
                }
                o.afloat = sel == 1;
                o.abits = if sel == 1 { 0 } else { sel };
                if sel != 0 {
                    if let Some(t) = src_depth {
                        hint(ui, t);
                    }
                }
            });
        }
    } else if eff != "amr" {
        row_t(ui, "Bitrate", tr!("Vyšší bitrate = lepší zvuk a větší soubor. Pro hudbu stačí 192–256 kb/s (MP3/AAC), u Opusu 96–128.", "Higher bitrate = better sound and a bigger file. 192–256 kb/s is plenty for music (MP3/AAC), 96–128 for Opus."), |ui| {
            if o.akbps == 0 {
                o.akbps = crate::ffmpeg::default_kbps(eff, 2);
            }
            dv_u32(ui, &mut o.akbps, 8..=4096, "kb/s", "", 1.0);
            ui.add(egui::Slider::new(&mut o.akbps, 32..=640).show_value(false));
            if preset_btn(ui, tr!("Výchozí", "Default")) {
                o.akbps = 0;
            }
            if let Some(a) = src {
                if a.bit_rate > 0 {
                    hint(ui, crate::probe::fmt_bitrate(a.bit_rate));
                }
            }
        });
    }
    if adv {
        row_t(ui, tr!("Vzorkování", "Sample rate"), tr!("Počet vzorků za sekundu. 44,1 kHz = CD, 48 kHz = video. 'Zdroj' ponechá původní.", "Samples per second. 44.1 kHz = CD, 48 kHz = video. 'Source' keeps the original."), |ui| {
            let sr = src.map(|a| a.sample_rate).filter(|x| *x > 0);
            dv_u32_src(ui, &mut o.sample_rate, 0..=768_000, "Hz", sr, tr!("Zdroj", "Source"), 100.0);
            let src_lbl = sr.map(|x| format!("{x} Hz")).unwrap_or_else(|| tr!("Zdroj", "Source").to_string());
            if preset_btn(ui, &src_lbl) {
                o.sample_rate = 0;
            }
            for (v, l) in [(8000, "8k"), (16000, "16k"), (22050, "22.05k"), (32000, "32k"), (44100, "44.1k"), (48000, "48k"), (88200, "88.2k"), (96000, "96k"), (192000, "192k")] {
                if sr == Some(v) {
                    continue;
                }
                if preset_btn(ui, l) {
                    o.sample_rate = v;
                }
            }
            if o.sample_rate != 0 {
                if let Some(x) = sr {
                    hint(ui, format!("{x} Hz"));
                }
            }
        });
        row_t(ui, tr!("Kanály", "Channels"), tr!("1 = mono, 2 = stereo, 6 = 5.1, 8 = 7.1. 'Zdroj' ponechá původní.", "1 = mono, 2 = stereo, 6 = 5.1, 8 = 7.1. 'Source' keeps the original."), |ui| {
            let sc = src.map(|a| a.channels).filter(|x| *x > 0);
            dv_u32_src(ui, &mut o.channels, 0..=32, tr!("kan.", "ch"), sc, tr!("Zdroj", "Source"), 0.2);
            let src_lbl = match (src, sc) {
                (Some(a), Some(n)) if !a.channel_layout.is_empty() => format!("{n} ({})", a.channel_layout),
                (_, Some(n)) => format!("{n} {}", tr!("kan.", "ch")),
                _ => tr!("Zdroj", "Source").to_string(),
            };
            if preset_btn(ui, &src_lbl) {
                o.channels = 0;
            }
            for (v, l) in [(1, "Mono"), (2, "Stereo"), (6, "5.1"), (8, "7.1")] {
                if sc == Some(v) {
                    continue;
                }
                if preset_btn(ui, l) {
                    o.channels = v;
                }
            }
            if o.channels != 0 {
                if let Some(a) = src {
                    hint(ui, format!("{} ({})", a.channels, a.channel_layout));
                }
            }
        });
        row_t(ui, tr!("Hlasitost", "Volume"), tr!("Zesílení nebo zeslabení v dB. Normalizace vyrovná hlasitost podle EBU R128.", "Boost or cut in dB. Normalisation evens out loudness (EBU R128)."), |ui| {
            ui.add(egui::DragValue::new(&mut o.volume_db).range(-60.0..=60.0).speed(0.1).suffix(" dB"));
            ui.add(egui::Slider::new(&mut o.volume_db, -20.0..=20.0).show_value(false));
            ui.checkbox(&mut o.normalize, tr!("Normalizovat", "Normalise"));
        });
        audio_track_combo(ui, o, probe);
    }
}

pub fn video_rows(ui: &mut Ui, o: &mut Opts, ext: &str, si: &SysInfo, probe: Option<&Probe>, adv: bool) {
    let gpu = pick_gpu(si, o, ext);
    let sv = probe.and_then(|p| p.video());
    source_strip(ui, probe);

    section(ui, tr!("Video", "Video"));
    device_rows(ui, o, si, ext);

    // ── kodek ──
    let allowed: Vec<&VCodec> = VCODECS
        .iter()
        .filter(|c| {
            (c.containers.is_empty() || c.containers.contains(&ext))
                && (si.has_encoder(c.cpu) || crate::gpu::ALL.iter().any(|b| b.encoder(c.id).map(|e| si.has_hw_enc(*b, e)).unwrap_or(false)))
        })
        .collect();
    row_t(ui, tr!("Video kodek", "Video codec"), tr!("H.264 se přehraje všude. HEVC a AV1 dělají menší soubory, ale chtějí novější přehrávač.", "H.264 plays everywhere. HEVC and AV1 give smaller files but need a newer player."), |ui| {
        let labels: Vec<(&'static str, String)> = allowed
            .iter()
            .map(|c| {
                let mut t = o.clone();
                t.vcodec = c.id.to_string();
                let g = pick_gpu(si, &t, ext).is_some();
                (c.id, format!("{}{}", c.label, if g { "  · GPU" } else { "" }))
            })
            .collect();
        let auto_label = format!("Auto ({})", vcodec(default_vcodec(ext)).map(|c| c.label).unwrap_or("?"));
        let cur = if o.vcodec == "auto" { auto_label.clone() } else { labels.iter().find(|(id, _)| *id == o.vcodec).map(|(_, l)| l.clone()).unwrap_or_default() };
        egui::ComboBox::from_id_salt("vcodec").selected_text(cur).width(260.0).show_ui(ui, |ui| {
            ui.selectable_value(&mut o.vcodec, "auto".into(), auto_label.clone());
            for (id, l) in &labels {
                ui.selectable_value(&mut o.vcodec, id.to_string(), l.clone());
            }
        });
        ui.checkbox(&mut o.copy_video, tr!("Jen zkopírovat", "Copy only")).on_hover_text(tr!("Video se nepřekóduje (okamžité, beze ztráty). Rozlišení, FPS a kvalita se ignorují.", "The video is not re-encoded (instant, lossless). Resolution, FPS and quality are ignored."));
        if let Some(v) = sv {
            hint(ui, v.codec.to_uppercase());
        }
    });
    if o.copy_video {
        ui.label(muted(tr!("Video se nepřekóduje (okamžité, beze ztráty). Rozlišení, FPS a kvalita se ignorují.", "The video is not re-encoded (instant, lossless). Resolution, FPS and quality are ignored.")).size(12.5));
    } else {
        let id = if o.vcodec == "auto" { default_vcodec(ext) } else { o.vcodec.as_str() };
        let vc = vcodec(id).unwrap_or(&VCODECS[0]);

        // ── rychlost / preset ──
        let speed_tip = tr!("Quality = nejlepší výsledek (pomalejší), Speed = nejrychlejší (o něco větší soubor).", "Quality = best result (slower), Speed = fastest (slightly bigger file).");
        if gpu.is_some() || !adv {
            row_t(ui, tr!("Rychlost", "Speed"), speed_tip, |ui| {
                let before = o.preset;
                segmented(ui, &mut o.preset, &[(Preset::Quality, "Quality"), (Preset::Balanced, "Balanced"), (Preset::Speed, "Speed")]);
                if o.preset != before && gpu.is_none() {
                    apply_cpu_preset(o);
                }
            });
        } else {
            match vc.id {
                "h264" | "hevc" => {
                    row_t(ui, tr!("Preset (CPU)", "Preset (CPU)"), tr!("Pomalejší preset = menší soubor při stejné kvalitě, ale delší převod. 'tune' ladí enkodér pro konkrétní obsah.", "A slower preset gives a smaller file at the same quality but takes longer. 'tune' optimises for specific content."), |ui| {
                        egui::ComboBox::from_id_salt("xpreset").selected_text(o.x_preset.clone()).show_ui(ui, |ui| {
                            for p in X_PRESETS {
                                ui.selectable_value(&mut o.x_preset, p.to_string(), p);
                            }
                        });
                        egui::ComboBox::from_id_salt("xtune").selected_text(if o.x_tune.is_empty() { format!("tune: {}", tr!("žádné", "none")) } else { format!("tune: {}", o.x_tune) }).show_ui(ui, |ui| {
                            ui.selectable_value(&mut o.x_tune, String::new(), tr!("žádné", "none"));
                            let tunes: &[&str] = if vc.id == "h264" { &["film", "animation", "grain", "stillimage", "fastdecode", "zerolatency", "psnr", "ssim"] } else { &["animation", "grain", "fastdecode", "zerolatency", "psnr", "ssim"] };
                            for t in tunes {
                                ui.selectable_value(&mut o.x_tune, t.to_string(), *t);
                            }
                        });
                    });
                }
                "av1" => row_t(ui, "Preset (SVT-AV1)", tr!("0 = nejpomalejší a nejlepší, 13 = nejrychlejší.", "0 = slowest and best, 13 = fastest."), |ui| {
                    ui.add(egui::Slider::new(&mut o.svt_preset, 0..=13));
                }),
                "vp9" => row_t(ui, tr!("Rychlost (VP9)", "Speed (VP9)"), tr!("0 = nejlepší kvalita, 8 = nejrychlejší.", "0 = best quality, 8 = fastest."), |ui| {
                    ui.add(egui::Slider::new(&mut o.vp9_speed, 0..=8));
                }),
                _ => {}
            }
        }

        // ── kvalita / bitrate ──
        if adv {
            row_t(ui, "Rate control", tr!("Kvalita (CRF/QP) drží stálou kvalitu a velikost se mění. VBR cílí na průměrný bitrate. CBR drží pevný bitrate (streamování).", "Quality (CRF/QP) keeps constant quality and the size varies. VBR targets an average bitrate. CBR keeps a fixed bitrate (streaming)."), |ui| {
                segmented(ui, &mut o.rc, &[(Rc::Cqp, if gpu.is_some() { "CQP" } else { tr!("Kvalita (CRF)", "Quality (CRF)") }), (Rc::Vbr, "VBR"), (Rc::Cbr, "CBR")]);
            });
        } else {
            o.rc = if o.rc == Rc::Cbr { Rc::Cbr } else { o.rc };
        }
        match o.rc {
            Rc::Cqp => {
                if let Some((b, _)) = gpu {
                    let (lo, hi, def) = b.qp_range(vc.id);
                    if o.qp == 0 {
                        o.qp = def;
                    }
                    o.qp = o.qp.clamp(lo, hi);
                    let (lbl, tip) = if b.higher_is_better() {
                        (tr!("Kvalita (1–100)", "Quality (1–100)"), tr!("Vyšší číslo = lepší kvalita a větší soubor.", "Higher number = better quality and a bigger file."))
                    } else {
                        (tr!("Kvalita (QP)", "Quality (QP)"), tr!("Nižší číslo = lepší kvalita a větší soubor. Typicky 18–26 u H.264/HEVC.", "Lower number = better quality and a bigger file. Typically 18–26 for H.264/HEVC."))
                    };
                    row_t(ui, lbl, tip, |ui| {
                        dv_u32(ui, &mut o.qp, lo..=hi, "", "", 0.2);
                        ui.add(egui::Slider::new(&mut o.qp, lo..=hi).show_value(false));
                        if preset_btn(ui, tr!("Výchozí", "Default")) {
                            o.qp = def;
                        }
                    });
                } else if let Some((lo, hi, d)) = vc.crf {
                    if o.crf == 0 {
                        o.crf = d;
                    }
                    row_t(ui, tr!("Kvalita (CRF)", "Quality (CRF)"), tr!("Nižší číslo = lepší kvalita a větší soubor. Typicky 18–24 u H.264, 22–28 u HEVC.", "Lower number = better quality and a bigger file. Typically 18–24 for H.264, 22–28 for HEVC."), |ui| {
                        dv_u32(ui, &mut o.crf, lo..=hi, "", "", 0.2);
                        ui.add(egui::Slider::new(&mut o.crf, lo..=hi).show_value(false));
                        if preset_btn(ui, tr!("Výchozí", "Default")) {
                            o.crf = d;
                        }
                    });
                } else if !matches!(vc.id, "prores" | "dnxhd" | "ffv1" | "utvideo" | "huffyuv") {
                    if o.crf == 0 {
                        o.crf = 4;
                    }
                    row_t(ui, tr!("Kvalita (q:v)", "Quality (q:v)"), tr!("Nižší číslo = lepší kvalita.", "Lower number = better quality."), |ui| {
                        dv_u32(ui, &mut o.crf, 1..=31, "", "", 0.1);
                        ui.add(egui::Slider::new(&mut o.crf, 1..=31).show_value(false));
                    });
                } else {
                    ui.label(muted(tr!("Profesionální / bezeztrátový kodek – kvalita je daná profilem.", "Professional / lossless codec – quality is set by the profile.")).size(12.5));
                }
            }
            _ => {
                row_t(ui, tr!("Bitrate videa", "Video bitrate"), tr!("Cílový bitrate videa. Orientačně: 1080p ≈ 6–10 Mb/s (6000–10000 kb/s), 4K ≈ 20–40 Mb/s.", "Target video bitrate. Rule of thumb: 1080p ≈ 6–10 Mb/s (6000–10000 kb/s), 4K ≈ 20–40 Mb/s."), |ui| {
                    dv_u32(ui, &mut o.vkbps, 50..=2_000_000, "kb/s", "", 50.0);
                    ui.add(egui::Slider::new(&mut o.vkbps, 300..=120_000).logarithmic(true).show_value(false));
                    if let Some(v) = sv {
                        if v.bit_rate > 0 {
                            hint(ui, crate::probe::fmt_bitrate(v.bit_rate));
                        }
                    }
                });
                if adv {
                    row_t(ui, tr!("Max. bitrate / buffer", "Max bitrate / buffer"), tr!("Volitelný strop pro špičky (VBR) a velikost bufferu. 0 = automaticky.", "Optional cap for peaks (VBR) and the buffer size. 0 = automatic."), |ui| {
                        dv_u32(ui, &mut o.maxrate, 0..=2_000_000, "kb/s", "Auto", 50.0);
                        dv_u32(ui, &mut o.bufsize, 0..=4_000_000, "kb", "Auto", 50.0);
                    });
                }
            }
        }

        // ── pixel format ──
        if adv {
            row_t(ui, "Pixel format", tr!("Rozložení barev a bitová hloubka. 'Zdroj' je nejbezpečnější. 10-bit omezuje pruhování, 8-bit 4:2:0 se přehraje všude.", "Colour layout and bit depth. 'Source' is the safest. 10-bit reduces banding; 8-bit 4:2:0 plays everywhere."), |ui| {
                let src_pix = sv.map(|v| format!("{} ({}-bit)", v.pix_fmt, v.bit_depth)).filter(|_| sv.map(|v| !v.pix_fmt.is_empty()).unwrap_or(false));
                let names: [(&str, &str); 13] = [
                    ("", tr!("Zdroj", "Source")),
                    ("yuv420p", tr!("8-bit 4:2:0 (nejkompatibilnější)", "8-bit 4:2:0 (most compatible)")),
                    ("yuv420p10le", "10-bit 4:2:0"),
                    ("yuv420p12le", "12-bit 4:2:0"),
                    ("yuv422p", "8-bit 4:2:2"),
                    ("yuv422p10le", "10-bit 4:2:2"),
                    ("yuv444p", "8-bit 4:4:4"),
                    ("yuv444p10le", "10-bit 4:4:4"),
                    ("yuv444p12le", "12-bit 4:4:4"),
                    ("gray", tr!("Šedá 8-bit", "Gray 8-bit")),
                    ("gray10le", tr!("Šedá 10-bit", "Gray 10-bit")),
                    ("gbrp", "RGB 8-bit"),
                    ("gbrp10le", "RGB 10-bit"),
                ];
                let label_of = |k: &str, l: &str| -> String {
                    if k.is_empty() {
                        src_pix.clone().unwrap_or_else(|| l.to_string())
                    } else {
                        l.to_string()
                    }
                };
                let cur = names.iter().find(|(k, _)| *k == o.pix_fmt).map(|(k, l)| label_of(k, l)).unwrap_or_else(|| format!("{}: {}", tr!("vlastní", "custom"), o.pix_fmt));
                egui::ComboBox::from_id_salt("pixfmt").selected_text(cur).width(260.0).show_ui(ui, |ui| {
                    for (k, l) in names {
                        ui.selectable_value(&mut o.pix_fmt, k.to_string(), label_of(k, l));
                    }
                });
                ui.add(egui::TextEdit::singleline(&mut o.pix_fmt).hint_text(tr!("vlastní (např. nv12)", "custom (e.g. nv12)")).desired_width(130.0));
                if !o.pix_fmt.is_empty() {
                    if let Some(t) = &src_pix {
                        hint(ui, t.clone());
                    }
                }
            });
            if gpu.is_some() && !o.pix_fmt.is_empty() {
                ui.label(muted(tr!("GPU umí jen 4:2:0 (8-bit; 10-bit u HEVC/AV1). Jiné formáty se automaticky převedou na CPU.", "The GPU only handles 4:2:0 (8-bit; 10-bit for HEVC/AV1). Other formats automatically fall back to the CPU.")).size(12.0));
            }
        }

        // ── rozlišení a FPS ──
        let res_tip = tr!("Velikost výstupu. 'Zdroj' ponechá původní. Zadej jen šířku NEBO výšku, aby se zachoval poměr stran.", "Output size. 'Source' keeps the original. Set only the width OR the height to keep the aspect ratio.");
        if adv {
            let (sw, sh) = sv.map(|v| (v.width, v.height)).unwrap_or((0, 0));
            // zobrazená hodnota: když je zadaná jen jedna strana, druhá se dopočítá podle poměru stran
            let even = |x: f64| ((x / 2.0).round() as u32 * 2).max(2);
            let shown_w = if sw == 0 { None } else if o.height > 0 && sh > 0 { Some(even(sw as f64 * o.height as f64 / sh as f64)) } else { Some(sw) };
            let shown_h = if sh == 0 { None } else if o.width > 0 && sw > 0 { Some(even(sh as f64 * o.width as f64 / sw as f64)) } else { Some(sh) };
            row_t(ui, tr!("Rozlišení", "Resolution"), res_tip, |ui| {
                dv_u32_src(ui, &mut o.width, 0..=16384, "px", shown_w, tr!("Zdroj", "Source"), 2.0);
                ui.label("×");
                dv_u32_src(ui, &mut o.height, 0..=16384, "px", shown_h, tr!("Zdroj", "Source"), 2.0);
                if o.width > 0 || o.height > 0 {
                    if sw > 0 {
                        hint(ui, format!("{sw}×{sh}"));
                    }
                }
            });
            row(ui, "", |ui| {
                let src_lbl = if sw > 0 { format!("{sw}×{sh}") } else { tr!("Zdroj", "Source").to_string() };
                if preset_btn(ui, &src_lbl) {
                    o.width = 0;
                    o.height = 0;
                }
                for (h, l) in [(2160, "4K"), (1440, "1440p"), (1080, "1080p"), (720, "720p"), (576, "576p"), (480, "480p"), (360, "360p"), (240, "240p")] {
                    if sh == h {
                        continue;
                    }
                    if preset_btn(ui, l) {
                        o.width = 0;
                        o.height = h;
                    }
                }
                if o.width > 0 && o.height > 0 {
                    ui.checkbox(&mut o.keep_aspect, tr!("Zachovat poměr (pruhy)", "Keep aspect (bars)"));
                }
                egui::ComboBox::from_id_salt("algo").selected_text(o.scale_algo.clone()).width(90.0).show_ui(ui, |ui| {
                    for a in ["lanczos", "bicubic", "bilinear", "spline", "area", "neighbor"] {
                        ui.selectable_value(&mut o.scale_algo, a.to_string(), a);
                    }
                });
            });
            if (o.width % 2 == 1 || o.height % 2 == 1) && o.width + o.height > 0 && o.pix_fmt.is_empty() {
                ui.label(RichText::new(tr!("Pozor: videa 4:2:0 potřebují sudé rozměry.", "Careful: 4:2:0 video needs even dimensions.")).color(warn()).size(12.5));
            }
        } else {
            row_t(ui, tr!("Rozlišení", "Resolution"), res_tip, |ui| {
                let src_lbl = sv.map(|v| format!("{}×{}", v.width, v.height)).unwrap_or_else(|| tr!("Zdroj", "Source").to_string());
                if chip(ui, o.height == 0 && o.width == 0, &src_lbl).clicked() {
                    o.width = 0;
                    o.height = 0;
                }
                for (h, l) in [(2160, "4K"), (1440, "1440p"), (1080, "1080p"), (720, "720p"), (480, "480p")] {
                    if sv.map(|v| v.height) == Some(h) {
                        continue;
                    }
                    if chip(ui, o.height == h && o.width == 0, l).clicked() {
                        o.width = 0;
                        o.height = h;
                    }
                }
            });
        }
        let fps_tip = tr!("Snímků za sekundu. 'Zdroj' ponechá původní. Nižší FPS = menší soubor.", "Frames per second. 'Source' keeps the original. Lower FPS = smaller file.");
        row_t(ui, "FPS", fps_tip, |ui| {
            let sfps = sv.map(|v| v.fps as f32).filter(|f| *f > 0.0);
            if adv {
                dv_f32_src(ui, &mut o.fps, 0.0..=1000.0, "fps", sfps, tr!("Zdroj", "Source"), 0.05);
            }
            let src_lbl = sfps.map(|f| format!("{} fps", fps_text(f as f64))).unwrap_or_else(|| tr!("Zdroj", "Source").to_string());
            let mut list: Vec<(f32, String)> = vec![(0.0, src_lbl)];
            let others: &[f32] = if adv { &[15.0, 23.976, 24.0, 25.0, 29.97, 30.0, 50.0, 59.94, 60.0, 120.0] } else { &[24.0, 30.0, 60.0] };
            for f in others {
                if sfps.map_or(false, |s| (s - f).abs() < 0.01) {
                    continue;
                }
                list.push((*f, fps_text(*f as f64)));
            }
            for (v, l) in &list {
                if adv {
                    if preset_btn(ui, l) {
                        o.fps = *v;
                    }
                } else if chip(ui, (o.fps - v).abs() < 0.001, l).clicked() {
                    o.fps = *v;
                }
            }
            if o.fps != 0.0 {
                if let Some(v) = sv {
                    hint(ui, format!("{} fps", fps_text(v.fps)));
                }
            }
        });
        if adv {
            egui::CollapsingHeader::new(tr!("Pokročilé video", "Advanced video")).id_salt("adv-video").show(ui, |ui| {
                if si.avx512 {
                    row_t(ui, tr!("Instrukce CPU", "CPU instructions"), tr!("AVX-512 zrychlí enkódování HEVC (x265) a AV1 (SVT-AV1) na CPU, které je podporuje. Na starších Intelech (Skylake-X) může být pomalejší.", "AVX-512 speeds up CPU encoding of HEVC (x265) and AV1 (SVT-AV1) on supporting CPUs. It can be slower on older Intel CPUs (Skylake-X)."), |ui| {
                        ui.checkbox(&mut o.avx512, "AVX-512");
                    });
                }
                row_t(ui, tr!("Interval klíčových snímků", "Keyframe interval (GOP)"), tr!("Vzdálenost mezi klíčovými snímky (ve snímcích). Kratší = snazší přetáčení, větší soubor. 0 = výchozí.", "Distance between keyframes (in frames). Shorter = easier seeking, larger file. 0 = default."), |ui| {
                    dv_u32(ui, &mut o.gop, 0..=100_000, tr!("snímků", "frames"), "Auto", 1.0);
                });
                row_t(ui, tr!("B-snímky (CPU)", "B-frames (CPU)"), tr!("Počet B-snímků mezi referenčními. -1 = výchozí enkodéru.", "Number of B-frames between reference frames. -1 = encoder default."), |ui| {
                    ui.add(egui::DragValue::new(&mut o.bframes).range(-1..=16));
                });
                row_t(ui, tr!("Profil / level", "Profile / level"), tr!("Např. high, main10 a 4.1, 5.1. Prázdné = automaticky.", "E.g. high, main10 and 4.1, 5.1. Empty = automatic."), |ui| {
                    ui.add(egui::TextEdit::singleline(&mut o.vprofile).hint_text("high, main10…").desired_width(120.0));
                    ui.add(egui::TextEdit::singleline(&mut o.vlevel).hint_text("4.1, 5.1…").desired_width(90.0));
                });
            });
        }
    }

    section(ui, tr!("Zvuk", "Audio"));
    audio_rows(ui, o, ext, si, probe, true, adv);
    section(ui, tr!("Ostatní", "Other"));
    trim_rows(ui, o);
    row_t(ui, tr!("Titulky", "Subtitles"), tr!("Zachovat titulky v MKV/MP4, případně je natrvalo vypálit do obrazu ze souboru .srt/.ass.", "Keep subtitles in MKV/MP4, or burn them permanently into the picture from an .srt/.ass file."), |ui| {
        ui.checkbox(&mut o.keep_subs, tr!("Zachovat", "Keep"));
        if adv {
            ui.add(egui::TextEdit::singleline(&mut o.burn_sub).hint_text(tr!("vypálit ze souboru (.srt/.ass)…", "burn in from file (.srt/.ass)…")).desired_width(260.0));
            if ui.button(ph::FOLDER_OPEN).clicked() {
                if let Some(f) = rfd::FileDialog::new().add_filter(tr!("Titulky", "Subtitles"), &["srt", "ass", "ssa", "vtt"]).pick_file() {
                    o.burn_sub = f.to_string_lossy().to_string();
                }
            }
        }
    });
    row_t(ui, "Metadata", tr!("Odstraní tagy, kapitoly a další metadata ze souboru.", "Removes tags, chapters and other metadata from the file."), |ui| {
        ui.checkbox(&mut o.strip_meta, tr!("Odstranit metadata a kapitoly", "Remove metadata and chapters"));
    });
}

pub fn extra_args_row(ui: &mut Ui, o: &mut Opts) {
    row_t(ui, tr!("Vlastní ffmpeg argumenty", "Custom ffmpeg arguments"), tr!("Pro experty: další volby ffmpegu, které se přidají těsně před výstupní soubor.", "For experts: extra ffmpeg options appended right before the output file."), |ui| {
        ui.add(egui::TextEdit::singleline(&mut o.extra_args).hint_text("-metadata title=\"Hello\" -vf eq=contrast=1.2").desired_width(520.0));
    });
}

pub fn panel(ui: &mut Ui, kind: Kind, fmt: &Fmt, in_ext: &str, o: &mut Opts, si: &SysInfo, probe: Option<&Probe>, adv: &mut bool) {
    mode_switch(ui, adv);
    let adv = *adv;
    match fmt.class {
        Class::Video => video_rows(ui, o, &fmt.ext, si, probe, adv),
        Class::Audio => {
            source_strip(ui, probe);
            audio_rows(ui, o, &fmt.ext, si, probe, false, adv);
            trim_rows(ui, o);
            if adv {
                row_t(ui, "Metadata", tr!("Odstraní tagy a obal alba.", "Removes tags and cover art."), |ui| {
                    ui.checkbox(&mut o.strip_meta, tr!("Odstranit tagy a obal alba", "Remove tags and cover art"));
                });
            }
        }
        Class::AnimImage => {
            source_strip(ui, probe);
            row_t(ui, tr!("FPS / šířka", "FPS / width"), tr!("Snímková frekvence a šířka animace. Menší hodnoty = menší soubor.", "Animation frame rate and width. Smaller values = smaller file."), |ui| {
                ui.add(egui::Slider::new(&mut o.anim_fps, 5..=30).text("fps"));
                ui.add(egui::Slider::new(&mut o.anim_width, 120..=1920).suffix(" px"));
            });
            if fmt.ext == "webp" {
                row_t(ui, tr!("Kvalita", "Quality"), "", |ui| {
                    ui.add(egui::Slider::new(&mut o.img_quality, 1..=100));
                });
            }
            trim_rows(ui, o);
        }
        Class::FrameImage => {
            source_strip(ui, probe);
            row_t(ui, tr!("Čas snímku", "Frame time"), tr!("V kolikáté sekundě videa se snímek uloží.", "At which second of the video the frame is taken."), |ui| {
                dv_f32(ui, &mut o.frame_time, 0.0..=100000.0, "s", "", 0.1);
            });
            row_t(ui, tr!("Rozměry", "Size"), tr!("Nechej 'Zdroj' pro původní velikost.", "Leave 'Source' for the original size."), |ui| {
                let pv = probe.and_then(|p| p.video());
                dv_u32_src(ui, &mut o.width, 0..=16384, "px", pv.map(|v| v.width), tr!("Zdroj", "Source"), 2.0);
                ui.label("×");
                dv_u32_src(ui, &mut o.height, 0..=16384, "px", pv.map(|v| v.height), tr!("Zdroj", "Source"), 2.0);
            });
            row_t(ui, tr!("Kvalita", "Quality"), "", |ui| {
                ui.add(egui::Slider::new(&mut o.img_quality, 1..=100));
            });
        }
        Class::Image | Class::Pdf if kind == Kind::Image => {
            let lossy = ["jpg", "jpeg", "webp", "avif", "heic", "jxl", "jp2", "pdf"].contains(&fmt.ext.as_str());
            if lossy {
                row_t(ui, tr!("Kvalita", "Quality"), tr!("Vyšší = lepší obraz a větší soubor. 85–92 je dobrý kompromis.", "Higher = better picture and a bigger file. 85–92 is a good compromise."), |ui| {
                    ui.add(egui::Slider::new(&mut o.img_quality, 1..=100));
                });
            }
            row_t(ui, tr!("Změna velikosti", "Resize"), tr!("Procenta původní velikosti. 0 = beze změny.", "Percent of the original size. 0 = unchanged."), |ui| {
                dv_u32_src(ui, &mut o.resize_pct, 0..=400, "%", Some(100), tr!("Zdroj", "Source"), 1.0);
                ui.add(egui::Slider::new(&mut o.resize_pct, 0..=400).show_value(false));
            });
            if o.resize_pct == 0 && adv {
                row_t(ui, tr!("Max. rozměry", "Max size"), tr!("Zmenší obrázek tak, aby se vešel do těchto rozměrů (zvětšovat nebude). 0 = bez omezení.", "Shrinks the image to fit these dimensions (never enlarges). 0 = no limit."), |ui| {
                    dv_u32(ui, &mut o.max_w, 0..=20000, "px", "Auto", 5.0);
                    ui.label("×");
                    dv_u32(ui, &mut o.max_h, 0..=20000, "px", "Auto", 5.0);
                });
            }
            if ["jpg", "bmp", "ppm", "pgm", "pbm", "pnm", "pcx", "wbmp"].contains(&fmt.ext.as_str()) {
                row_t(ui, tr!("Pozadí (průhlednost)", "Background (transparency)"), tr!("Tenhle formát nemá průhlednost – průhledná místa se vyplní touto barvou.", "This format has no transparency – transparent areas are filled with this colour."), |ui| {
                    egui::ComboBox::from_id_salt("bg").selected_text(o.img_bg.clone()).show_ui(ui, |ui| {
                        for c in ["white", "black", "gray"] {
                            ui.selectable_value(&mut o.img_bg, c.to_string(), c);
                        }
                    });
                });
            }
            if adv && (matches!(in_ext, "svg" | "svgz") || fmt.ext == "pdf") {
                row_t(ui, "DPI", tr!("Rozlišení při vykreslování vektorové grafiky.", "Resolution used to render vector graphics."), |ui| {
                    ui.add(egui::Slider::new(&mut o.dpi, 72..=600));
                });
            }
            if adv {
                row_t(ui, "Metadata", tr!("Odstraní EXIF a barevné profily.", "Removes EXIF and colour profiles."), |ui| {
                    ui.checkbox(&mut o.strip_meta, tr!("Odstranit EXIF a profily", "Remove EXIF and profiles"));
                });
            }
        }
        Class::Image | Class::Text => {
            row_t(ui, tr!("Rozlišení (DPI)", "Resolution (DPI)"), tr!("Hustota vykreslení stránek PDF. 150 stačí na obrazovku, 300 na tisk.", "Render density of the PDF pages. 150 is enough for screens, 300 for print."), |ui| {
                ui.add(egui::Slider::new(&mut o.dpi, 36..=600));
            });
            if fmt.class == Class::Image && ["jpg", "webp", "avif", "heic", "jxl"].contains(&fmt.ext.as_str()) {
                row_t(ui, tr!("Kvalita", "Quality"), "", |ui| {
                    ui.add(egui::Slider::new(&mut o.img_quality, 1..=100));
                });
            }
            row_t(ui, tr!("Heslo PDF", "PDF password"), tr!("Jen pokud je PDF zamčené.", "Only if the PDF is locked."), |ui| {
                ui.add(egui::TextEdit::singleline(&mut o.password).password(true).desired_width(200.0));
            });
        }
        Class::Doc | Class::Pdf => {
            if fmt.class == Class::Pdf {
                row_t(ui, tr!("Papír / písmo", "Paper / font"), tr!("Velikost stránky a písma ve výsledném PDF.", "Page size and font size in the resulting PDF."), |ui| {
                    egui::ComboBox::from_id_salt("paper").selected_text(o.paper.clone()).show_ui(ui, |ui| {
                        for c in ["a4", "a5", "a3", "us-letter", "us-legal"] {
                            ui.selectable_value(&mut o.paper, c.to_string(), c);
                        }
                    });
                    ui.add(egui::Slider::new(&mut o.font_pt, 8..=18).suffix(" pt"));
                });
            }
            row_t(ui, tr!("Obsah", "Contents"), tr!("Vloží automatický obsah (TOC) podle nadpisů.", "Adds an automatic table of contents based on headings."), |ui| {
                ui.checkbox(&mut o.toc, tr!("Vložit obsah (TOC)", "Add table of contents"));
            });
        }
        Class::Archive => {
            if fmt.extra != "extract" {
                row_t(ui, tr!("Komprese", "Compression"), tr!("0 = bez komprese (nejrychlejší), 9 = nejmenší soubor.", "0 = no compression (fastest), 9 = smallest file."), |ui| {
                    ui.add(egui::Slider::new(&mut o.arc_level, 0..=9));
                });
            }
            if ["zip", "7z"].contains(&fmt.ext.as_str()) || kind == Kind::Archive {
                row_t(ui, tr!("Heslo", "Password"), tr!("ZIP/7z: šifrování AES-256. Při rozbalování zadej heslo archivu.", "ZIP/7z: AES-256 encryption. When extracting, enter the archive's password."), |ui| {
                    ui.add(egui::TextEdit::singleline(&mut o.password).password(true).desired_width(200.0));
                });
            }
        }
        Class::Subtitle => {
            ui.label(muted(tr!("Titulky se převádějí bez dalšího nastavení.", "Subtitles are converted without further settings.")).size(12.5));
        }
    }
    if adv && matches!(fmt.class, Class::Video | Class::Audio | Class::AnimImage | Class::FrameImage | Class::Subtitle) {
        section(ui, tr!("Expertní", "Expert"));
        extra_args_row(ui, o);
    }
}
