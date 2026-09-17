use std::{net::TcpListener, path::PathBuf};

#[tokio::test]
async fn health_and_search_respond() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let root = PathBuf::from("/tmp/keplr-serve-test");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("a.txt"), "hello serve\n").unwrap();
    let server_root = root.clone();
    tokio::spawn(async move {
        keplr_serve::serve(server_root, port).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    let health = reqwest::get(format!("http://127.0.0.1:{port}/health"))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(health.contains("ok"));
    let search = reqwest::get(format!("http://127.0.0.1:{port}/search?needle=hello&limit=5"))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(search.contains("a.txt"));
}
