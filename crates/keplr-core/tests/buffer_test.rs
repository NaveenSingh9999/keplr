use keplr_core::buffer::Buffer;
use std::fs;

#[test]
fn buffer_load_line_save_roundtrip() {
    let path = std::env::temp_dir().join("keplr-buffer-test.txt");
    fs::write(&path, "one\ntwo\nthree\n").unwrap();
    let buf = Buffer::load(path.clone()).unwrap();
    assert_eq!(buf.len_lines(), 3);
    assert_eq!(buf.line(2).unwrap(), "two");
    fs::write(&path, "changed\n").unwrap();
    let buf2 = Buffer::load(path.clone()).unwrap();
    buf2.save().unwrap();
    assert!(path.exists());
}
