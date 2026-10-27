use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    if env::var_os("CARGO_CFG_WINDOWS").is_none() {
        return;
    }

    println!("cargo:rerun-if-changed=assets/controller-control-icon.ico");
    println!("cargo:rerun-if-changed=assets/controller-control-icon.rc");

    let rc = find_resource_compiler();
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set"));
    let resource = out.join("controller-control-icon.res");
    let status = Command::new(&rc)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .arg("/nologo")
        .arg(format!("/fo{}", resource.display()))
        .arg("assets/controller-control-icon.rc")
        .status()
        .unwrap_or_else(|error| {
            panic!(
                "Could not run Windows resource compiler {}: {error}",
                rc.display()
            )
        });
    assert!(status.success(), "Windows resource compilation failed");
    println!("cargo:rustc-link-arg={}", resource.display());
}

fn find_resource_compiler() -> PathBuf {
    if let Some(path) = env::var_os("WindowsSdkVerBinPath") {
        let candidate = PathBuf::from(path).join("x64/rc.exe");
        if candidate.is_file() {
            return candidate;
        }
    }

    if let Some(sdk_root) = env::var_os("WindowsSdkDir") {
        let sdk_root = PathBuf::from(sdk_root).join("bin");
        if let Some(candidate) = sdk_root
            .join("x64/rc.exe")
            .is_file()
            .then(|| sdk_root.join("x64/rc.exe"))
        {
            return candidate;
        }
        if let Ok(versions) = std::fs::read_dir(&sdk_root) {
            let mut versions: Vec<_> = versions.flatten().map(|entry| entry.path()).collect();
            versions.sort();
            for version in versions.into_iter().rev() {
                let candidate = version.join("x64/rc.exe");
                if candidate.is_file() {
                    return candidate;
                }
            }
        }
    }

    if let Some(program_files) = env::var_os("ProgramFiles(x86)") {
        let sdk_root = PathBuf::from(program_files).join("Windows Kits/10/bin");
        if let Ok(versions) = std::fs::read_dir(&sdk_root) {
            let mut versions: Vec<_> = versions.flatten().map(|entry| entry.path()).collect();
            versions.sort();
            for version in versions.into_iter().rev() {
                let candidate = version.join("x64/rc.exe");
                if candidate.is_file() {
                    return candidate;
                }
            }
        }
    }

    panic!("rc.exe was not found. Install the Windows SDK to embed the application icon.");
}
