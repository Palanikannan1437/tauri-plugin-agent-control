use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{
    command,
    plugin::{Builder, TauriPlugin},
    Manager, Runtime, State,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{oneshot, Mutex};

// ── State ────────────────────────────────────────────────────────────

struct AgentControlState {
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<String>>>>,
}

static RECORDING_STATE: std::sync::Mutex<Option<(u32, String)>> = std::sync::Mutex::new(None);

const MAX_REQUEST_SIZE: usize = 10_485_760;

// ── Respond command (called from JS shim) ────────────────────────────

#[command]
async fn agent_control_respond(
    state: State<'_, AgentControlState>,
    request_id: String,
    data: String,
) -> Result<(), String> {
    let mut map = state.pending.lock().await;
    if let Some(tx) = map.remove(&request_id) {
        let _ = tx.send(data);
    }
    Ok(())
}

// ── HTTP helpers ─────────────────────────────────────────────────────

fn http_response(status: u16, status_text: &str, body: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status,
        status_text,
        body.len(),
        body
    )
    .into_bytes()
}

fn ok_json(body: &str) -> Vec<u8> {
    http_response(200, "OK", body)
}

fn err_json(status: u16, msg: &str) -> Vec<u8> {
    let body = format!("{{\"error\":{}}}", serde_json::to_string(msg).unwrap_or_else(|_| format!("\"{}\"", msg)));
    http_response(status, "Error", &body)
}

// ── Parse HTTP request ───────────────────────────────────────────────

struct HttpRequest {
    method: String,
    path: String,
    query: HashMap<String, String>,
    body: String,
}

async fn parse_request(stream: &mut tokio::net::TcpStream) -> Option<HttpRequest> {
    let mut buf = vec![0u8; 8192];
    let mut total = 0;

    // Read headers
    loop {
        let n = stream.read(&mut buf[total..]).await.ok()?;
        if n == 0 {
            return None;
        }
        total += n;
        if total >= 4 {
            let s = std::str::from_utf8(&buf[..total]).ok()?;
            if s.contains("\r\n\r\n") {
                break;
            }
        }
        if total >= buf.len() {
            buf.resize(buf.len() * 2, 0);
            if buf.len() > MAX_REQUEST_SIZE {
                return None;
            }
        }
    }

    let raw = std::str::from_utf8(&buf[..total]).ok()?;
    let header_end = raw.find("\r\n\r\n")?;
    let header_section = &raw[..header_end];
    let body_start_offset = header_end + 4;

    let first_line = header_section.lines().next()?;
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }

    let method = parts[0].to_uppercase();
    let full_path = parts[1];

    let (path, query) = if let Some(idx) = full_path.find('?') {
        let p = &full_path[..idx];
        let q_str = &full_path[idx + 1..];
        let mut q = HashMap::new();
        for pair in q_str.split('&') {
            if let Some(eq) = pair.find('=') {
                let k = urlencoding::decode(&pair[..eq]).unwrap_or_default().to_string();
                let v = urlencoding::decode(&pair[eq + 1..]).unwrap_or_default().to_string();
                q.insert(k, v);
            }
        }
        (p.to_string(), q)
    } else {
        (full_path.to_string(), HashMap::new())
    };

    // Read body based on Content-Length
    let content_length: usize = header_section
        .lines()
        .find_map(|line| {
            let lower = line.to_lowercase();
            if lower.starts_with("content-length:") {
                lower.split(':').nth(1)?.trim().parse().ok()
            } else {
                None
            }
        })
        .unwrap_or(0);

    if content_length > MAX_REQUEST_SIZE {
        return None;
    }

    let already_read = total - body_start_offset;
    let mut body_bytes = buf[body_start_offset..total].to_vec();

    if content_length > already_read {
        let remaining = content_length - already_read;
        let mut extra = vec![0u8; remaining];
        stream.read_exact(&mut extra).await.ok()?;
        body_bytes.extend(extra);
    }

    let body = String::from_utf8(body_bytes).unwrap_or_default();

    Some(HttpRequest {
        method,
        path,
        query,
        body,
    })
}

// ── Eval bridge ──────────────────────────────────────────────────────

async fn eval_in_webview<R: Runtime>(
    app: &tauri::AppHandle<R>,
    pending: &Arc<Mutex<HashMap<String, oneshot::Sender<String>>>>,
    expr: &str,
    timeout_ms: u64,
) -> Result<String, String> {
    let id = format!("{:016x}", rand::random::<u64>());
    let (tx, rx) = oneshot::channel::<String>();

    {
        let mut map = pending.lock().await;
        map.insert(id.clone(), tx);
    }

    let escaped = expr.replace('\\', "\\\\").replace('`', "\\`");
    let js = [
        "(async()=>{try{let r=await (eval(`",
        &escaped,
        "`));window.__TAURI_INTERNALS__.invoke('plugin:agent-control|agent_control_respond',{requestId:'",
        &id,
        "',data:JSON.stringify(r??null)})}catch(e){window.__TAURI_INTERNALS__.invoke('plugin:agent-control|agent_control_respond',{requestId:'",
        &id,
        "',data:JSON.stringify({__error:e.message??String(e)})})}})()",
    ]
    .join("");

    let webview = app
        .get_webview_window("main")
        .ok_or_else(|| "No main webview found".to_string())?;
    webview
        .eval(&js)
        .map_err(|e| format!("eval error: {e}"))?;

    match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), rx).await {
        Ok(Ok(data)) => {
            // Check for error
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&data) {
                if let Some(err) = val.get("__error") {
                    return Err(format!("JS error: {}", err));
                }
            }
            Ok(data)
        }
        Ok(Err(_)) => Err("Channel closed".to_string()),
        Err(_) => {
            // Clean up
            let mut map = pending.lock().await;
            map.remove(&id);
            Err(format!("Timeout after {}ms", timeout_ms))
        }
    }
}

