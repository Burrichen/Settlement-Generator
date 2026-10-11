use std::process::Command;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_settlement-generator-cli"))
}

#[test]
fn version_flag_prints_core_version() {
    let out = cli()
        .arg("--version")
        .output()
        .expect("run settlement-generator-cli");
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(
        stdout.trim(),
        format!("settlement-generator-cli {}", settlement_core::VERSION)
    );
}

#[test]
fn help_flag_succeeds() {
    let out = cli()
        .arg("--help")
        .output()
        .expect("run settlement-generator-cli");
    assert!(out.status.success());
}
