use serde::Serialize;
use sysinfo::System;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemInfo {
    operating_system: String,
    os_version: String,
    host_name: String,
    architecture: String,
    total_memory_mb: u64,
}

#[tauri::command]
fn get_system_info() -> Result<SystemInfo, String> {
    info!("Collecting system information");

    let mut system = System::new_all().map_err(|error| error.to_string())?;
    system.refresh_all();

    Ok(SystemInfo {
        operating_system: System::name().unwrap_or_else(|_| "Unknown".to_string()),
        os_version: System::os_version().unwrap_or_else(|_| "Unknown".to_string()),
        host_name: System::host_name().unwrap_or_else(|_| "Unknown".to_string()),
        architecture: System::cpu_arch(),
        total_memory_mb: system.total_memory() / 1024 / 1024,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();

    info!("Starting C-3PO");

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_system_info])
        .run(tauri::generate_context!())
        .expect("failed to run C-3PO");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_information_is_available() {
        let info = get_system_info().expect("system information should be available");

        assert!(!info.operating_system.is_empty());
        assert!(!info.architecture.is_empty());
        assert!(info.total_memory_mb > 0);
    }
}
