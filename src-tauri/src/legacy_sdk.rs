// ponytail: Legacy Samsung Station SDK (v1.0.4 / v1.0.12) integration
use std::ffi::{c_int, c_void};
use std::path::{Path, PathBuf};

#[cfg(target_os = "windows")]
type Initialize = unsafe extern "C" fn();
#[cfg(target_os = "windows")]
type AddMode = unsafe extern "C" fn(c_int) -> c_int;
#[cfg(target_os = "windows")]
type AddDevice = unsafe extern "C" fn(c_int) -> c_int;
#[cfg(target_os = "windows")]
type InstallToken = unsafe extern "C" fn(c_int) -> c_int;
#[cfg(target_os = "windows")]
type RemoveToken = unsafe extern "C" fn() -> c_int;

#[cfg(not(target_os = "windows"))]
type GetInstance = unsafe extern "C" fn() -> *mut c_void;
#[cfg(not(target_os = "windows"))]
type FreeInstance = unsafe extern "C" fn(*mut c_void);
#[cfg(not(target_os = "windows"))]
type AddDevice = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;
#[cfg(not(target_os = "windows"))]
type InstallToken = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;
#[cfg(not(target_os = "windows"))]
type RemoveToken = unsafe extern "C" fn(*mut c_void) -> c_int;

/// Resolve the path to JPCWrapperDll.dll (or libJpClientManager.so) from configured legacy path
pub fn resolve_sdk_path(legacy_path: Option<&str>) -> Option<PathBuf> {
    let mut candidates = Vec::new();

    let mut check_base = |base: &Path| {
        candidates.push(base.join("data").join("flutter_assets").join("assets").join("sdk").join("JPCWrapperDll.dll"));
        candidates.push(base.join("JPCWrapperDll.dll"));
        candidates.push(base.join("data").join("flutter_assets").join("assets").join("sdk").join("libJpClientManager.so"));
        candidates.push(base.join("libJpClientManager.so"));
        candidates.push(base.to_path_buf());

        // Check subdirectories (e.g. extracted Station_windows_v1.0.12 folder)
        if let Ok(entries) = std::fs::read_dir(base) {
            for entry in entries.flatten() {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    let sub = entry.path();
                    candidates.push(sub.join("data").join("flutter_assets").join("assets").join("sdk").join("JPCWrapperDll.dll"));
                    candidates.push(sub.join("JPCWrapperDll.dll"));
                    candidates.push(sub.join("data").join("flutter_assets").join("assets").join("sdk").join("libJpClientManager.so"));
                    candidates.push(sub.join("libJpClientManager.so"));
                }
            }
        }
    };

    if let Some(p) = legacy_path {
        check_base(Path::new(p));
    }

    if let Ok(env_p) = std::env::var("MRT_LEGACY_DIR") {
        check_base(Path::new(&env_p));
    }

    check_base(Path::new(r"C:\windows_v1.0.4"));
    check_base(Path::new(r"C:\Station_windows_v1.0.12"));
    check_base(Path::new(r"C:\Station"));

    candidates.into_iter().find(|p| p.is_file())
}

/// Parse COM port number from port path (e.g. "COM87" -> 87, "/dev/ttyACM0" -> 0)
pub fn parse_port_number(port_str: &str) -> Option<i32> {
    let clean = port_str.trim();
    if let Some(num_str) = clean.strip_prefix("COM").or_else(|| clean.strip_prefix("com")) {
        return num_str.parse::<i32>().ok();
    }
    if let Some(idx) = clean.rfind(|c: char| !c.is_ascii_digit()) {
        let (_, num_str) = clean.split_at(idx + 1);
        if let Ok(n) = num_str.parse::<i32>() {
            return Some(n);
        }
    }
    clean.parse::<i32>().ok()
}

