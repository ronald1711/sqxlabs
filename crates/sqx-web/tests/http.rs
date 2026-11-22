use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

fn request(addr: &str, method: &str, path: &str, body: &str) -> String {
    let mut s = TcpStream::connect(addr).expect("connect");
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).expect("write");
    let mut out = String::new();
    s.read_to_string(&mut out).expect("read");
    out
}

#[test]
fn http_endpoints_respond() {
    let addr = "127.0.0.1:18099";
    let store = sqx_data::DataStore::new();
    std::thread::spawn(move || {
        let _ = sqx_web::serve(addr, store);
    });
    std::thread::sleep(Duration::from_millis(400));

    let r = request(addr, "GET", "/api/datasources", "");
    assert!(r.contains("200 OK"), "datasources status: {r}");
    assert!(r.contains("symbols"), "datasources body: {r}");

    let r = request(addr, "GET", "/api/databank", "");
    assert!(r.contains("200 OK") && r.contains("rows"));

    let r = request(addr, "GET", "/api/profiles", "");
    assert!(r.contains("200 OK") && r.contains("brokers"));

    let r = request(addr, "GET", "/api/indicators", "");
    assert!(r.contains("200 OK") && r.contains("RSI"));
}
