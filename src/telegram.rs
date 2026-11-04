use std::fmt;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use native_tls::{TlsConnector, TlsStream};
use serde::Deserialize;

const API_HOST: &str = "api.telegram.org";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
// Longer than the getUpdates long-poll wait, so a quiet poll isn't mistaken for a dead connection.
const SOCKET_TIMEOUT: Duration = Duration::from_secs(70);

pub trait TelegramApi {
    /// Sends one Bot API request with a JSON body and returns Telegram's raw JSON reply.
    fn post(&mut self, method: &str, body: &str) -> Result<String, ApiError>;
}

#[derive(Debug)]
pub enum ApiError {
    /// The connection failed or the reply was unreadable; trying again may help.
    Network(String),
    /// Telegram answered but refused the request.
    Rejected(String),
}

impl ApiError {
    pub fn is_network(&self) -> bool {
        matches!(self, Self::Network(_))
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Network(problem) | Self::Rejected(problem) => formatter.write_str(problem),
        }
    }
}

#[derive(Deserialize)]
struct Reply<T> {
    ok: bool,
    result: Option<T>,
    description: Option<String>,
}

pub fn parse_reply<'a, T: Deserialize<'a>>(method: &str, body: &'a str) -> Result<T, ApiError> {
    let reply: Reply<T> = serde_json::from_str(body)
        .map_err(|error| ApiError::Network(format!("{method}: unreadable reply ({error})")))?;
    if !reply.ok {
        let description = reply.description.unwrap_or_default();
        return Err(ApiError::Rejected(format!(
            "{method} failed: {description}"
        )));
    }
    reply
        .result
        .ok_or_else(|| ApiError::Network(format!("{method}: reply has no result")))
}

type Connection = BufReader<TlsStream<TcpStream>>;

/// One kept-alive HTTPS connection to the Bot API, reopened after any failure.
pub struct TelegramClient {
    token: String,
    tls: TlsConnector,
    connection: Option<Connection>,
}

impl TelegramClient {
    pub fn new(token: String) -> Result<Self, ApiError> {
        let tls = TlsConnector::new().map_err(|error| ApiError::Network(error.to_string()))?;
        Ok(Self {
            token,
            tls,
            connection: None,
        })
    }

    fn connection(&mut self) -> io::Result<&mut Connection> {
        if self.connection.is_none() {
            self.connection = Some(open_connection(&self.tls)?);
        }
        Ok(self
            .connection
            .as_mut()
            .expect("the connection was just opened"))
    }

    fn send(&mut self, method: &str, body: &str) -> io::Result<String> {
        let request = format!(
            "POST /bot{}/{method} HTTP/1.1\r\nHost: {API_HOST}\r\n\
             Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            self.token,
            body.len()
        );
        let connection = self.connection()?;
        connection.get_mut().write_all(request.as_bytes())?;
        connection.get_mut().flush()?;
        let head = read_response_head(connection)?;
        let reply = read_response_body(connection, &head)?;
        if head.closes_connection() {
            self.connection = None;
        }
        String::from_utf8(reply)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "reply isn't UTF-8"))
    }
}

impl TelegramApi for TelegramClient {
    fn post(&mut self, method: &str, body: &str) -> Result<String, ApiError> {
        self.send(method, body).map_err(|error| {
            self.connection = None; // The next request opens a fresh connection.
            ApiError::Network(format!("{method}: {error}"))
        })
    }
}

fn open_connection(tls: &TlsConnector) -> io::Result<Connection> {
    let tcp = connect_preferring_ipv4()?;
    tcp.set_read_timeout(Some(SOCKET_TIMEOUT))?;
    tcp.set_write_timeout(Some(SOCKET_TIMEOUT))?;
    let stream = tls
        .connect(API_HOST, tcp)
        .map_err(|error| io::Error::other(error.to_string()))?;
    Ok(BufReader::new(stream))
}

