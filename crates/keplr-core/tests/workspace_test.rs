use keplr_core::Workspace;
use std::path::PathBuf;

#[test]
fn workspace_cas_dir_is_under_root() {
    let ws = Workspace::new(PathBuf::from("/tmp/keplr-ws"));
    assert_eq!(ws.cas_dir(), PathBuf::from("/tmp/keplr-ws/.keplr/cas"));
}
