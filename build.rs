fn main() {
    // .version is written by CI on push to main (see .github/workflows/version.yml)
    // Fall back to git for local dev, then "unknown" if neither is available
    let hash = std::fs::read_to_string(".version")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            std::process::Command::new("git")
                .args(["rev-parse", "--short", "HEAD"])
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=GIT_HASH={}", hash);
    println!("cargo:rerun-if-changed=.version");
    println!("cargo:rerun-if-changed=.git/HEAD");
}
