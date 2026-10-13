//! Integrace se systémem: spuštění s počítačem, "Otevřít v programu", kontextové menu, zástupce ve Startu.
//! Windows: registr (HKCU, přes reg.exe) + .lnk. Linux: .desktop soubory. macOS: LaunchAgent.
#[allow(unused_imports)]
use std::path::{Path, PathBuf};

fn exe() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| e.to_string())
}

// ───────────────────────────── Windows ─────────────────────────────
#[cfg(windows)]
mod win {
    use super::*;

    pub fn reg(args: &[&str]) -> Result<(), String> {
        let out = crate::util::command(Path::new("reg.exe")).args(args).output().map_err(|e| format!("reg.exe: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }
    pub fn reg_delete(key: &str) {
        let _ = reg(&["delete", key, "/f"]);
    }
    pub fn lnk_path() -> Option<PathBuf> {
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Microsoft").join("Windows").join("Start Menu").join("Programs").join("TomasekConvert.lnk"))
    }
}

// ───────────────────────────── Linux ─────────────────────────────
#[cfg(all(unix, not(target_os = "macos")))]
mod lin {
    use super::*;
    pub fn home() -> PathBuf {
        PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
    }
    pub fn data_home() -> PathBuf {
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".local/share"))
    }
    pub fn config_home() -> PathBuf {
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".config"))
    }
    pub fn icon_path() -> PathBuf {
        data_home().join("icons/hicolor/256x256/apps/tomasekconvert.png")
    }
    pub fn desktop_file(exec: &str, with_files: bool) -> String {
        format!(
            "[Desktop Entry]\nType=Application\nName=TomasekConvert\nComment=Local file converter\nExec={exec}{}\nIcon=tomasekconvert\nTerminal=false\nCategories=AudioVideo;Utility;\nMimeType=video/*;audio/*;image/*;application/pdf;application/zip;application/x-7z-compressed;application/x-tar;text/markdown;text/html;text/plain;application/vnd.openxmlformats-officedocument.wordprocessingml.document;application/epub+zip;\nStartupWMClass=TomasekConvert\n",
            if with_files { " %F" } else { "" }
        )
    }
    pub fn write_icon() {
        let p = icon_path();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(p, include_bytes!("../assets/icon.png"));
    }
}

// ───────────────────────────── API ─────────────────────────────

pub fn shortcut_exists() -> bool {
    #[cfg(windows)]
    {
        win::lnk_path().map(|p| p.exists()).unwrap_or(false)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        lin::data_home().join("applications/TomasekConvert.desktop").exists()
    }
    #[cfg(target_os = "macos")]
    {
        false
    }
}

/// Zástupce / záznam v nabídce aplikací, aby šla appka najít ve vyhledávání systému.
pub fn create_start_shortcut() -> Result<PathBuf, String> {
    let exe = exe()?;
    #[cfg(windows)]
    {
        let dir = exe.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let lnk = win::lnk_path().ok_or("APPDATA is not set")?;
        if let Some(p) = lnk.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        let q = |p: &Path| p.to_string_lossy().replace('\'', "''");
        let script = format!(
            "$ErrorActionPreference='Stop'; $s=(New-Object -ComObject WScript.Shell).CreateShortcut('{lnk}'); $s.TargetPath='{exe}'; $s.WorkingDirectory='{dir}'; $s.IconLocation='{exe},0'; $s.Description='TomasekConvert'; $s.Save()",
            lnk = q(&lnk),
            exe = q(&exe),
            dir = q(&dir)
        );
        let out = crate::util::command(Path::new("powershell.exe"))
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &script])
            .output()
            .map_err(|e| format!("PowerShell: {e}"))?;
        if out.status.success() && lnk.exists() {
            Ok(lnk)
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        lin::write_icon();
        let dir = lin::data_home().join("applications");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let f = dir.join("TomasekConvert.desktop");
        std::fs::write(&f, lin::desktop_file(&format!("\"{}\"", exe.display()), true)).map_err(|e| e.to_string())?;
        let _ = std::process::Command::new("update-desktop-database").arg(&dir).output();
        Ok(f)
    }
    #[cfg(target_os = "macos")]
    {
        let _ = exe;
        Err("On macOS install the app bundle (.app / .dmg) – it is already searchable in Spotlight.".into())
    }
}

