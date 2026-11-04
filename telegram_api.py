import json
import socket
import ssl

from config import BOT_TOKEN

API_HOST = "api.telegram.org"
CONNECT_TIMEOUT_SECONDS = 10
# Longer than the getUpdates long-poll wait, so a quiet poll isn't mistaken for a dead connection.
SOCKET_TIMEOUT_SECONDS = 70
# A dropped or garbled connection shows up as one of these; the caller may simply retry.
NETWORK_ERRORS = (OSError, ValueError)
TLS_CONTEXT = ssl.create_default_context()

bot_account: dict = {}


class TelegramError(Exception):
    pass


class Connection:
    """One kept-alive HTTPS connection to the Bot API, reopened after any failure."""

    def __init__(self) -> None:
        self.tls_socket = None
        self.reader = None

    def open(self) -> None:
        if self.tls_socket is not None:
            return
        raw_socket = open_socket()
        raw_socket.settimeout(SOCKET_TIMEOUT_SECONDS)
        self.tls_socket = TLS_CONTEXT.wrap_socket(raw_socket, server_hostname=API_HOST)
        self.reader = self.tls_socket.makefile("rb")

    def close(self) -> None:
        tls_socket, reader = self.tls_socket, self.reader
        self.tls_socket = self.reader = None
        for resource in (reader, tls_socket):
            try:
                if resource is not None:
                    resource.close()
            except OSError:
                pass


def open_socket() -> socket.socket:
    # IPv4 first: a flaky home IPv6 route otherwise stalls each reconnect for ~20 s.
    last_error = OSError(f"Can't resolve {API_HOST}")
    for family in (socket.AF_INET, socket.AF_INET6):
        try:
            address = socket.getaddrinfo(API_HOST, 443, family, socket.SOCK_STREAM)[0][4]
            return socket.create_connection(address[:2], timeout=CONNECT_TIMEOUT_SECONDS)
        except OSError as error:
            last_error = error
    raise last_error


connection = Connection()


def call(method: str, **params) -> object:
    try:
        reply = json.loads(send_request(method, json.dumps(params).encode()))
    except NETWORK_ERRORS:
        connection.close()
        raise
    if not reply["ok"]:
        raise TelegramError(f"{method} failed: {reply.get('description')}")
    return reply["result"]


def send_request(method: str, body: bytes) -> bytes:
    connection.open()
    head = (
        f"POST /bot{BOT_TOKEN}/{method} HTTP/1.1\r\nHost: {API_HOST}\r\n"
        f"Content-Type: application/json\r\nContent-Length: {len(body)}\r\n\r\n"
    )
    connection.tls_socket.sendall(head.encode() + body)
    headers = read_response_headers(connection.reader)
    reply_body = read_response_body(connection.reader, headers)
    if headers.get("connection", "").lower() == "close":
        connection.close()
    return reply_body


def read_response_headers(stream) -> dict:
    if not stream.readline():
        raise ConnectionError("Telegram closed the connection")
    headers = {}
    while (line := stream.readline()) not in (b"\r\n", b"\n", b""):
        name, _, value = line.decode("latin-1").partition(":")
        headers[name.strip().lower()] = value.strip()
    return headers


def read_response_body(stream, headers: dict) -> bytes:
    if headers.get("transfer-encoding", "").lower() == "chunked":
        return read_chunked_body(stream)
    if "content-length" not in headers:
        body = stream.read()
        connection.close()
        return body
    length = int(headers["content-length"])
    body = stream.read(length)
    if len(body) < length:
        raise ConnectionError("Telegram closed the connection mid-reply")
    return body


def read_chunked_body(stream) -> bytes:
    chunks = []
    while (size := int(stream.readline().split(b";")[0], 16)) > 0:
        chunks.append(stream.read(size))
        stream.readline()
    while stream.readline() not in (b"\r\n", b"\n", b""):
        pass
    return b"".join(chunks)


def identify_bot() -> None:
    bot_account.update(call("getMe"))
