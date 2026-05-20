fn main() {
    prepare_dev_sidecar();
    tauri_build::build()
}

fn prepare_dev_sidecar() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let target = std::env::var("TARGET").unwrap_or_default();
    let profile = std::env::var("PROFILE").unwrap_or_default();
    if target.is_empty() {
        return;
    }

    let suffix = if target.contains("windows") {
        ".exe"
    } else {
        ""
    };
    let sidecar_dir = std::path::Path::new(&manifest_dir).join("binaries");
    let sidecar_path = sidecar_dir.join(format!("codex-pilot-launcher-{target}{suffix}"));
    println!("cargo:rerun-if-changed={}", sidecar_path.display());

    if sidecar_path.exists() {
        return;
    }

    if profile == "release" {
        panic!(
            "missing launcher sidecar: {}. Run scripts/package-macos.sh or prepare the platform sidecar before release bundling.",
            sidecar_path.display()
        );
    }

    std::fs::create_dir_all(&sidecar_dir).expect("failed to create sidecar directory");
    #[cfg(windows)]
    {
        std::fs::write(&sidecar_path, b"").expect("failed to create dev sidecar placeholder");
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(&sidecar_path, b"#!/bin/sh\nexit 1\n")
            .expect("failed to create dev sidecar placeholder");
        let mut permissions = std::fs::metadata(&sidecar_path)
            .expect("failed to stat dev sidecar placeholder")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&sidecar_path, permissions)
            .expect("failed to chmod dev sidecar placeholder");
    }
}
