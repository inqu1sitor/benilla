//! Finding a 1.12.1 install for a launcher on top of benilla, before the client starts: the
//! places a player's install usually is, a check that a folder is a 1.12.1 (build 5875) client,
//! and a native folder dialog for when neither finds one. The launcher hands its answer to
//! [`benilla_formats::set_data_override`]; benilla itself still reads only `$WOW_DATA` and the
//! folders beside the binary ([`benilla_formats::candidates`]).

use std::path::{Path, PathBuf};

/// The build every 1.12.1 client is: `WoW.exe`'s file version is `1.12.1.5875`.
pub const BUILD: u16 = 5875;

/// A folder that holds a usable 1.12.1 install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Install {
    /// The install's `Data` folder, which [`benilla_formats::set_data_override`] takes.
    pub data: PathBuf,
    /// The client's build from `WoW.exe`'s version resource, `None` when the folder has no
    /// `WoW.exe` (a copied `Data` folder alone is still readable).
    pub build: Option<u16>,
}

/// Why a folder is not a usable install, in words for the player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotAnInstall(pub String);

impl std::fmt::Display for NotAnInstall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Check that `folder` (the WoW folder or its `Data` folder) is a 1.12.1 install: every base
/// archive present, no archive of a later expansion, and `WoW.exe`, when there is one, at build
/// 5875.
pub fn verify(folder: &Path) -> Result<Install, NotAnInstall> {
    let data = data_folder(folder).ok_or_else(|| {
        NotAnInstall(format!(
            "{} has no Data folder with WoW's archives in it.",
            folder.display()
        ))
    })?;
    let names: Vec<String> = std::fs::read_dir(&data)
        .map_err(|e| NotAnInstall(format!("{} cannot be read: {e}", data.display())))?
        .flatten()
        .filter_map(|e| e.file_name().to_str().map(str::to_owned))
        .collect();
    let has = |want: &str| names.iter().any(|n| n.eq_ignore_ascii_case(want));
    // A later client keeps `common.MPQ` (2.x) or `common-2.MPQ` (3.x) in its `Data` folder.
    if has("common.MPQ") || has("common-2.MPQ") || has("expansion.MPQ") {
        return Err(NotAnInstall(format!(
            "{} is a later World of Warcraft client. Banilla needs version 1.12.1 (build {BUILD}).",
            data.display()
        )));
    }
    let missing: Vec<&str> = benilla_formats::VANILLA_BASE_ORDER
        .iter()
        .copied()
        .filter(|b| !has(b))
        .collect();
    if !missing.is_empty() {
        return Err(NotAnInstall(format!(
            "{} is missing {}. Is this a complete 1.12.1 install?",
            data.display(),
            missing.join(", ")
        )));
    }
    let build = data
        .parent()
        .and_then(find_exe)
        .and_then(|exe| std::fs::read(exe).ok())
        .and_then(|bytes| exe_build(&bytes));
    match build {
        Some((1, 12, 1, b)) if b == BUILD => Ok(Install {
            data,
            build: Some(b),
        }),
        Some((a, b, c, d)) => Err(NotAnInstall(format!(
            "The WoW.exe beside {} is version {a}.{b}.{c} (build {d}). Banilla needs 1.12.1 \
             (build {BUILD}).",
            data.display()
        ))),
        None => Ok(Install { data, build: None }),
    }
}

/// `folder` itself when it holds the base archives, else its `Data` child, matched in any case.
fn data_folder(folder: &Path) -> Option<PathBuf> {
    let holds_archives = |dir: &Path| {
        std::fs::read_dir(dir).is_ok_and(|entries| {
            entries.flatten().any(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case("dbc.MPQ"))
            })
        })
    };
    if holds_archives(folder) {
        return Some(folder.to_path_buf());
    }
    let child = std::fs::read_dir(folder)
        .ok()?
        .flatten()
        .find(|e| {
            e.path().is_dir()
                && e.file_name()
                    .to_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case("Data"))
        })?
        .path();
    holds_archives(&child).then_some(child)
}

/// `WoW.exe` in `dir`, matched in any case.
fn find_exe(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .find(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.eq_ignore_ascii_case("WoW.exe"))
        })
        .map(|e| e.path())
}

