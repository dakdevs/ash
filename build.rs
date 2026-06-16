use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    println!("cargo:rerun-if-changed=agent-bridges/anthropic/bridge.ts");
    println!("cargo:rerun-if-changed=agent-bridges/anthropic/cli-compiled.template.ts");
    println!("cargo:rerun-if-changed=package.json");
    println!("cargo:rerun-if-changed=agent-bridges/anthropic/package.json");
    println!("cargo:rerun-if-changed=bun.lock");
    println!("cargo:rerun-if-env-changed=ASH_ANTHROPIC_BRIDGE");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("out dir"));
    let bridge_binary = out_dir.join(executable_name("ash-anthropic-agent"));

    let target = target().unwrap_or_else(|message| panic!("{message}"));
    let package = target.claude_agent_sdk_package;
    let package_dir = find_node_package(&manifest_dir, package).unwrap_or_else(|| {
        panic!("missing {package}; run `bun install --frozen-lockfile` before `cargo build`")
    });
    let sdk_package = "@anthropic-ai/claude-agent-sdk";
    let sdk_package_dir = find_node_package(&manifest_dir, sdk_package).unwrap_or_else(|| {
        panic!("missing {sdk_package}; run `bun install --frozen-lockfile` before `cargo build`")
    });

    let template_path = manifest_dir.join("agent-bridges/anthropic/cli-compiled.template.ts");
    let bridge_path = manifest_dir.join("agent-bridges/anthropic/bridge.ts");
    let template = fs::read_to_string(&template_path).expect("read Anthropic bridge template");
    let entrypoint = template
        .replace(
            "__ASH_CLAUDE_AGENT_SDK_BINARY__",
            &absolute_import_path(&package_dir.join(target.binary_name)),
        )
        .replace(
            "__ASH_CLAUDE_AGENT_SDK_EXTRACT__",
            &absolute_import_path(&sdk_package_dir.join("extractFromBunfs.js")),
        )
        .replace("__ASH_BRIDGE_MODULE__", &absolute_import_path(&bridge_path));
    let entrypoint_path = out_dir.join("anthropic-bridge-entry.ts");
    fs::write(&entrypoint_path, entrypoint).expect("write generated Anthropic bridge entrypoint");

    let bun = find_bun().unwrap_or_else(|| {
        panic!("Bun is required to build the embedded Anthropic bridge; install Bun and run `bun install --frozen-lockfile`")
    });
    let output = Command::new(&bun)
        .arg("build")
        .arg(&entrypoint_path)
        .arg("--compile")
        .arg(format!("--target={}", target.bun_platform))
        .arg("--outfile")
        .arg(&bridge_binary)
        .current_dir(&manifest_dir)
        .output()
        .expect("spawn bun build for Anthropic bridge");

    assert!(
        output.status.success(),
        "failed to build embedded Anthropic bridge with Bun\nstdout:\n{}\nstderr:\n{}\nRun `bun install --frozen-lockfile` and retry.",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    println!(
        "cargo:rustc-env=ASH_EMBEDDED_ANTHROPIC_BRIDGE={}",
        bridge_binary.display()
    );
}

struct Target {
    bun_platform: &'static str,
    claude_agent_sdk_package: &'static str,
    binary_name: &'static str,
}

fn target() -> Result<Target, String> {
    let os = env::var("CARGO_CFG_TARGET_OS").expect("target os");
    let arch = env::var("CARGO_CFG_TARGET_ARCH").expect("target arch");
    let env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    match (os.as_str(), arch.as_str(), env.as_str()) {
        ("macos", "aarch64", _) => Ok(Target {
            bun_platform: "bun-darwin-arm64",
            claude_agent_sdk_package: "@anthropic-ai/claude-agent-sdk-darwin-arm64",
            binary_name: "claude",
        }),
        ("macos", "x86_64", _) => Ok(Target {
            bun_platform: "bun-darwin-x64",
            claude_agent_sdk_package: "@anthropic-ai/claude-agent-sdk-darwin-x64",
            binary_name: "claude",
        }),
        ("linux", "x86_64", "musl") => Ok(Target {
            bun_platform: "bun-linux-x64-musl",
            claude_agent_sdk_package: "@anthropic-ai/claude-agent-sdk-linux-x64-musl",
            binary_name: "claude",
        }),
        ("linux", "x86_64", _) => Ok(Target {
            bun_platform: "bun-linux-x64",
            claude_agent_sdk_package: "@anthropic-ai/claude-agent-sdk-linux-x64",
            binary_name: "claude",
        }),
        ("linux", "aarch64", "musl") => Ok(Target {
            bun_platform: "bun-linux-arm64-musl",
            claude_agent_sdk_package: "@anthropic-ai/claude-agent-sdk-linux-arm64-musl",
            binary_name: "claude",
        }),
        ("linux", "aarch64", _) => Ok(Target {
            bun_platform: "bun-linux-arm64",
            claude_agent_sdk_package: "@anthropic-ai/claude-agent-sdk-linux-arm64",
            binary_name: "claude",
        }),
        ("windows", "x86_64", _) => Ok(Target {
            bun_platform: "bun-windows-x64",
            claude_agent_sdk_package: "@anthropic-ai/claude-agent-sdk-win32-x64",
            binary_name: "claude.exe",
        }),
        ("windows", "aarch64", _) => Ok(Target {
            bun_platform: "bun-windows-arm64",
            claude_agent_sdk_package: "@anthropic-ai/claude-agent-sdk-win32-arm64",
            binary_name: "claude.exe",
        }),
        _ => Err(format!(
            "unsupported Anthropic bridge target {os}/{arch}/{env}; install a matching @anthropic-ai/claude-agent-sdk platform package and add it to build.rs"
        )),
    }
}

fn find_bun() -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|dir| dir.join(executable_name("bun")))
        .find(|candidate| candidate.is_file())
}

fn find_node_package(manifest_dir: &Path, package: &str) -> Option<PathBuf> {
    [
        manifest_dir.join("node_modules").join(package),
        manifest_dir
            .join("node_modules/.bun/node_modules")
            .join(package),
        manifest_dir
            .join("agent-bridges/anthropic/node_modules")
            .join(package),
    ]
    .into_iter()
    .find(|candidate| candidate.exists())
}

fn executable_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

fn absolute_import_path(path: &Path) -> String {
    let mut value = path
        .canonicalize()
        .expect("canonical bridge module path")
        .to_string_lossy()
        .replace('\\', "/");
    value = value.replace('\\', "\\\\").replace('"', "\\\"");
    value
}
