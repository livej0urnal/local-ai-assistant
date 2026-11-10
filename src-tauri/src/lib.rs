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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeCapability {
    id: String,
    available: bool,
    permission_required: bool,
    state: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EnvironmentReport {
    system: SystemInfo,
    platform: String,
    family: String,
    capabilities: Vec<NativeCapability>,
}

fn collect_system_info() -> Result<SystemInfo, String> {
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

#[tauri::command]
fn get_system_info() -> Result<SystemInfo, String> {
    info!("Collecting system information");

    collect_system_info()
}

#[tauri::command]
fn get_environment_report() -> Result<EnvironmentReport, String> {
    info!("Collecting environment and native capability information");

    let desktop = matches!(std::env::consts::OS, "macos" | "windows" | "linux");

    Ok(EnvironmentReport {
        system: collect_system_info()?,
        platform: std::env::consts::OS.to_string(),
        family: std::env::consts::FAMILY.to_string(),
        capabilities: vec![
            NativeCapability {
                id: "system-info".to_string(),
                available: true,
                permission_required: false,
                state: "available".to_string(),
            },
            NativeCapability {
                id: "microphone".to_string(),
                available: desktop,
                permission_required: true,
                state: "requires-runtime-check".to_string(),
            },
            NativeCapability {
                id: "files".to_string(),
                available: desktop,
                permission_required: true,
                state: "on-demand".to_string(),
            },
            NativeCapability {
                id: "screen-capture".to_string(),
                available: desktop,
                permission_required: true,
                state: "not-requested".to_string(),
            },
            NativeCapability {
                id: "accessibility".to_string(),
                available: matches!(std::env::consts::OS, "macos" | "windows"),
                permission_required: true,
                state: "not-requested".to_string(),
            },
        ],
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();

    info!("Starting C-3PO");

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_system_info,
            get_environment_report
        ])
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

    #[test]
    fn environment_report_matches_build_target() {
        let report = get_environment_report().expect("environment report should be available");

        assert_eq!(report.platform, std::env::consts::OS);
        assert_eq!(report.family, std::env::consts::FAMILY);
        assert!(report.capabilities.iter().any(|item| item.id == "microphone"));
    }
}