/// The file version in a Windows executable's `VS_FIXEDFILEINFO` (signature `0xFEEF04BD`), as
/// `(major, minor, patch, build)`.
pub fn exe_build(bytes: &[u8]) -> Option<(u16, u16, u16, u16)> {
    const SIGNATURE: [u8; 4] = 0xFEEF_04BDu32.to_le_bytes();
    let at = bytes.windows(4).position(|w| w == SIGNATURE)?;
    let word = |off: usize| -> Option<u32> {
        let b = bytes.get(at + off..at + off + 4)?;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    let (ms, ls) = (word(8)?, word(12)?);
    Some(((ms >> 16) as u16, ms as u16, (ls >> 16) as u16, ls as u16))
}

/// Every folder worth checking for an install, most likely first: the registry's `InstallPath`
/// (Windows), the launcher's own folder and its parent, then the usual install folders.
pub fn likely_folders() -> Vec<PathBuf> {
    let mut out = Vec::new();
    out.extend(registry_install_paths());
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        out.push(dir.clone());
        if let Some(parent) = dir.parent() {
            out.push(parent.to_path_buf());
            // A launcher unpacked beside the install: `Games/BanillaCrucible` next to `Games/WoW`.
            out.extend(siblings(parent));
        }
    }
    out.extend(common_folders());
    let mut seen = std::collections::HashSet::new();
    out.retain(|p| seen.insert(p.clone()));
    out
}

/// The first of [`likely_folders`] that [`verify`] accepts.
pub fn detect() -> Option<Install> {
    likely_folders().iter().find_map(|f| verify(f).ok())
}

/// Folders beside `dir` whose name looks like a WoW install.
fn siblings(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter(|e| {
            e.file_name().to_str().is_some_and(|n| {
                let n = n.to_ascii_lowercase();
                n.contains("wow") || n.contains("warcraft") || n.contains("1.12")
            })
        })
        .map(|e| e.path())
        .collect();
    out.sort();
    out
}

/// The usual install folders on this platform.
fn common_folders() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    #[cfg(windows)]
    {
        for var in ["ProgramFiles(x86)", "ProgramFiles", "PUBLIC"] {
            if let Some(v) = std::env::var_os(var) {
                roots.push(PathBuf::from(v));
            }
        }
        for drive in ["C", "D", "E", "F"] {
            roots.push(PathBuf::from(format!("{drive}:\\")));
            roots.push(PathBuf::from(format!("{drive}:\\Games")));
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            roots.push(home.join("Games"));
            roots.push(home.clone());
            roots.push(home.join(".wine/drive_c/Program Files (x86)"));
            roots.push(home.join(".wine/drive_c/Program Files"));
        }
        roots.push(PathBuf::from("/Applications"));
    }
    let names = [
        "World of Warcraft",
        "World of Warcraft 1.12",
        "World of Warcraft 1.12.1",
        "WoW",
        "WoW 1.12",
        "WoW 1.12.1",
        "Wow-1.12.1",
        "Vanilla WoW",
    ];
    let mut out = Vec::new();
    for root in &roots {
        for name in names {
            out.push(root.join(name));
        }
        out.push(
            root.join("Blizzard Entertainment")
                .join("World of Warcraft"),
        );
    }
    out
}

/// The `InstallPath` values Blizzard's installer wrote, 64- and 32-bit views; none off Windows.
fn registry_install_paths() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // `CREATE_NO_WINDOW`: no console flashes behind the launcher.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let keys = [
            r"HKLM\SOFTWARE\Blizzard Entertainment\World of Warcraft",
            r"HKLM\SOFTWARE\WOW6432Node\Blizzard Entertainment\World of Warcraft",
            r"HKCU\SOFTWARE\Blizzard Entertainment\World of Warcraft",
        ];
        keys.iter()
            .filter_map(|key| {
                let out = std::process::Command::new("reg")
                    .args(["query", key, "/v", "InstallPath"])
                    .creation_flags(CREATE_NO_WINDOW)
                    .output()
                    .ok()?;
                parse_reg_query(&String::from_utf8_lossy(&out.stdout))
            })
            .collect()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// The value of a `reg query … /v InstallPath` answer: `    InstallPath    REG_SZ    C:\…`.
#[cfg_attr(not(windows), allow(dead_code))]
fn parse_reg_query(text: &str) -> Option<PathBuf> {
    text.lines().find_map(|l| {
        let rest = l.trim().strip_prefix("InstallPath")?.trim_start();
        let value = rest.strip_prefix("REG_SZ")?.trim();
        (!value.is_empty()).then(|| PathBuf::from(value))
    })
}

