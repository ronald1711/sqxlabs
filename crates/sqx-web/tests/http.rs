use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

fn request(addr: &str, method: &str, path: &str, body: &str) -> String {
    let mut s = TcpStream::connect(addr).expect("connect");
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
        let _ = sqx_web::serve(addr, store, None);
    });
    let deadline=std::time::Instant::now()+Duration::from_secs(10);
    loop {
        if TcpStream::connect(addr).is_ok(){break;}
        assert!(std::time::Instant::now()<deadline,"server did not become ready");
        std::thread::sleep(Duration::from_millis(25));
    }

    let r = request(addr, "GET", "/api/datasources", "");
    assert!(r.contains("200 OK"), "datasources status: {r}");
    assert!(r.contains("symbols"), "datasources body: {r}");

    let r = request(addr, "GET", "/api/databank", "");
    assert!(r.contains("200 OK") && r.contains("rows"));

    let r = request(addr, "GET", "/api/profiles", "");
    assert!(r.contains("200 OK") && r.contains("brokers"));

    let r = request(addr, "GET", "/api/indicators", "");
    assert!(r.contains("200 OK") && r.contains("RSI"));
    let mut stream=TcpStream::connect(addr).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    write!(stream,"POST /api/research/replay HTTP/1.1\r\nHost: {addr}\r\nOrigin: http://{addr}\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{{}}").unwrap();
    let mut response=String::new();stream.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200"),"same-origin JSON should reach the application: {response}");
    assert!(response.contains("missing run_id"));

    for (headers,status) in [
        (format!("Host: attacker.example:{port}\r\nContent-Length: 0",port=18099),"403"),
        (format!("Host: {addr}\r\nOrigin: https://attacker.example\r\nContent-Length: 0"),"403"),
        (format!("Host: {addr}\r\nSec-Fetch-Site: cross-site\r\nContent-Length: 0"),"403"),
        (format!("Host: {addr}\r\nContent-Length: 0\r\nContent-Length: 1"),"400"),
        (format!("Host: {addr}\r\nContent-Type: text/plain\r\nContent-Length: 0"),"415"),
        (format!("Host: {addr}\r\nContent-Type: application/json\r\nContent-Length: 104857601"),"413"),
    ] {
        let mut stream=TcpStream::connect(addr).unwrap();stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        write!(stream,"POST /api/research/replay HTTP/1.1\r\n{headers}\r\n\r\n").unwrap();
        let mut response=String::new();stream.read_to_string(&mut response).unwrap();
        assert!(response.starts_with(&format!("HTTP/1.1 {status}")),"{response}");
    }

}

#[test]
fn remote_binding_requires_an_authentication_design(){
    let result=sqx_web::serve("0.0.0.0:0",sqx_data::DataStore::new(),None);
    assert_eq!(result.unwrap_err().kind(),std::io::ErrorKind::PermissionDenied);
}
