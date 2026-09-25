use std::net::TcpListener;

#[tokio::test]
async fn health_and_search_respond() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let root = std::env::temp_dir().join("keplr-serve-test");
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
    let module = reqwest::get(format!("http://127.0.0.1:{port}/ui-layout.js"))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(module.contains("export function currentLayout"));
    let font = reqwest::get(format!(
        "http://127.0.0.1:{port}/fonts/JetBrainsMono-Regular.ttf"
    ))
    .await
    .unwrap();
    assert_eq!(font.status(), 200);
    assert!(font.content_length().unwrap_or(0) > 1000);
}

#[test]
fn ui_document_closes_style_before_body() {
    let html = include_str!("../src/ui.html");
    let style_end = html.find("</style>").expect("UI style must be closed");
    let body_start = html.find("<body>").expect("UI body must be present");
    assert!(style_end < body_start);
}