fn connect_preferring_ipv4() -> io::Result<TcpStream> {
    // IPv4 first: a flaky home IPv6 route otherwise stalls each reconnect for ~20 s.
    let mut addresses: Vec<SocketAddr> = (API_HOST, 443).to_socket_addrs()?.collect();
    addresses.sort_by_key(SocketAddr::is_ipv6);
    let mut last_error =
        io::Error::new(io::ErrorKind::NotFound, format!("can't resolve {API_HOST}"));
    for address in addresses {
        match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
            Ok(stream) => return Ok(stream),
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

#[derive(Default)]
struct ResponseHead {
    content_length: Option<usize>,
    chunked: bool,
    close: bool,
}

impl ResponseHead {
    fn closes_connection(&self) -> bool {
        self.close || (self.content_length.is_none() && !self.chunked)
    }
}

fn read_response_head(reader: &mut impl BufRead) -> io::Result<ResponseHead> {
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Err(io::Error::new(
            io::ErrorKind::ConnectionAborted,
            "Telegram closed the connection",
        ));
    }
    let mut head = ResponseHead::default();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
            return Ok(head);
        }
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "content-length" => head.content_length = value.parse().ok(),
            "transfer-encoding" => head.chunked = value.eq_ignore_ascii_case("chunked"),
            "connection" => head.close = value.eq_ignore_ascii_case("close"),
            _ => {}
        }
    }
}

fn read_response_body(reader: &mut impl BufRead, head: &ResponseHead) -> io::Result<Vec<u8>> {
    if head.chunked {
        return read_chunked_body(reader);
    }
    let mut body = Vec::new();
    match head.content_length {
        Some(length) => {
            body.resize(length, 0);
            reader.read_exact(&mut body)?;
        }
        None => {
            reader.read_to_end(&mut body)?;
        }
    }
    Ok(body)
}

fn read_chunked_body(reader: &mut impl BufRead) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    let mut line = String::new();
    loop {
        line.clear();
        reader.read_line(&mut line)?;
        let size_text = line.split(';').next().unwrap_or_default().trim();
        let size = usize::from_str_radix(size_text, 16)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "bad chunk size"))?;
        if size == 0 {
            break;
        }
        let start = body.len();
        body.resize(start + size, 0);
        reader.read_exact(&mut body[start..])?;
        line.clear();
        reader.read_line(&mut line)?;
    }
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
            return Ok(body);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn read(reply: &str) -> io::Result<Vec<u8>> {
        let mut reader = Cursor::new(reply.as_bytes().to_vec());
        let head = read_response_head(&mut reader)?;
        read_response_body(&mut reader, &head)
    }

    #[test]
    fn reads_a_content_length_reply() {
        let reply =
            "HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: keep-alive\r\n\r\n{\"ok\":true}";
        assert_eq!(read(reply).unwrap(), b"{\"ok\":true}");
    }

    #[test]
    fn reads_a_chunked_reply() {
        let reply = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\n{\"ok\r\n7;x=y\r\n\":true}\r\n0\r\n\r\n";
        assert_eq!(read(reply).unwrap(), b"{\"ok\":true}");
    }

    #[test]
    fn a_reply_cut_short_is_an_error() {
        assert!(read("HTTP/1.1 200 OK\r\nContent-Length: 50\r\n\r\n{\"ok\":tr").is_err());
    }

    #[test]
    fn a_closed_connection_is_an_error() {
        assert!(read("").is_err());
    }

    #[test]
    fn telegram_refusals_and_garbage_are_told_apart() {
        let refused = parse_reply::<bool>("getMe", r#"{"ok":false,"description":"Unauthorized"}"#);
        assert!(
            matches!(refused, Err(ApiError::Rejected(problem)) if problem == "getMe failed: Unauthorized")
        );
        assert!(
            parse_reply::<bool>("getMe", "<html>502</html>")
                .unwrap_err()
                .is_network()
        );
        assert!(parse_reply::<bool>("getMe", r#"{"ok":true,"result":true}"#).unwrap());
    }
}
