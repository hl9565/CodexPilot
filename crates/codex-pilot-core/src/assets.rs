pub fn injection_script(helper_port: u16) -> String {
    let script = include_str!("../../../assets/inject/renderer-inject.js");
    script
        .replace("__CODEX_PILOT_HELPER_PORT__", &helper_port.to_string())
        .replace("__CODEX_PILOT_VERSION__", crate::version::VERSION)
}