/// Ask the player for their install folder with the platform's folder dialog, after a message
/// saying why (`reason`, or a plain request on first launch). Loops until the folder passes
/// [`verify`] or the player cancels (`None`). `title` names the launcher.
pub fn pick(title: &str, reason: Option<&str>) -> Option<Install> {
    let mut why = reason.map(str::to_owned);
    loop {
        let text = match &why {
            Some(why) => format!(
                "{why}\n\nChoose the folder of your World of Warcraft 1.12.1 install (the one with \
                 WoW.exe and the Data folder)."
            ),
            None => "No World of Warcraft 1.12.1 install was found.\n\nChoose the folder of your \
                     install (the one with WoW.exe and the Data folder)."
                .to_owned(),
        };
        let go_on = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Info)
            .set_title(title)
            .set_description(&text)
            .set_buttons(rfd::MessageButtons::OkCancel)
            .show();
        if !matches!(
            go_on,
            rfd::MessageDialogResult::Ok | rfd::MessageDialogResult::Yes
        ) {
            return None;
        }
        let folder = rfd::FileDialog::new()
            .set_title("Choose your World of Warcraft 1.12.1 folder")
            .pick_folder()?;
        match verify(&folder) {
            Ok(install) => return Some(install),
            Err(e) => why = Some(e.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("benilla-install-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fake_install(root: &Path, data: &str) -> PathBuf {
        let data = root.join(data);
        std::fs::create_dir_all(&data).unwrap();
        for b in benilla_formats::VANILLA_BASE_ORDER {
            std::fs::write(data.join(b), b"").unwrap();
        }
        std::fs::write(data.join("patch.MPQ"), b"").unwrap();
        data
    }

    fn exe_with_version(ms: u32, ls: u32) -> Vec<u8> {
        let mut bytes = b"MZ...padding...".to_vec();
        bytes.extend(0xFEEF_04BDu32.to_le_bytes());
        bytes.extend(0x0001_0000u32.to_le_bytes());
        bytes.extend(ms.to_le_bytes());
        bytes.extend(ls.to_le_bytes());
        bytes.extend([0u8; 16]);
        bytes
    }

    #[test]
    fn the_wow_folder_or_its_data_folder_is_an_install() {
        let root = temp("both");
        let data = fake_install(&root, "Data");
        assert_eq!(verify(&root).unwrap().data, data);
        assert_eq!(verify(&data).unwrap().data, data);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_data_folder_in_another_case_is_found() {
        let root = temp("case");
        let data = fake_install(&root, "data");
        assert_eq!(verify(&root).unwrap().data, data);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_missing_archive_is_named() {
        let root = temp("missing");
        let data = fake_install(&root, "Data");
        std::fs::remove_file(data.join("model.MPQ")).unwrap();
        let err = verify(&root).unwrap_err();
        assert!(err.0.contains("model.MPQ"), "{err}");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_later_client_is_refused() {
        let root = temp("later");
        let data = fake_install(&root, "Data");
        std::fs::write(data.join("common.MPQ"), b"").unwrap();
        assert!(verify(&root).unwrap_err().0.contains("later"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_build_comes_from_wow_exe() {
        let root = temp("build");
        fake_install(&root, "Data");
        let good = exe_with_version((1 << 16) | 12, (1 << 16) | 5875);
        std::fs::write(root.join("WoW.exe"), &good).unwrap();
        assert_eq!(verify(&root).unwrap().build, Some(5875));
        let other = exe_with_version((1 << 16) | 12, (1 << 16) | 5595);
        std::fs::write(root.join("WoW.exe"), &other).unwrap();
        let err = verify(&root).unwrap_err();
        assert!(err.0.contains("5595") && err.0.contains("5875"), "{err}");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_folder_without_data_says_so() {
        let root = temp("empty");
        assert!(verify(&root).unwrap_err().0.contains("no Data folder"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn reg_query_output_is_read() {
        let text =
            "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\Blizzard Entertainment\\World of Warcraft\r\n    \
                    InstallPath    REG_SZ    C:\\Games\\World of Warcraft\\\r\n\r\n";
        assert_eq!(
            parse_reg_query(text),
            Some(PathBuf::from("C:\\Games\\World of Warcraft\\"))
        );
        assert_eq!(parse_reg_query("ERROR: not found"), None);
    }

    #[test]
    fn likely_folders_start_beside_the_launcher_and_hold_no_duplicate() {
        let folders = likely_folders();
        let exe_dir = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        assert!(folders.contains(&exe_dir));
        let unique: std::collections::HashSet<_> = folders.iter().collect();
        assert_eq!(unique.len(), folders.len());
    }
}
