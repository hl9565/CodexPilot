pub fn injection_script(helper_port: u16) -> String {
    let script = include_str!("../../../assets/inject/renderer-inject.js");
    script
        .replace("__CODEX_PILOT_HELPER_PORT__", &helper_port.to_string())
        .replace("__CODEX_PILOT_VERSION__", crate::version::VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injection_script_exposes_plugin_patch_status_bridge() {
        let script = injection_script(57321);

        assert!(script.contains("/provider/plugin-patch-status"));
        assert!(script.contains("pluginPatchStatus()"));
        assert!(script.contains("plugin_patch_status"));
    }

    #[test]
    fn injection_script_unlocks_plugin_entry_and_install_buttons_for_api_mode() {
        let script = injection_script(57321);

        assert!(script.contains("pluginEntryButton()"));
        assert!(script.contains("spoofChatGPTAuthMethod"));
        assert!(script.contains("button[aria-disabled=\"true\"]"));
        assert!(script.contains("codex-pilot-force-install-unlocked"));
        assert!(script.contains("codexPilotPluginEnabled"));
        assert!(script.contains("codexPilotForceInstallUnlocked"));
    }
}
