#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn server_command_uses_same_configuration_and_disables_startup_migrations() {
    assert_eq!(Command::parse([]).unwrap(), Command { deployed: false });
    assert_eq!(
        Command::parse(["server".into()]).unwrap(),
        Command { deployed: true }
    );
    for arguments in [
        vec!["unknown"],
        vec!["server", "vps"],
        vec!["server", "--runtime", "vps"],
        vec!["server", "--runtime", "vercel"],
        vec!["server", "extra"],
    ] {
        assert!(Command::parse(arguments.into_iter().map(str::to_owned)).is_err());
    }
}
