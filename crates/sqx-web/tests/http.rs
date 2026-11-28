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
        let _ = sqx_web::serve(addr, store, None, None);
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
    let result=sqx_web::serve("0.0.0.0:0",sqx_data::DataStore::new(),None,None);
    assert_eq!(result.unwrap_err().kind(),std::io::ErrorKind::PermissionDenied);
}

fn raw_request(addr:&str, host:&str, extra_headers:&str) -> String {
    let mut s = TcpStream::connect(addr).expect("connect");
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let req = format!("GET /api/datasources HTTP/1.1\r\nHost: {host}\r\n{extra_headers}Connection: close\r\n\r\n");
    s.write_all(req.as_bytes()).expect("write");
    let mut out = String::new();
    s.read_to_string(&mut out).expect("read");
    out
}

#[test]
fn external_host_accepts_exact_host_only(){
    let addr = "127.0.0.1:18101";
    let store = sqx_data::DataStore::new();
    std::thread::spawn(move || {
        let _ = sqx_web::serve(addr, store, None, Some("dashboard.tailnet.ts.net"));
    });
    let deadline=std::time::Instant::now()+Duration::from_secs(10);
    loop {
        if TcpStream::connect(addr).is_ok(){break;}
        assert!(std::time::Instant::now()<deadline,"server did not become ready");
        std::thread::sleep(Duration::from_millis(25));
    }
    // The configured host is accepted exactly, bare and with the local port.
    assert!(raw_request(addr,"dashboard.tailnet.ts.net:18101","").contains("200 OK"));
    assert!(raw_request(addr,"dashboard.tailnet.ts.net","").contains("200 OK"));
    // Any other host — including lookalikes — is rejected.
    assert!(raw_request(addr,"attacker.example:18101","").starts_with("HTTP/1.1 403"));
    assert!(raw_request(addr,"dashboard.tailnet.ts.net.evil.example:18101","").starts_with("HTTP/1.1 403"));
    // A foreign Origin is rejected even when the Host is the configured host.
    let mut s=TcpStream::connect(addr).unwrap();s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    write!(s,"GET /api/datasources HTTP/1.1\r\nHost: dashboard.tailnet.ts.net\r\nOrigin: https://attacker.example\r\nConnection: close\r\n\r\n").unwrap();
    let mut response=String::new();s.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 403"),"{response}");
}

#[test]
fn external_host_must_be_a_bare_hostname(){
    let result=sqx_web::serve("127.0.0.1:0",sqx_data::DataStore::new(),None,Some("https://dashboard.tailnet.ts.net"));
    assert_eq!(result.unwrap_err().kind(),std::io::ErrorKind::InvalidInput);
}
