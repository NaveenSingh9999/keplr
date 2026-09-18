use keplr_sync::Cas;

#[test]
fn cas_put_get_roundtrip() {
    let dir = std::env::temp_dir().join("keplr-cas-test");
    let _ = std::fs::remove_dir_all(&dir);
    let cas = Cas::new(dir);
    let hash = cas.put(b"hello cas").unwrap();
    assert!(cas.exists(&hash));
    assert_eq!(cas.get(&hash).unwrap(), b"hello cas");
}
