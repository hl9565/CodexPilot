#![cfg_attr(windows, windows_subsystem = "windows")]

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let options = codex_pilot_core::launcher::parse_launch_options(std::env::args().skip(1));
    codex_pilot_core::launcher::launch_and_inject(options).await
}
