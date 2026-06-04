use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

#[test]
fn config_set_then_show() {
    let d = TempDir::new().unwrap();
    let cfg = d.path().join("config.toml");

    Command::cargo_bin("wxupd").unwrap()
        .env("WXUPD_CONFIG", &cfg)
        .args(["config", "set", "scheme.variant=flypy"])
        .assert().success().stdout(contains("set scheme.variant = flypy"));

    Command::cargo_bin("wxupd").unwrap()
        .env("WXUPD_CONFIG", &cfg)
        .args(["config", "show"])
        .assert().success().stdout(contains("variant = \"flypy\""));
}