pub fn remove_start_shortcut() -> Result<(), String> {
    #[cfg(windows)]
    {
        if let Some(p) = win::lnk_path() {
            if p.exists() {
                return std::fs::remove_file(p).map_err(|e| e.to_string());
            }
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let f = lin::data_home().join("applications/TomasekConvert.desktop");
        if f.exists() {
            return std::fs::remove_file(f).map_err(|e| e.to_string());
        }
    }
    Ok(())
}

/// Spuštění s přihlášením do systému (volitelně rovnou na pozadí v tray: `--tray`).
pub fn set_autostart(enabled: bool, hidden: bool) -> Result<(), String> {
    let exe = exe()?;
    #[cfg(windows)]
    {
        const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
        if enabled {
            let cmd = format!("\"{}\"{}", exe.display(), if hidden { " --tray" } else { "" });
            win::reg(&["add", KEY, "/v", "TomasekConvert", "/t", "REG_SZ", "/d", &cmd, "/f"])
        } else {
            let _ = win::reg(&["delete", KEY, "/v", "TomasekConvert", "/f"]);
            Ok(())
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let f = lin::config_home().join("autostart/TomasekConvert.desktop");
        if enabled {
            lin::write_icon();
            std::fs::create_dir_all(f.parent().unwrap()).map_err(|e| e.to_string())?;
            let exec = format!("\"{}\"{}", exe.display(), if hidden { " --tray" } else { "" });
            std::fs::write(&f, lin::desktop_file(&exec, false)).map_err(|e| e.to_string())
        } else {
            let _ = std::fs::remove_file(f);
            Ok(())
        }
    }
    #[cfg(target_os = "macos")]
    {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
        let f = home.join("Library/LaunchAgents/com.tomasekconvert.app.plist");
        if enabled {
            std::fs::create_dir_all(f.parent().unwrap()).map_err(|e| e.to_string())?;
            let args = if hidden { format!("<string>{}</string><string>--tray</string>", exe.display()) } else { format!("<string>{}</string>", exe.display()) };
            let plist = format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>Label</key><string>com.tomasekconvert.app</string><key>ProgramArguments</key><array>{args}</array><key>RunAtLoad</key><true/></dict></plist>\n"
            );
            std::fs::write(&f, plist).map_err(|e| e.to_string())
        } else {
            let _ = std::fs::remove_file(f);
            Ok(())
        }
    }
}

/// Záznam v seznamu "Otevřít v programu".
pub fn set_open_with(enabled: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        let exe = exe()?;
        const APP: &str = r"HKCU\Software\Classes\Applications\TomasekConvert.exe";
        if enabled {
            let cmd = format!("\"{}\" \"%1\"", exe.display());
            win::reg(&["add", APP, "/v", "FriendlyAppName", "/d", "TomasekConvert", "/f"])?;
            win::reg(&["add", &format!(r"{APP}\shell\open\command"), "/ve", "/d", &cmd, "/f"])
        } else {
            win::reg_delete(APP);
            Ok(())
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // záznam v nabídce aplikací obsahuje MimeType + %F, takže stačí zástupce
        if enabled {
            create_start_shortcut().map(|_| ())
        } else {
            Ok(())
        }
    }
    #[cfg(target_os = "macos")]
    {
        let _ = enabled;
        Err("On macOS “Open With” works through the installed app bundle (.app).".into())
    }
}

/// Položka "Převést v TomasekConvert" v kontextovém menu souborů a složek.
pub fn set_context_menu(enabled: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        let exe = exe()?;
        let cmd = format!("\"{}\" \"%1\"", exe.display());
        let icon = format!("\"{}\",0", exe.display());
        for base in [r"HKCU\Software\Classes\*\shell\TomasekConvert", r"HKCU\Software\Classes\Directory\shell\TomasekConvert"] {
            if enabled {
                win::reg(&["add", base, "/ve", "/d", crate::tr!("Převést v TomasekConvert", "Convert with TomasekConvert"), "/f"])?;
                win::reg(&["add", base, "/v", "Icon", "/d", &icon, "/f"])?;
                win::reg(&["add", &format!(r"{base}\command"), "/ve", "/d", &cmd, "/f"])?;
            } else {
                win::reg_delete(base);
            }
        }
        Ok(())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let exe = exe()?;
        let nautilus = lin::data_home().join("nautilus/scripts/Convert with TomasekConvert");
        let dolphin = lin::data_home().join("kio/servicemenus/tomasekconvert.desktop");
        if enabled {
            if let Some(d) = nautilus.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            std::fs::write(&nautilus, format!("#!/bin/sh\nexec \"{}\" \"$@\"\n", exe.display())).map_err(|e| e.to_string())?;
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&nautilus, std::fs::Permissions::from_mode(0o755));
            }
            if let Some(d) = dolphin.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            std::fs::write(
                &dolphin,
                format!("[Desktop Entry]\nType=Service\nMimeType=all/all;inode/directory;\nActions=convert;\nX-KDE-Submenu=TomasekConvert\n\n[Desktop Action convert]\nName=Convert with TomasekConvert\nIcon=tomasekconvert\nExec=\"{}\" %F\n", exe.display()),
            )
            .map_err(|e| e.to_string())?;
            lin::write_icon();
        } else {
            let _ = std::fs::remove_file(nautilus);
            let _ = std::fs::remove_file(dolphin);
        }
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        let _ = enabled;
        Err("On macOS use Finder → right-click → Open With → TomasekConvert (installed app bundle).".into())
    }
}