/// Execute token installation using the official Samsung Legacy SDK (JPCWrapperDll)
pub fn direct_install_token(library_path: &Path, port_num: i32) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    if let Some(directory) = library_path.parent() {
        let _ = std::env::set_current_dir(directory);
    }

    let library = unsafe { libloading::Library::new(library_path) }.map_err(|e| {
        format!("Gagal memuat Legacy Station SDK di '{}': {}", library_path.display(), e)
    })?;

    unsafe {
        #[cfg(target_os = "windows")]
        {
            if let Ok(init) = library.get::<Initialize>(b"JPC_initialize\0") {
                init();
            }
            if let Ok(add_mode) = library.get::<AddMode>(b"JPC_add_request_mode\0") {
                for m in &[0, 1, 12, 49] {
                    let _ = add_mode(*m as c_int);
                }
            }
            let add_device: libloading::Symbol<AddDevice> = library
                .get(b"JPC_add_request_device\0")
                .map_err(|e| format!("JPC_add_request_device tidak ditemukan: {}", e))?;
            let install: libloading::Symbol<InstallToken> = library
                .get(b"JPC_install_token\0")
                .map_err(|e| format!("JPC_install_token tidak ditemukan: {}", e))?;

            let res_dev = add_device(port_num as c_int);
            if res_dev != 0 {
                return Err(format!("Legacy JPC_add_request_device gagal (code 0x{:08X})", res_dev));
            }

            let res_install = install(1);
            if res_install != 0 {
                return Err(format!("Legacy JPC_install_token gagal (code 0x{:08X})", res_install));
            }
            Ok(())
        }

        #[cfg(not(target_os = "windows"))]
        {
            let get_instance: libloading::Symbol<GetInstance> = library
                .get(b"JPC_get_instance\0")
                .map_err(|e| format!("JPC_get_instance tidak ditemukan: {}", e))?;
            let free_instance: libloading::Symbol<FreeInstance> = library
                .get(b"JPC_free_instance\0")
                .map_err(|e| format!("JPC_free_instance tidak ditemukan: {}", e))?;
            let add_device: libloading::Symbol<AddDevice> = library
                .get(b"JPC_add_request_device\0")
                .map_err(|e| format!("JPC_add_request_device tidak ditemukan: {}", e))?;
            let install: libloading::Symbol<InstallToken> = library
                .get(b"JPC_install_token\0")
                .map_err(|e| format!("JPC_install_token tidak ditemukan: {}", e))?;

            let instance = get_instance();
            if instance.is_null() {
                return Err("Legacy SDK get_instance mengembalikan null pointer".to_string());
            }

            let res_dev = add_device(instance, port_num as c_int);
            if res_dev != 0 {
                free_instance(instance);
                return Err(format!("Legacy JPC_add_request_device gagal (code 0x{:08X})", res_dev));
            }

            let res_install = install(instance, 1);
            free_instance(instance);
            if res_install != 0 {
                return Err(format!("Legacy JPC_install_token gagal (code 0x{:08X})", res_install));
            }
            Ok(())
        }
    }
}

/// Execute token removal using the official Samsung Legacy SDK (JPCWrapperDll)
pub fn direct_remove_token(library_path: &Path, port_num: i32) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    if let Some(directory) = library_path.parent() {
        let _ = std::env::set_current_dir(directory);
    }

    let library = unsafe { libloading::Library::new(library_path) }.map_err(|e| {
        format!("Gagal memuat Legacy Station SDK di '{}': {}", library_path.display(), e)
    })?;

    unsafe {
        #[cfg(target_os = "windows")]
        {
            if let Ok(init) = library.get::<Initialize>(b"JPC_initialize\0") {
                init();
            }
            let add_device: libloading::Symbol<AddDevice> = library
                .get(b"JPC_add_request_device\0")
                .map_err(|e| format!("JPC_add_request_device tidak ditemukan: {}", e))?;
            let remove: libloading::Symbol<RemoveToken> = library
                .get(b"JPC_remove_token\0")
                .map_err(|e| format!("JPC_remove_token tidak ditemukan: {}", e))?;

            let res_dev = add_device(port_num as c_int);
            if res_dev != 0 {
                return Err(format!("Legacy JPC_add_request_device gagal (code 0x{:08X})", res_dev));
            }

            let res_remove = remove();
            if res_remove != 0 {
                return Err(format!("Legacy JPC_remove_token gagal (code 0x{:08X})", res_remove));
            }
            Ok(())
        }

        #[cfg(not(target_os = "windows"))]
        {
            let get_instance: libloading::Symbol<GetInstance> = library
                .get(b"JPC_get_instance\0")
                .map_err(|e| format!("JPC_get_instance tidak ditemukan: {}", e))?;
            let free_instance: libloading::Symbol<FreeInstance> = library
                .get(b"JPC_free_instance\0")
                .map_err(|e| format!("JPC_free_instance tidak ditemukan: {}", e))?;
            let add_device: libloading::Symbol<AddDevice> = library
                .get(b"JPC_add_request_device\0")
                .map_err(|e| format!("JPC_add_request_device tidak ditemukan: {}", e))?;
            let remove: libloading::Symbol<RemoveToken> = library
                .get(b"JPC_remove_token\0")
                .map_err(|e| format!("JPC_remove_token tidak ditemukan: {}", e))?;

            let instance = get_instance();
            if instance.is_null() {
                return Err("Legacy SDK get_instance mengembalikan null pointer".to_string());
            }

            let res_dev = add_device(instance, port_num as c_int);
            if res_dev != 0 {
                free_instance(instance);
                return Err(format!("Legacy JPC_add_request_device gagal (code 0x{:08X})", res_dev));
            }

            let res_remove = remove(instance);
            free_instance(instance);
            if res_remove != 0 {
                return Err(format!("Legacy JPC_remove_token gagal (code 0x{:08X})", res_remove));
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_port_number() {
        assert_eq!(parse_port_number("COM87"), Some(87));
        assert_eq!(parse_port_number("com6"), Some(6));
        assert_eq!(parse_port_number("/dev/ttyACM0"), Some(0));
        assert_eq!(parse_port_number("87"), Some(87));
        assert_eq!(parse_port_number("invalid"), None);
    }
}

