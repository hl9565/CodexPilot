use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LaunchPreferences {
    pub(crate) app_path: String,
    pub(crate) debug_port: u16,
    pub(crate) helper_port: u16,
    #[serde(default)]
    pub(crate) auto_launch_on_open: bool,
    #[serde(default)]
    pub(crate) auto_sync_sessions_on_launch: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EnhancementSettings {
    #[serde(default = "default_true")]
    pub(crate) enabled: bool,
    #[serde(default = "default_true")]
    pub(crate) timeline: bool,
    #[serde(default = "default_true")]
    pub(crate) inline_actions: bool,
    #[serde(default = "default_true")]
    pub(crate) scroll_restore: bool,
    #[serde(default = "default_true")]
    pub(crate) plugin_entry_unlock: bool,
    #[serde(default = "default_true")]
    pub(crate) force_plugin_install: bool,
    #[serde(default = "default_true")]
    pub(crate) fast_global_mode: bool,
}

impl Default for EnhancementSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            timeline: true,
            inline_actions: true,
            scroll_restore: true,
            plugin_entry_unlock: true,
            force_plugin_install: true,
            fast_global_mode: true,
        }
    }
}

fn default_true() -> bool {
    true
}

impl Default for LaunchPreferences {
    fn default() -> Self {
        let options = codex_pilot_core::launcher::LaunchOptions::default();
        Self {
            app_path: String::new(),
            debug_port: options.debug_port,
            helper_port: options.helper_port,
            auto_launch_on_open: false,
            auto_sync_sessions_on_launch: false,
        }
    }
}

pub(crate) fn manager_config_path() -> PathBuf {
    codex_pilot_core::app_paths::app_state_dir().join("manager-launch.json")
}

pub(crate) fn enhancement_settings_path() -> PathBuf {
    codex_pilot_core::app_paths::app_state_dir().join("enhancement-settings.json")
}

pub(crate) fn load_launch_preferences() -> LaunchPreferences {
    load_launch_preferences_from_path(&manager_config_path()).unwrap_or_default()
}

pub(crate) fn load_enhancement_settings() -> EnhancementSettings {
    load_enhancement_settings_from_path(&enhancement_settings_path()).unwrap_or_default()
}

pub(crate) fn load_enhancement_settings_from_path(
    path: &Path,
) -> Result<EnhancementSettings, String> {
    if !path.exists() {
        return Ok(EnhancementSettings::default());
    }
    let contents =
        std::fs::read_to_string(path).map_err(|error| format!("读取页面增强设置失败：{error}"))?;
    let settings = serde_json::from_str::<EnhancementSettings>(&contents)
        .map_err(|error| format!("解析页面增强设置失败：{error}"))?;
    Ok(sanitize_enhancement_settings(settings))
}

pub(crate) fn save_enhancement_settings_to_path(
    path: &Path,
    settings: &EnhancementSettings,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("创建配置目录失败：{error}"))?;
    }
    let contents = serde_json::to_string_pretty(&sanitize_enhancement_settings(settings.clone()))
        .map_err(|error| format!("序列化页面增强设置失败：{error}"))?;
    std::fs::write(path, contents).map_err(|error| format!("写入页面增强设置失败：{error}"))
}

pub(crate) fn sanitize_enhancement_settings(settings: EnhancementSettings) -> EnhancementSettings {
    settings
}

pub(crate) fn load_launch_preferences_from_path(path: &Path) -> Result<LaunchPreferences, String> {
    if !path.exists() {
        return Ok(LaunchPreferences::default());
    }
    let contents =
        std::fs::read_to_string(path).map_err(|error| format!("读取启动偏好失败：{error}"))?;
    let prefs = serde_json::from_str::<LaunchPreferences>(&contents)
        .map_err(|error| format!("解析启动偏好失败：{error}"))?;
    sanitize_launch_preferences(prefs)
}

pub(crate) fn save_launch_preferences_to_path(
    path: &Path,
    prefs: &LaunchPreferences,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("创建配置目录失败：{error}"))?;
    }
    let contents = serde_json::to_string_pretty(prefs)
        .map_err(|error| format!("序列化启动偏好失败：{error}"))?;
    std::fs::write(path, contents).map_err(|error| format!("写入启动偏好失败：{error}"))
}

pub(crate) fn sanitize_launch_preferences(
    mut prefs: LaunchPreferences,
) -> Result<LaunchPreferences, String> {
    prefs.app_path = prefs.app_path.trim().to_string();
    migrate_old_default_ports(&mut prefs);
    validate_port("调试端口", prefs.debug_port)?;
    validate_port("后端端口", prefs.helper_port)?;
    if prefs.debug_port == prefs.helper_port {
        return Err("调试端口和后端端口不能相同。".to_string());
    }
    if !prefs.app_path.is_empty() {
        let path = PathBuf::from(&prefs.app_path);
        if !path.exists() {
            return Err("Codex 应用路径不存在。".to_string());
        }
    }
    Ok(prefs)
}

fn migrate_old_default_ports(prefs: &mut LaunchPreferences) {
    const OLD_DEBUG_PORT: u16 = 9333;
    const OLD_HELPER_PORT: u16 = 57321;
    if prefs.debug_port == OLD_DEBUG_PORT && prefs.helper_port == OLD_HELPER_PORT {
        prefs.debug_port = codex_pilot_core::ports::DEFAULT_DEBUG_PORT;
        prefs.helper_port = codex_pilot_core::ports::DEFAULT_HELPER_PORT;
    }
}

fn validate_port(label: &str, port: u16) -> Result<(), String> {
    if port == 0 {
        Err(format!("{label}不能为 0。"))
    } else {
        Ok(())
    }
}

pub(crate) fn launch_options_from_preferences(
    prefs: &LaunchPreferences,
) -> codex_pilot_core::launcher::LaunchOptions {
    codex_pilot_core::launcher::LaunchOptions {
        app_dir: if prefs.app_path.is_empty() {
            None
        } else {
            Some(PathBuf::from(&prefs.app_path))
        },
        debug_port: prefs.debug_port,
        helper_port: prefs.helper_port,
    }
}

pub(crate) fn build_codex_command_preview(app_dir: &Path, debug_port: u16) -> Vec<String> {
    if app_dir.extension().and_then(|value| value.to_str()) == Some("app") {
        codex_pilot_core::launcher::build_macos_open_command(app_dir, debug_port)
    } else {
        codex_pilot_core::launcher::build_codex_command(app_dir, debug_port)
    }
}

pub(crate) fn append_tokio_launcher_args(
    command: &mut tokio::process::Command,
    prefs: &LaunchPreferences,
) {
    if !prefs.app_path.is_empty() {
        command.arg("--app-path").arg(&prefs.app_path);
    }
    command
        .arg("--debug-port")
        .arg(prefs.debug_port.to_string())
        .arg("--helper-port")
        .arg(prefs.helper_port.to_string());
}
