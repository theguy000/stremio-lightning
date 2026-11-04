//! A local HTTP server for one test run: it serves the addresses a registry names from a table,
//! and answers every other address with a page that says it is missing.
//!
//! The core's mods module reads the registry through `reqwest`, so a test can only check install,
//! update and error paths against a server it controls. The server answers one request per
//! connection and closes it, which is all `reqwest` asks for, and it binds an ephemeral port on
//! the loopback address so two runs never want the same one.

#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// One address the server answers, as the bytes and the content type it answers with.
#[derive(Clone, Debug)]
pub struct Page {
    pub status: u16,
    pub content_type: &'static str,
    pub body: String,
}

impl Page {
    pub fn json(body: impl Into<String>) -> Self {
        Self {
            status: 200,
            content_type: "application/json",
            body: body.into(),
        }
    }

    pub fn javascript(body: impl Into<String>) -> Self {
        Self {
            status: 200,
            content_type: "application/javascript",
            body: body.into(),
        }
    }

    pub fn css(body: impl Into<String>) -> Self {
        Self {
            status: 200,
            content_type: "text/css",
            body: body.into(),
        }
    }

    pub fn html(body: impl Into<String>) -> Self {
        Self {
            status: 200,
            content_type: "text/html",
            body: body.into(),
        }
    }

    pub fn status(status: u16) -> Self {
        Self {
            status,
            content_type: "text/plain",
            body: "not found".to_string(),
        }
    }
}

/// A server of this run's own, answering from `pages` until it is dropped.
pub struct Server {
    address: SocketAddr,
    pages: Vec<(String, Page)>,
    asked: Arc<std::sync::Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    /// Starts the server on a free loopback port.
    ///
    /// `make` builds the table from the address the server took, which is how one page names
    /// another on the same server: a registry's downloads are addresses, and they are written
    /// before the registry is served.
    pub fn start(make: impl FnOnce(&str) -> Vec<(String, Page)>) -> Self {
        Self::bind("127.0.0.1:0", make)
    }

    fn bind(address: &str, make: impl FnOnce(&str) -> Vec<(String, Page)>) -> Self {
        let listener = TcpListener::bind(address)
            .unwrap_or_else(|error| panic!("a free loopback port at {address}: {error}"));
        let address = listener.local_addr().expect("the port that was taken");
        let pages: Vec<(String, Page)> = make(&format!("http://{address}"))
            .into_iter()
            .map(|(path, page)| (path.to_string(), page))
            .collect();
        listener
            .set_nonblocking(true)
            .expect("a listener that can be stopped");
        let stop = Arc::new(AtomicBool::new(false));
        let asked: Vec<String> = Vec::new();
        let asked = Arc::new(std::sync::Mutex::new(asked));
        let thread_pages = pages.clone();
        let thread_stop = stop.clone();
        let thread_asked = asked.clone();
        let thread = std::thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    // Each request is answered on a thread of its own, so two reads in flight
                    // at once are both answered: the update check asks for several files before
                    // it waits on any of them.
                    Ok((stream, _)) => {
                        let pages = thread_pages.clone();
                        let asked = thread_asked.clone();
                        std::thread::spawn(move || answer(stream, &pages, &asked));
                    }
                    Err(_) => std::thread::sleep(std::time::Duration::from_millis(10)),
                }
            }
        });
        Self {
            address,
            pages,
            asked,
            stop,
            thread: Some(thread),
        }
    }

    /// Where the server answers, without a trailing slash: "http://127.0.0.1:PORT".
    pub fn base(&self) -> String {
        format!("http://{}", self.address)
    }

    /// The address of one page on this server.
    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base())
    }

    /// The paths the server answers, for a failure that says what was asked for.
    pub fn paths(&self) -> Vec<String> {
        self.pages.iter().map(|(path, _)| path.clone()).collect()
    }

    /// Every request line target this server has answered, oldest first: what it was actually
    /// asked for, which a test compares with what it meant to ask for.
    pub fn asked(&self) -> Vec<String> {
        self.asked.lock().expect("the server's log").clone()
    }

    /// What the server answered for `path`, as status and body length.
    pub fn answers(&self, path: &str) -> Option<(u16, usize)> {
        self.pages
            .iter()
            .find(|(known, _)| known == path)
            .map(|(_, page)| (page.status, page.body.len()))
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Answers one request on `stream` and closes it.
fn answer(mut stream: TcpStream, pages: &[(String, Page)], asked: &std::sync::Mutex<Vec<String>>) {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
    let mut request = Vec::new();
    let mut buffer = [0u8; 1024];
    // The request line and its headers end at the first blank line; a registry read sends no body.
    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(read) => request.extend_from_slice(&buffer[..read]),
        }
    }
    let text = String::from_utf8_lossy(&request);
    let path = text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/")
        .to_string();
    asked.lock().expect("the server's log").push(path.clone());
    let page = pages
        .iter()
        .find(|(known, _)| *known == path)
        .map(|(_, page)| page.clone())
        .unwrap_or_else(|| Page::status(404));
    let reason = match page.status {
        200 => "OK",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Unknown",
    };
    let response = if page.status == 302 {
        // A redirect names the address to follow in full. A relative one makes the client resolve
        // it against the request's own address, which turns "/registry.json" into
        // "/downloads/registry.json": the test then reads the registry from the installed file.
        format!(
            "HTTP/1.1 302 Found\r\nLocation: {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            page.body
        )
    } else {
        format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            page.status,
            reason,
            page.content_type,
            page.body.len(),
            page.body
        )
    };
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}