// ── Route handler ────────────────────────────────────────────────────

#[derive(Deserialize)]
struct RefBody {
    #[serde(rename = "ref")]
    ref_id: Option<String>,
}

#[derive(Deserialize)]
struct FillBody {
    #[serde(rename = "ref")]
    ref_id: String,
    text: String,
}

#[derive(Deserialize)]
struct PressBody {
    key: String,
}

#[derive(Deserialize)]
struct SelectBody {
    #[serde(rename = "ref")]
    ref_id: String,
    values: Vec<String>,
}

#[derive(Deserialize)]
struct ScrollBody {
    direction: String,
    amount: f64,
    selector: Option<String>,
}

#[derive(Deserialize)]
struct DragBody {
    from: String,
    to: String,
}

#[derive(Deserialize)]
struct UploadBody {
    #[serde(rename = "ref")]
    ref_id: String,
    #[serde(rename = "dataUrl")]
    data_url: String,
}

#[derive(Deserialize)]
struct WaitBody {
    ms: Option<u64>,
    selector: Option<String>,
    text: Option<String>,
    url: Option<String>,
    #[serde(rename = "fn")]
    func: Option<String>,
    timeout: Option<u64>,
}

#[derive(Deserialize)]
struct EvalBody {
    code: String,
}

#[derive(Deserialize)]
struct CookieSetBody {
    name: String,
    value: String,
}

#[derive(Deserialize)]
struct StorageSetBody {
    #[serde(rename = "type")]
    storage_type: String,
    key: String,
    value: String,
}

#[derive(Deserialize)]
struct StorageClearBody {
    #[serde(rename = "type")]
    storage_type: String,
}

#[derive(Deserialize, Serialize)]
struct FindBody {
    by: String,
    value: Option<String>,
    action: Option<String>,
    text: Option<String>,
    name: Option<String>,
    exact: Option<bool>,
    n: Option<u32>,
    index: Option<u32>,
}

#[derive(Deserialize, Serialize)]
struct MouseBody {
    action: String,
    x: Option<f64>,
    y: Option<f64>,
    button: Option<String>,
    #[serde(rename = "deltaY")]
    delta_y: Option<f64>,
}

#[derive(Deserialize)]
struct KeyBody {
    key: String,
}

#[derive(Deserialize)]
struct HighlightBody {
    #[serde(rename = "ref")]
    ref_id: String,
}

#[derive(Deserialize)]
struct DialogBody {
    action: String,
    text: Option<String>,
}

#[derive(Deserialize)]
struct ViewportBody {
    width: u32,
    height: u32,
}

#[derive(Deserialize, Serialize)]
struct StateLoadBody {
    #[serde(rename = "localStorage")]
    local_storage: Option<HashMap<String, String>>,
    #[serde(rename = "sessionStorage")]
    session_storage: Option<HashMap<String, String>>,
    cookies: Option<HashMap<String, String>>,
}

#[derive(Deserialize)]
struct DownloadBody {
    url: String,
    path: String,
}

#[derive(Deserialize)]
struct GeoBody {
    lat: f64,
    lng: f64,
}

#[derive(Deserialize)]
struct OfflineBody {
    enabled: bool,
}

#[derive(Deserialize)]
struct HeadersBody {
    headers: HashMap<String, String>,
}

#[derive(Deserialize, Serialize)]
struct NetworkRouteBody {
    url: String,
    abort: Option<bool>,
    body: Option<String>,
}

