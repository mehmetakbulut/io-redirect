use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;

#[test]
fn runs_selftest() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("examples/selftest")?;
    cmd.assert().success();
    Ok(())
}