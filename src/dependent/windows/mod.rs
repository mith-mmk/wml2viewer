use crate::dependent::normalize_locale_tag;
use std::path::PathBuf;
use std::process::Command;
use std::os::windows::process::CommandExt;
use std::thread;
use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
};
use windows::Win32::UI::Shell::{
    FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog,
    SIGDN_FILESYSPATH,
};

const LOCALE_NAME_MAX_LENGTH: i32 = 85;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[link(name = "Kernel32")]
unsafe extern "system" {
    fn GetUserDefaultLocaleName(locale_name: *mut u16, locale_name_count: i32) -> i32;
}

pub fn system_locale() -> Option<String> {
    let mut buf = [0_u16; LOCALE_NAME_MAX_LENGTH as usize];
    let len = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), LOCALE_NAME_MAX_LENGTH) };
    if len > 1 {
        return String::from_utf16(&buf[..(len as usize - 1)])
            .ok()
            .map(|locale| normalize_locale_tag(Some(&locale)));
    }

    std::env::var("LC_ALL")
        .ok()
        .or_else(|| std::env::var("LANG").ok())
        .or_else(|| std::env::var("LANGUAGE").ok())
        .map(|locale| normalize_locale_tag(Some(&locale)))
}

pub fn locale_font_candidates(locale: &str) -> Vec<PathBuf> {
    let mut names = Vec::new();
    if locale.starts_with("ja") {
        names.extend([
            "YuGothM.ttc",
            "YuGothR.ttc",
            "YuGothB.ttc",
            "meiryo.ttc",
            "msgothic.ttc",
            "NotoSansJP-Regular.otf",
            "NotoSansCJK-Regular.ttc",
        ]);
    } else if locale.starts_with("zh") {
        names.extend([
            "msjh.ttc",
            "msyh.ttc",
            "NotoSansTC-Regular.otf",
            "NotoSansCJK-Regular.ttc",
        ]);
    } else if locale.starts_with("ko") {
        names.extend(["malgun.ttf", "NotoSansCJK-Regular.ttc"]);
    }
    resolve_font_candidates(&names)
}

pub fn emoji_font_candidates() -> Vec<PathBuf> {
    resolve_font_candidates(&["seguiemj.ttf", "seguisym.ttf"])
}

pub fn last_resort_font_candidates() -> Vec<PathBuf> {
    resolve_font_candidates(&[
        "segoeui.ttf",
        "seguisym.ttf",
        "arialuni.ttf",
        "arial.ttf",
        "consola.ttf",
    ])
}

fn resolve_font_candidates(file_names: &[&str]) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for root in windows_font_roots() {
        for name in file_names {
            paths.push(root.join(name));
        }
    }
    paths
}

fn windows_font_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        roots.push(
            PathBuf::from(local)
                .join("Microsoft")
                .join("Windows")
                .join("Fonts"),
        );
    }
    if let Some(windir) = std::env::var_os("WINDIR") {
        roots.push(PathBuf::from(windir).join("Fonts"));
    } else {
        roots.push(PathBuf::from(r"C:\Windows\Fonts"));
    }
    roots
}

const PROG_ID: &str = "wml2viewer.image";
const APPLICATION_KEY: &str = r"Applications\wml2viewer.exe";
const ASSOCIATED_EXTENSIONS: &[&str] = &[
    ".webp", ".jpg", ".jpeg", ".bmp", ".gif", ".png", ".tif", ".tiff", ".mag", ".mki", ".pi",
    ".pic", ".zip", ".wmltxt",
];

pub fn available_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for letter in b'A'..=b'Z' {
        let drive = format!("{}:\\", letter as char);
        let path = PathBuf::from(&drive);
        if path.exists() {
            roots.push(path);
        }
    }

    if let Some(home) = std::env::var_os("USERPROFILE") {
        let home = PathBuf::from(home);
        if !roots.iter().any(|root| root == &home) {
            roots.push(home);
        }
    }

    roots
}

pub fn register_file_associations(
    exe_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (classes, _) = hkcu.create_subkey(r"Software\Classes")?;
    let command_line = format!("\"{}\" \"%1\"", exe_path.display());

    let (prog_id, _) = classes.create_subkey(PROG_ID)?;
    prog_id.set_value("", &"wml2viewer image")?;
    let (default_icon, _) = prog_id.create_subkey("DefaultIcon")?;
    default_icon.set_value("", &format!("{},0", exe_path.display()))?;
    let (prog_command, _) = prog_id.create_subkey(r"shell\open\command")?;
    prog_command.set_value("", &command_line)?;

    let (app_root, _) = classes.create_subkey(APPLICATION_KEY)?;
    let (supported_types, _) = app_root.create_subkey("SupportedTypes")?;
    let (open_command, _) = app_root.create_subkey(r"shell\open\command")?;
    open_command.set_value("", &command_line)?;

    for ext in ASSOCIATED_EXTENSIONS {
        supported_types.set_value(*ext, &"")?;

        let (open_with_progids, _) = classes.create_subkey(format!(r"{ext}\OpenWithProgids"))?;
        open_with_progids.set_value(PROG_ID, &"")?;

        let (open_with_list, _) = classes.create_subkey(format!(r"{ext}\OpenWithList"))?;
        open_with_list.set_value("wml2viewer.exe", &"")?;
    }

    Ok(())
}

pub fn clean_file_associations() -> Result<(), Box<dyn std::error::Error>> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let classes = hkcu.open_subkey_with_flags(r"Software\Classes", KEY_READ | KEY_WRITE)?;

    let _ = classes.delete_subkey_all(PROG_ID);
    let _ = classes.delete_subkey_all(APPLICATION_KEY);

    for ext in ASSOCIATED_EXTENSIONS {
        if let Ok(key) =
            classes.open_subkey_with_flags(format!(r"{ext}\OpenWithProgids"), KEY_READ | KEY_WRITE)
        {
            let _ = key.delete_value(PROG_ID);
        }
        if let Ok(key) =
            classes.open_subkey_with_flags(format!(r"{ext}\OpenWithList"), KEY_READ | KEY_WRITE)
        {
            let _ = key.delete_value("wml2viewer.exe");
        }
    }

    Ok(())
}

pub fn pick_directory_dialog() -> Option<PathBuf> {
    thread::spawn(native_pick_directory_dialog)
        .join()
        .ok()
        .flatten()
}

fn native_pick_directory_dialog() -> Option<PathBuf> {
    unsafe {
        let init_result =
            CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
        let needs_uninit = init_result.is_ok();

        let result = (|| {
            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
            let options = dialog.GetOptions().ok()?;
            dialog
                .SetOptions(options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST)
                .ok()?;
            dialog.Show(None).ok()?;

            let item = dialog.GetResult().ok()?;
            let display_name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
            let path = display_name.to_string().ok().map(PathBuf::from);
            CoTaskMemFree(Some(display_name.0 as _));
            path
        })();

        if needs_uninit {
            CoUninitialize();
        }

        result
    }
}

#[allow(dead_code)]
pub fn download_url_to_temp(url: &str) -> Option<PathBuf> {
    let temp_path = std::env::temp_dir().join(format!(
        "wml2viewer_url_{}.bin",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_nanos()
    ));
    let script = format!(
        "$ProgressPreference='SilentlyContinue'; Invoke-WebRequest -Uri '{}' -OutFile '{}'; Write-Output '{}'",
        url.replace('\'', "''"),
        temp_path.display().to_string().replace('\'', "''"),
        temp_path.display().to_string().replace('\'', "''")
    );
    let mut command = Command::new("powershell");
    command
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW);
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(temp_path)
}