async fn handle_request<R: Runtime>(
    req: HttpRequest,
    app: &tauri::AppHandle<R>,
    pending: &Arc<Mutex<HashMap<String, oneshot::Sender<String>>>>,
) -> Vec<u8> {
    let path = req.path.as_str();
    let method = req.method.as_str();

    // OPTIONS (CORS preflight)
    if method == "OPTIONS" {
        return ok_json("{}");
    }

    // Health
    if path == "/health" && method == "GET" {
        return ok_json("{\"ok\":true}");
    }

    // Screenshot
    if path == "/screenshot" && method == "GET" {
        let custom_path = req.query.get("path");
        let is_full = req.query.get("full").map(|v| v == "true").unwrap_or(false);
        let out_path = match custom_path {
            Some(p) => p.clone(),
            None => {
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis();
                format!("/tmp/agent-control-screenshot-{ts}.png")
            }
        };

        if is_full {
            // Get full page dimensions
            let dims = eval_in_webview(app, pending, "JSON.stringify({w:document.documentElement.scrollWidth,h:document.documentElement.scrollHeight})", 5000).await;
            let webview = match app.get_webview_window("main") {
                Some(w) => w,
                None => return err_json(500, "No main webview found"),
            };
            let original_size = webview.inner_size().ok();

            if let Ok(dims_str) = dims {
                if let Ok(d) = serde_json::from_str::<serde_json::Value>(&dims_str) {
                    let w = d["w"].as_f64().unwrap_or(1280.0) as u32;
                    let h = d["h"].as_f64().unwrap_or(800.0) as u32;
                    let _ = webview.set_size(tauri::Size::Logical(tauri::LogicalSize::new(w as f64, h as f64)));
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            }

            let result = std::process::Command::new("screencapture")
                .args(["-x", &out_path])
                .output();

            // Restore original size
            if let Some(size) = original_size {
                let _ = webview.set_size(tauri::Size::Physical(size));
            }

            return match result {
                Ok(o) if o.status.success() => {
                    ok_json(&format!("{{\"path\":{}}}", serde_json::to_string(&out_path).unwrap()))
                }
                Ok(o) => err_json(500, &format!("screencapture failed: {}", String::from_utf8_lossy(&o.stderr))),
                Err(e) => err_json(500, &format!("screencapture error: {e}")),
            };
        }

        let result = std::process::Command::new("screencapture")
            .args(["-x", &out_path])
            .output();
        return match result {
            Ok(o) if o.status.success() => {
                ok_json(&format!("{{\"path\":{}}}", serde_json::to_string(&out_path).unwrap()))
            }
            Ok(o) => err_json(500, &format!("screencapture failed: {}", String::from_utf8_lossy(&o.stderr))),
            Err(e) => err_json(500, &format!("screencapture error: {e}")),
        };
    }

    // Snapshot
    if path == "/snapshot" && method == "GET" {
        let format = req.query.get("format").map(|s| s.as_str()).unwrap_or("json");
        let scope = req.query.get("scope");
        let depth = req.query.get("depth").and_then(|d| d.parse::<u32>().ok());
        let interactive = req.query.get("interactive").map(|v| v != "false");

        let mut opts = Vec::new();
        opts.push(format!("format:'{format}'"));
        if let Some(s) = scope {
            opts.push(format!("scope:{}", serde_json::to_string(s).unwrap()));
        }
        if let Some(d) = depth {
            opts.push(format!("depth:{d}"));
        }
        if let Some(i) = interactive {
            opts.push(format!("interactive:{i}"));
        }

        let expr = format!("window.__DEVTOOLS__.snapshot({{{}}})", opts.join(","));
        match eval_in_webview(app, pending, &expr, 5000).await {
            Ok(data) => ok_json(&data),
            Err(e) => err_json(500, &e),
        }
    }
    // POST interactions with ref
    else if method == "POST" && matches!(path, "/click" | "/dblclick" | "/hover" | "/focus" | "/check" | "/uncheck" | "/scrollintoview") {
        let action = &path[1..]; // strip leading /
        let body: RefBody = match serde_json::from_str(&req.body) {
            Ok(b) => b,
            Err(e) => return err_json(400, &format!("bad json: {e}")),
        };
        let ref_id = match &body.ref_id {
            Some(r) => r,
            None => return err_json(400, "missing 'ref' field"),
        };
        let method_name = match action {
            "scrollintoview" => "scrollIntoView",
            other => other,
        };
        let expr = format!("window.__DEVTOOLS__.{method_name}({})", serde_json::to_string(ref_id).unwrap());
        match eval_in_webview(app, pending, &expr, 5000).await {
            Ok(_) => ok_json("{\"ok\":true}"),
            Err(e) => err_json(500, &e),
        }
    }
    // Fill
    else if method == "POST" && path == "/fill" {
        let body: FillBody = match serde_json::from_str(&req.body) {
            Ok(b) => b,
            Err(e) => return err_json(400, &format!("bad json: {e}")),
        };
        let expr = format!(
            "window.__DEVTOOLS__.fill({},{})",
            serde_json::to_string(&body.ref_id).unwrap(),
            serde_json::to_string(&body.text).unwrap()
        );
        match eval_in_webview(app, pending, &expr, 5000).await {
            Ok(_) => ok_json("{\"ok\":true}"),
            Err(e) => err_json(500, &e),
        }
    }
    // Type
    else if method == "POST" && path == "/type" {
        let body: FillBody = match serde_json::from_str(&req.body) {
            Ok(b) => b,
            Err(e) => return err_json(400, &format!("bad json: {e}")),
        };
        let expr = format!(
            "window.__DEVTOOLS__.type({},{})",
            serde_json::to_string(&body.ref_id).unwrap(),
            serde_json::to_string(&body.text).unwrap()
        );
        match eval_in_webview(app, pending, &expr, 5000).await {
            Ok(_) => ok_json("{\"ok\":true}"),
            Err(e) => err_json(500, &e),
        }
    }
    // Press
    else if method == "POST" && path == "/press" {
        let body: PressBody = match serde_json::from_str(&req.body) {
            Ok(b) => b,
            Err(e) => return err_json(400, &format!("bad json: {e}")),
        };
        let expr = format!("window.__DEVTOOLS__.press({})", serde_json::to_string(&body.key).unwrap());
        match eval_in_webview(app, pending, &expr, 5000).await {
            Ok(_) => ok_json("{\"ok\":true}"),
            Err(e) => err_json(500, &e),
        }
    }
    // Select
    else if method == "POST" && path == "/select" {
        let body: SelectBody = match serde_json::from_str(&req.body) {
            Ok(b) => b,
            Err(e) => return err_json(400, &format!("bad json: {e}")),
        };
        let expr = format!(
            "window.__DEVTOOLS__.select({},{})",
            serde_json::to_string(&body.ref_id).unwrap(),
            serde_json::to_string(&body.values).unwrap()
        );
        match eval_in_webview(app, pending, &expr, 5000).await {
            Ok(_) => ok_json("{\"ok\":true}"),
            Err(e) => err_json(500, &e),
        }
    }
    // Scroll
    else if method == "POST" && path == "/scroll" {
        let body: ScrollBody = match serde_json::from_str(&req.body) {
            Ok(b) => b,
            Err(e) => return err_json(400, &format!("bad json: {e}")),
        };
        let selector_arg = match &body.selector {
            Some(s) => serde_json::to_string(s).unwrap(),
            None => "undefined".to_string(),
        };
        let expr = format!(
            "window.__DEVTOOLS__.scroll({{direction:{},amount:{},selector:{}}})",
            serde_json::to_string(&body.direction).unwrap(),
            body.amount,
            selector_arg
        );
        match eval_in_webview(app, pending, &expr, 5000).await {
            Ok(_) => ok_json("{\"ok\":true}"),
            Err(e) => err_json(500, &e),
        }
    }
    // Drag
    else if method == "POST" && path == "/drag" {
        let body: DragBody = match serde_json::from_str(&req.body) {
            Ok(b) => b,
            Err(e) => return err_json(400, &format!("bad json: {e}")),
        };
        let expr = format!(
            "window.__DEVTOOLS__.drag({},{})",
            serde_json::to_string(&body.from).unwrap(),
            serde_json::to_string(&body.to).unwrap()
        );
        match eval_in_webview(app, pending, &expr, 5000).await {
            Ok(_) => ok_json("{\"ok\":true}"),
            Err(e) => err_json(500, &e),
        }
    }
    // Upload
    else if method == "POST" && path == "/upload" {
        let body: UploadBody = match serde_json::from_str(&req.body) {
            Ok(b) => b,
            Err(e) => return err_json(400, &format!("bad json: {e}")),
        };
        let expr = format!(
            "window.__DEVTOOLS__.upload({},{})",
            serde_json::to_string(&body.ref_id).unwrap(),
            serde_json::to_string(&body.data_url).unwrap()
        );
        match eval_in_webview(app, pending, &expr, 5000).await {
            Ok(_) => ok_json("{\"ok\":true}"),
            Err(e) => err_json(500, &e),
        }
    }
    // Wait
    else if method == "POST" && path == "/wait" {
        let body: WaitBody = match serde_json::from_str(&req.body) {
            Ok(b) => b,
            Err(e) => return err_json(400, &format!("bad json: {e}")),
        };
        // Pure delay (no JS needed)
        if let Some(ms) = body.ms {
            if body.selector.is_none() && body.text.is_none() && body.url.is_none() && body.func.is_none() {
                tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
                return ok_json("{\"ok\":true}");
            }
        }
        let timeout = body.timeout.unwrap_or(5000);
        let mut opts = Vec::new();
        if let Some(s) = &body.selector {
            opts.push(format!("selector:{}", serde_json::to_string(s).unwrap()));
        }
        if let Some(t) = &body.text {
            opts.push(format!("text:{}", serde_json::to_string(t).unwrap()));
        }
        if let Some(u) = &body.url {
            opts.push(format!("url:{}", serde_json::to_string(u).unwrap()));
        }
        if let Some(f) = &body.func {
            opts.push(format!("fn:{}", serde_json::to_string(f).unwrap()));
        }
        opts.push(format!("timeout:{timeout}"));

        let expr = format!("window.__DEVTOOLS__.wait({{{}}})", opts.join(","));
        match eval_in_webview(app, pending, &expr, timeout + 1000).await {
            Ok(data) => ok_json(&format!("{{\"ok\":{data}}}")),
            Err(e) => err_json(500, &e),
        }
    }
    // Eval
    else if method == "POST" && path == "/eval" {
        let body: EvalBody = match serde_json::from_str(&req.body) {
            Ok(b) => b,
            Err(e) => return err_json(400, &format!("bad json: {e}")),
        };
        match eval_in_webview(app, pending, &body.code, 5000).await {
            Ok(data) => ok_json(&data),
            Err(e) => err_json(500, &e),
        }
    }
    // Console
    else if method == "GET" && path == "/console" {
        let expr = "window.__DEVTOOLS__.getConsole()";
        match eval_in_webview(app, pending, expr, 5000).await {
            Ok(data) => ok_json(&data),
            Err(e) => err_json(500, &e),
        }
    }
    else if method == "GET" && path == "/errors" {
        let expr = "window.__DEVTOOLS__.getErrors()";
        match eval_in_webview(app, pending, expr, 5000).await {
            Ok(data) => ok_json(&data),
            Err(e) => err_json(500, &e),
        }
    }
    // Cookies
    else if method == "GET" && path == "/cookies" {
        let expr = "window.__DEVTOOLS__.getCookies()";
        match eval_in_webview(app, pending, expr, 5000).await {
            Ok(data) => ok_json(&data),
            Err(e) => err_json(500, &e),
        }
    }
    else if method == "POST" && path == "/cookies/set" {
        let body: CookieSetBody = match serde_json::from_str(&req.body) {
            Ok(b) => b,
            Err(e) => return err_json(400, &format!("bad json: {e}")),
        };
        let expr = format!(
            "window.__DEVTOOLS__.setCookie({},{})",
            serde_json::to_string(&body.name).unwrap(),
            serde_json::to_string(&body.value).unwrap()
        );
        match eval_in_webview(app, pending, &expr, 5000).await {
            Ok(_) => ok_json("{\"ok\":true}"),
            Err(e) => err_json(500, &e),
        }
    }
    else if method == "POST" && path == "/cookies/clear" {
        let expr = "window.__DEVTOOLS__.clearCookies()";
        match eval_in_webview(app, pending, &expr, 5000).await {
            Ok(_) => ok_json("{\"ok\":true}"),
            Err(e) => err_json(500, &e),
        }
    }
    // Storage
    else if method == "POST" && path == "/storage/set" {
        let body: StorageSetBody = match serde_json::from_str(&req.body) {
            Ok(b) => b,
            Err(e) => return err_json(400, &format!("bad json: {e}")),
        };
        let expr = format!(
            "window.__DEVTOOLS__.setStorage({},{},{})",
            serde_json::to_string(&body.storage_type).unwrap(),
            serde_json::to_string(&body.key).unwrap(),
            serde_json::to_string(&body.value).unwrap()
        );
        match eval_in_webview(app, pending, &expr, 5000).await {
            Ok(_) => ok_json("{\"ok\":true}"),
            Err(e) => err_json(500, &e),
        }
    }
    else if method == "POST" && path == "/storage/clear" {
        let body: StorageClearBody = match serde_json::from_str(&req.body) {
            Ok(b) => b,
            Err(e) => return err_json(400, &format!("bad json: {e}")),
        };
        let expr = format!(
            "window.__DEVTOOLS__.clearStorage({})",
            serde_json::to_string(&body.storage_type).unwrap()
        );
        match eval_in_webview(app, pending, &expr, 5000).await {
            Ok(_) => ok_json("{\"ok\":true}"),
            Err(e) => err_json(500, &e),
        }
    }
    // Back
    else if method == "POST" && path == "/back" {
        match eval_in_webview(app, pending, "history.back()", 5000).await {
            Ok(_) => ok_json("{\"ok\":true}"),
            Err(e) => err_json(500, &e),
        }
    }
    // Forward
    else if method == "POST" && path == "/forward" {
        match eval_in_webview(app, pending, "history.forward()", 5000).await {
            Ok(_) => ok_json("{\"ok\":true}"),
            Err(e) => err_json(500, &e),
        }
    }
    // Reload
    else if method == "POST" && path == "/reload" {
        match eval_in_webview(app, pending, "location.reload()", 5000).await {
            Ok(_) => ok_json("{\"ok\":true}"),
            Err(e) => err_json(500, &e),
        }
    }
    // Find (semantic locators)
    else if method == "POST" && path == "/find" {
        let body: FindBody = match serde_json::from_str(&req.body) {
            Ok(b) => b,
            Err(e) => return err_json(400, &format!("bad json: {e}")),
        };
        let expr = format!(
            "window.__DEVTOOLS__.find({})",
            serde_json::to_string(&body).unwrap()
        );
        match eval_in_webview(app, pending, &expr, 5000).await {
            Ok(data) => ok_json(&data),
            Err(e) => err_json(500, &e),
        }
    }
    // Mouse
    else if method == "POST" && path == "/mouse" {
        let body: MouseBody = match serde_json::from_str(&req.body) {
            Ok(b) => b,
            Err(e) => return err_json(400, &format!("bad json: {e}")),
        };
        let expr = format!(
            "window.__DEVTOOLS__.mouse({})",
            serde_json::to_string(&body).unwrap()
        );
        match eval_in_webview(app, pending, &expr, 5000).await {
            Ok(_) => ok_json("{\"ok\":true}"),
            Err(e) => err_json(500, &e),
        }
    }
    // Keydown
    else if method == "POST" && path == "/keydown" {
        let body: KeyBody = match serde_json::from_str(&req.body) {
            Ok(b) => b,
            Err(e) => return err_json(400, &format!("bad json: {e}")),
        };
        let expr = format!(
            "window.__DEVTOOLS__.keydown({})",
            serde_json::to_string(&body.key).unwrap()
        );
        match eval_in_webview(app, pending, &expr, 5000).await {
            Ok(_) => ok_json("{\"ok\":true}"),
            Err(e) => err_json(500, &e),
        }
    }
    // Keyup
    else if method == "POST" && path == "/keyup" {
        let body: KeyBody = match serde_json::from_str(&req.body) {
            Ok(b) => b,
            Err(e) => return err_json(400, &format!("bad json: {e}")),
        };
        let expr = format!(
            "window.__DEVTOOLS__.keyup({})",
            serde_json::to_string(&body.key).unwrap()
        );
        match eval_in_webview(app, pending, &expr, 5000).await {
            Ok(_) => ok_json("{\"ok\":true}"),
            Err(e) => err_json(500, &e),
        }
    }
    // Highlight
    else if method == "POST" && path == "/highlight" {
        let body: HighlightBody = match serde_json::from_str(&req.body) {
            Ok(b) => b,
            Err(e) => return err_json(400, &format!("bad json: {e}")),
        };
        let expr = format!(
            "window.__DEVTOOLS__.highlight({})",
            serde_json::to_string(&body.ref_id).unwrap()
        );
        match eval_in_webview(app, pending, &expr, 5000).await {
            Ok(_) => ok_json("{\"ok\":true}"),
            Err(e) => err_json(500, &e),
        }
    }
    // Dialog
    else if method == "POST" && path == "/dialog" {
        let body: DialogBody = match serde_json::from_str(&req.body) {
            Ok(b) => b,
            Err(e) => return err_json(400, &format!("bad json: {e}")),
        };
        let expr = match body.action.as_str() {
            "accept" => {
                match &body.text {
                    Some(t) => format!("window.__DEVTOOLS__.dialogAccept({})", serde_json::to_string(t).unwrap()),
                    None => "window.__DEVTOOLS__.dialogAccept()".to_string(),
                }
            }
            "dismiss" => "window.__DEVTOOLS__.dialogDismiss()".to_string(),
            other => return err_json(400, &format!("unknown dialog action: {other}")),
        };
        match eval_in_webview(app, pending, &expr, 5000).await {
            Ok(_) => ok_json("{\"ok\":true}"),
            Err(e) => err_json(500, &e),
        }
    }
    // Get dialogs
    else if method == "GET" && path == "/dialogs" {
        let expr = "window.__DEVTOOLS__.getDialogs()";
        match eval_in_webview(app, pending, expr, 5000).await {
            Ok(data) => ok_json(&data),
            Err(e) => err_json(500, &e),
        }
    }
    // Viewport
    else if method == "POST" && path == "/viewport" {
        let body: ViewportBody = match serde_json::from_str(&req.body) {
            Ok(b) => b,
            Err(e) => return err_json(400, &format!("bad json: {e}")),
        };
        let webview = match app.get_webview_window("main") {
            Some(w) => w,
            None => return err_json(500, "No main webview found"),
        };
        match webview.set_size(tauri::Size::Logical(tauri::LogicalSize::new(body.width as f64, body.height as f64))) {
            Ok(_) => ok_json("{\"ok\":true}"),
            Err(e) => err_json(500, &format!("resize error: {e}")),
        }
    }
    // State save
    else if method == "POST" && path == "/state/save" {
        let expr = "window.__DEVTOOLS__.saveState()";
        match eval_in_webview(app, pending, expr, 5000).await {
            Ok(data) => ok_json(&data),
            Err(e) => err_json(500, &e),
        }
    }
    // State load
    else if method == "POST" && path == "/state/load" {
        let body: StateLoadBody = match serde_json::from_str(&req.body) {
            Ok(b) => b,
            Err(e) => return err_json(400, &format!("bad json: {e}")),
        };
        let expr = format!(
            "window.__DEVTOOLS__.loadState({})",
            serde_json::to_string(&body).unwrap()
        );
        match eval_in_webview(app, pending, &expr, 5000).await {
            Ok(_) => ok_json("{\"ok\":true}"),
            Err(e) => err_json(500, &e),
        }
    }
    // GET routes with path segments
    else if method == "GET" && path.starts_with("/get/") {
        handle_get_route(path, app, pending).await
    }
    else if method == "GET" && path.starts_with("/is/") {
        handle_is_route(path, app, pending).await
    }
    else if method == "GET" && path.starts_with("/storage/") {
        handle_storage_get(path, app, pending).await
    }
    // Record start
    else if method == "POST" && path == "/record/start" {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let out_path = format!("/tmp/agent-control-recording-{ts}.mov");
        let child = std::process::Command::new("screencapture")
            .args(["-v", &out_path])
            .spawn();
        match child {
            Ok(c) => {
                let pid = c.id();
                let mut state = RECORDING_STATE.lock().unwrap();
                *state = Some((pid, out_path.clone()));
                ok_json(&format!("{{\"ok\":true,\"pid\":{pid},\"path\":{}}}", serde_json::to_string(&out_path).unwrap()))
            }
            Err(e) => err_json(500, &format!("failed to start recording: {e}")),
        }
    }
    // Record stop
    else if method == "POST" && path == "/record/stop" {
        let state = {
            let mut s = RECORDING_STATE.lock().unwrap();
            s.take()
        };
        match state {
            Some((pid, path)) => {
                // screencapture -v is interactive — send SIGINT to stop it gracefully
                let _ = std::process::Command::new("kill")
                    .args(["-2", &pid.to_string()])
                    .output();
                // Wait a moment for file to finalize
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                ok_json(&format!("{{\"ok\":true,\"path\":{}}}", serde_json::to_string(&path).unwrap()))
            }
            None => err_json(400, "no recording in progress"),
        }
    }
    // Download
    else if method == "POST" && path == "/download" {
        let body: DownloadBody = match serde_json::from_str(&req.body) {
            Ok(b) => b,
            Err(e) => return err_json(400, &format!("bad json: {e}")),
        };
        let result = std::process::Command::new("curl")
            .args(["-fsSL", "-o", &body.path, &body.url])
            .output();
        match result {
            Ok(o) if o.status.success() => {
                ok_json(&format!("{{\"ok\":true,\"path\":{}}}", serde_json::to_string(&body.path).unwrap()))
            }
            Ok(o) => err_json(500, &format!("download failed: {}", String::from_utf8_lossy(&o.stderr))),
            Err(e) => err_json(500, &format!("download error: {e}")),
        }
    }
    // Network intercept
    else if method == "POST" && path == "/network/intercept" {
        let expr = "window.__DEVTOOLS__.networkIntercept()";
        match eval_in_webview(app, pending, expr, 5000).await {
            Ok(_) => ok_json("{\"ok\":true}"),
            Err(e) => err_json(500, &e),
        }
    }
    // Network reset
    else if method == "POST" && path == "/network/reset" {
        let expr = "window.__DEVTOOLS__.networkReset()";
        match eval_in_webview(app, pending, expr, 5000).await {
            Ok(_) => ok_json("{\"ok\":true}"),
            Err(e) => err_json(500, &e),
        }
    }
    // Network requests
    else if method == "GET" && path == "/network/requests" {
        let expr = "window.__DEVTOOLS__.networkRequests()";
        match eval_in_webview(app, pending, expr, 5000).await {
            Ok(data) => ok_json(&data),
            Err(e) => err_json(500, &e),
        }
    }
    // Network route
    else if method == "POST" && path == "/network/route" {
        let body: NetworkRouteBody = match serde_json::from_str(&req.body) {
            Ok(b) => b,
            Err(e) => return err_json(400, &format!("bad json: {e}")),
        };
        let expr = format!(
            "window.__DEVTOOLS__.networkRoute({})",
            serde_json::to_string(&body).unwrap()
        );
        match eval_in_webview(app, pending, &expr, 5000).await {
            Ok(_) => ok_json("{\"ok\":true}"),
            Err(e) => err_json(500, &e),
        }
    }
    // Geolocation
    else if method == "POST" && path == "/geo" {
        let body: GeoBody = match serde_json::from_str(&req.body) {
            Ok(b) => b,
            Err(e) => return err_json(400, &format!("bad json: {e}")),
        };
        let expr = format!("window.__DEVTOOLS__.setGeo({},{})", body.lat, body.lng);
        match eval_in_webview(app, pending, &expr, 5000).await {
            Ok(_) => ok_json("{\"ok\":true}"),
            Err(e) => err_json(500, &e),
        }
    }
    // Offline
    else if method == "POST" && path == "/offline" {
        let body: OfflineBody = match serde_json::from_str(&req.body) {
            Ok(b) => b,
            Err(e) => return err_json(400, &format!("bad json: {e}")),
        };
        let expr = format!("window.__DEVTOOLS__.setOffline({})", body.enabled);
        match eval_in_webview(app, pending, &expr, 5000).await {
            Ok(_) => ok_json("{\"ok\":true}"),
            Err(e) => err_json(500, &e),
        }
    }
    // Headers
    else if method == "POST" && path == "/headers" {
        let body: HeadersBody = match serde_json::from_str(&req.body) {
            Ok(b) => b,
            Err(e) => return err_json(400, &format!("bad json: {e}")),
        };
        let expr = format!(
            "window.__DEVTOOLS__.setHeaders({})",
            serde_json::to_string(&body.headers).unwrap()
        );
        match eval_in_webview(app, pending, &expr, 5000).await {
            Ok(_) => ok_json("{\"ok\":true}"),
            Err(e) => err_json(500, &e),
        }
    }
    // Trace start
    else if method == "POST" && path == "/trace/start" {
        let expr = "window.__DEVTOOLS__.traceStart()";
        match eval_in_webview(app, pending, expr, 5000).await {
            Ok(_) => ok_json("{\"ok\":true}"),
            Err(e) => err_json(500, &e),
        }
    }
    // Trace stop
    else if method == "POST" && path == "/trace/stop" {
        let expr = "window.__DEVTOOLS__.traceStop()";
        match eval_in_webview(app, pending, expr, 5000).await {
            Ok(data) => ok_json(&data),
            Err(e) => err_json(500, &e),
        }
    }
    // 404
    else {
        err_json(404, "not found")
    }
}

async fn handle_get_route<R: Runtime>(
    path: &str,
    app: &tauri::AppHandle<R>,
    pending: &Arc<Mutex<HashMap<String, oneshot::Sender<String>>>>,
) -> Vec<u8> {
    let segments: Vec<&str> = path.trim_start_matches("/get/").splitn(3, '/').collect();

    match segments.as_slice() {
        ["title"] => {
            match eval_in_webview(app, pending, "document.title", 5000).await {
                Ok(data) => ok_json(&data),
                Err(e) => err_json(500, &e),
            }
        }
        ["url"] => {
            match eval_in_webview(app, pending, "location.href", 5000).await {
                Ok(data) => ok_json(&data),
                Err(e) => err_json(500, &e),
            }
        }
        ["text", ref_id] => {
            let expr = format!("window.__DEVTOOLS__.getText({})", serde_json::to_string(ref_id).unwrap());
            match eval_in_webview(app, pending, &expr, 5000).await {
                Ok(data) => ok_json(&data),
                Err(e) => err_json(500, &e),
            }
        }
        ["html", ref_id] => {
            let expr = format!("window.__DEVTOOLS__.getHtml({})", serde_json::to_string(ref_id).unwrap());
            match eval_in_webview(app, pending, &expr, 5000).await {
                Ok(data) => ok_json(&data),
                Err(e) => err_json(500, &e),
            }
        }
        ["value", ref_id] => {
            let expr = format!("window.__DEVTOOLS__.getValue({})", serde_json::to_string(ref_id).unwrap());
            match eval_in_webview(app, pending, &expr, 5000).await {
                Ok(data) => ok_json(&data),
                Err(e) => err_json(500, &e),
            }
        }
        ["attr", ref_id, attr] => {
            let expr = format!(
                "window.__DEVTOOLS__.getAttr({},{})",
                serde_json::to_string(ref_id).unwrap(),
                serde_json::to_string(attr).unwrap()
            );
            match eval_in_webview(app, pending, &expr, 5000).await {
                Ok(data) => ok_json(&data),
                Err(e) => err_json(500, &e),
            }
        }
        ["styles", ref_id] => {
            let expr = format!("window.__DEVTOOLS__.getStyles({})", serde_json::to_string(ref_id).unwrap());
            match eval_in_webview(app, pending, &expr, 5000).await {
                Ok(data) => ok_json(&data),
                Err(e) => err_json(500, &e),
            }
        }
        ["box", ref_id] => {
            let expr = format!("window.__DEVTOOLS__.getBox({})", serde_json::to_string(ref_id).unwrap());
            match eval_in_webview(app, pending, &expr, 5000).await {
                Ok(data) => ok_json(&data),
                Err(e) => err_json(500, &e),
            }
        }
        ["count", rest] => {
            let selector = urlencoding::decode(rest).unwrap_or_default().to_string();
            let expr = format!("window.__DEVTOOLS__.getCount({})", serde_json::to_string(&selector).unwrap());
            match eval_in_webview(app, pending, &expr, 5000).await {
                Ok(data) => ok_json(&data),
                Err(e) => err_json(500, &e),
            }
        }
        _ => err_json(404, "unknown get route"),
    }
}

async fn handle_is_route<R: Runtime>(
    path: &str,
    app: &tauri::AppHandle<R>,
    pending: &Arc<Mutex<HashMap<String, oneshot::Sender<String>>>>,
) -> Vec<u8> {
    let segments: Vec<&str> = path.trim_start_matches("/is/").splitn(2, '/').collect();

    match segments.as_slice() {
        ["visible", ref_id] => {
            let expr = format!("window.__DEVTOOLS__.isVisible({})", serde_json::to_string(ref_id).unwrap());
            match eval_in_webview(app, pending, &expr, 5000).await {
                Ok(data) => ok_json(&data),
                Err(e) => err_json(500, &e),
            }
        }
        ["enabled", ref_id] => {
            let expr = format!("window.__DEVTOOLS__.isEnabled({})", serde_json::to_string(ref_id).unwrap());
            match eval_in_webview(app, pending, &expr, 5000).await {
                Ok(data) => ok_json(&data),
                Err(e) => err_json(500, &e),
            }
        }
        ["checked", ref_id] => {
            let expr = format!("window.__DEVTOOLS__.isChecked({})", serde_json::to_string(ref_id).unwrap());
            match eval_in_webview(app, pending, &expr, 5000).await {
                Ok(data) => ok_json(&data),
                Err(e) => err_json(500, &e),
            }
        }
        _ => err_json(404, "unknown is route"),
    }
}

async fn handle_storage_get<R: Runtime>(
    path: &str,
    app: &tauri::AppHandle<R>,
    pending: &Arc<Mutex<HashMap<String, oneshot::Sender<String>>>>,
) -> Vec<u8> {
    let segments: Vec<&str> = path.trim_start_matches("/storage/").splitn(2, '/').collect();

    match segments.as_slice() {
        ["local"] => {
            let expr = "window.__DEVTOOLS__.getStorage(\"local\")";
            match eval_in_webview(app, pending, expr, 5000).await {
                Ok(data) => ok_json(&data),
                Err(e) => err_json(500, &e),
            }
        }
        ["session"] => {
            let expr = "window.__DEVTOOLS__.getStorage(\"session\")";
            match eval_in_webview(app, pending, expr, 5000).await {
                Ok(data) => ok_json(&data),
                Err(e) => err_json(500, &e),
            }
        }
        ["local", key] => {
            let decoded = urlencoding::decode(key).unwrap_or_default().to_string();
            let expr = format!("window.__DEVTOOLS__.getStorageKey(\"local\",{})", serde_json::to_string(&decoded).unwrap());
            match eval_in_webview(app, pending, &expr, 5000).await {
                Ok(data) => ok_json(&data),
                Err(e) => err_json(500, &e),
            }
        }
        ["session", key] => {
            let decoded = urlencoding::decode(key).unwrap_or_default().to_string();
            let expr = format!("window.__DEVTOOLS__.getStorageKey(\"session\",{})", serde_json::to_string(&decoded).unwrap());
            match eval_in_webview(app, pending, &expr, 5000).await {
                Ok(data) => ok_json(&data),
                Err(e) => err_json(500, &e),
            }
        }
        _ => err_json(404, "unknown storage route"),
    }
}

// ── Plugin init ──────────────────────────────────────────────────────

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("agent-control")
        .invoke_handler(tauri::generate_handler![agent_control_respond])
        .on_navigation(|window, url| {
            #[cfg(debug_assertions)]
            {
                let shim = include_str!("../guest-js/index.js");
                let webview = window.clone();
                // Re-inject shim after a short delay to let the new page settle
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    let _ = webview.eval(shim);
                });
            }
            let _ = url;
            true
        })
        .setup(|app, _api| {
            #[cfg(debug_assertions)]
            {
                let state = AgentControlState {
                    pending: Arc::new(Mutex::new(HashMap::new())),
                };
                let pending = state.pending.clone();
                app.manage(state);

                let app_handle = app.clone();

                tauri::async_runtime::spawn(async move {
                    // Wait briefly for webview to initialize
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

                    // Start HTTP server
                    let listener = match TcpListener::bind("127.0.0.1:9876").await {
                        Ok(l) => l,
                        Err(e) => {
                            eprintln!("[agent-control] Failed to bind port 9876: {e}");
                            return;
                        }
                    };

                    eprintln!("[agent-control] HTTP bridge listening on http://localhost:9876");

                    loop {
                        let (mut stream, _) = match listener.accept().await {
                            Ok(s) => s,
                            Err(_) => continue,
                        };

                        let app = app_handle.clone();
                        let pending = pending.clone();

                        tauri::async_runtime::spawn(async move {
                            if let Some(req) = parse_request(&mut stream).await {
                                let response = handle_request(req, &app, &pending).await;
                                let _ = stream.write_all(&response).await;
                                let _ = stream.flush().await;
                            }
                        });
                    }
                });
            }

            #[cfg(not(debug_assertions))]
            {
                let state = AgentControlState {
                    pending: Arc::new(Mutex::new(HashMap::new())),
                };
                app.manage(state);
            }

            Ok(())
        })
        .build()
}
