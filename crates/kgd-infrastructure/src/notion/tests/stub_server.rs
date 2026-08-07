//! Notion API の代わりに応答を返すスタブ HTTP サーバー。
//!
//! アダプタと再試行の組み合わせを、モックではなく実際の HTTP のやり取りで確認するために使う。

use std::sync::{Arc, Mutex};

use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::{TcpListener, TcpStream},
};

/// 用意した応答を順に返すスタブサーバー。
pub(super) struct StubServer {
    /// このサーバーを指すベース URL
    base_url: String,
    /// 受け取ったリクエストの記録 ("METHOD PATH" の形式)
    requests: Arc<Mutex<Vec<String>>>,
}

impl StubServer {
    /// 応答を順に返すスタブサーバーを起動する。
    ///
    /// 応答を使い切ったあとのリクエストには 500 を返す。
    pub(super) async fn start(responses: Vec<StubResponse>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("failed to bind stub server");
        let base_url = format!(
            "http://{}",
            listener.local_addr().expect("failed to read address")
        );

        let requests = Arc::new(Mutex::new(Vec::new()));
        let remaining = Arc::new(Mutex::new(responses.into_iter().collect::<Vec<_>>()));

        let accepted_requests = requests.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let requests = accepted_requests.clone();
                let remaining = remaining.clone();
                tokio::spawn(async move { serve(stream, requests, remaining).await });
            }
        });

        Self { base_url, requests }
    }

    /// このサーバーを指すベース URL を返す。
    pub(super) fn base_url(&self) -> &str {
        &self.base_url
    }

    /// 受け取ったリクエストを "METHOD PATH" の形式で返す。
    pub(super) fn requests(&self) -> Vec<String> {
        self.requests.lock().expect("lock poisoned").clone()
    }
}

/// スタブサーバーが返す応答。
pub(super) struct StubResponse {
    /// ステータスコード
    status: u16,
    /// レスポンスボディ (JSON)
    body: String,
}

impl StubResponse {
    /// 200 で JSON を返す応答を作る。
    pub(super) fn ok(body: &str) -> Self {
        Self {
            status: 200,
            body: body.to_string(),
        }
    }

    /// 指定したステータスで返す応答を作る。
    pub(super) fn status(status: u16, body: &str) -> Self {
        Self {
            status,
            body: body.to_string(),
        }
    }
}

/// 1 本の接続を処理する。keep-alive で複数のリクエストが来るため繰り返し読む。
async fn serve(
    mut stream: TcpStream,
    requests: Arc<Mutex<Vec<String>>>,
    remaining: Arc<Mutex<Vec<StubResponse>>>,
) {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];

    loop {
        // ヘッダの終端まで読む
        let head_end = loop {
            if let Some(index) = find_header_end(&buffer) {
                break index;
            }
            match stream.read(&mut chunk).await {
                Ok(0) | Err(_) => return,
                Ok(size) => buffer.extend_from_slice(&chunk[..size]),
            }
        };

        let head = String::from_utf8_lossy(&buffer[..head_end]).to_string();
        let request_line = head.lines().next().unwrap_or_default();
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or_default().to_string();
        let path = parts.next().unwrap_or_default().to_string();

        // ボディを読み切らないと次のリクエストの解釈がずれる
        let body_len = content_length(&head);
        let mut consumed = head_end + 4;
        while buffer.len() < consumed + body_len {
            match stream.read(&mut chunk).await {
                Ok(0) | Err(_) => return,
                Ok(size) => buffer.extend_from_slice(&chunk[..size]),
            }
        }
        consumed += body_len;
        buffer.drain(..consumed);

        requests
            .lock()
            .expect("lock poisoned")
            .push(format!("{} {}", method, path));

        let response = {
            let mut remaining = remaining.lock().expect("lock poisoned");
            if remaining.is_empty() {
                StubResponse::status(500, "{\"message\":\"no stub response left\"}")
            } else {
                remaining.remove(0)
            }
        };

        let raw = format!(
            "HTTP/1.1 {} {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
            response.status,
            reason_phrase(response.status),
            response.body.len(),
            response.body
        );
        if stream.write_all(raw.as_bytes()).await.is_err() {
            return;
        }
    }
}

/// ヘッダの終端 (空行) の位置を返す。
fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

/// ヘッダから Content-Length を読む。
fn content_length(head: &str) -> usize {
    head.lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.eq_ignore_ascii_case("content-length") {
                value.trim().parse().ok()
            } else {
                None
            }
        })
        .unwrap_or(0)
}

/// ステータスコードに対応する簡易的な理由句を返す。
fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        503 => "Service Unavailable",
        _ => "Error",
    }
}
