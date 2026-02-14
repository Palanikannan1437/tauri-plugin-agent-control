# tauri-plugin-agent-control

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)
[![Tauri v2](https://img.shields.io/badge/Tauri-v2-24C8D8.svg)](https://v2.tauri.app)

**Dev-only HTTP bridge for Tauri webviews — like Chrome DevTools Protocol, but for Tauri.**

## What it does

`tauri-plugin-agent-control` exposes your Tauri webview over a local HTTP API at `http://localhost:9876`. You can snapshot the DOM, click buttons, fill inputs, evaluate JavaScript, intercept network requests, take screenshots, and more — all via simple `curl` commands or any HTTP client. The server only runs in debug builds, so it's completely stripped from production.

## Installation

### 1. Add the Rust dependency

**From crates.io:**

```toml
[dependencies]
tauri-plugin-agent-control = "0.1"
```

**Or from git:**

```toml
[dependencies]
tauri-plugin-agent-control = { git = "https://github.com/palanik1/tauri-plugin-agent-control" }
```

### 2. Register the plugin

In your `src-tauri/src/lib.rs` (where `tauri::Builder` is configured):

```rust
tauri::Builder::default()
    .plugin(tauri_plugin_agent_control::init())
    // ... your other plugins and handlers
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
```

### 3. Add capability permissions

In your `src-tauri/capabilities/default.json` (create it if it doesn't exist):

```json
{
  "identifier": "default",
  "windows": ["main"],
  "permissions": [
    "agent-control:default"
  ]
}
```

> `agent-control:default` grants `allow-agent-control-respond` — the only Tauri command the plugin registers. It allows the JS shim to send eval results back to the Rust HTTP server.

### 4. Run in dev mode

```bash
cargo tauri dev
```

The HTTP bridge starts automatically on `http://localhost:9876` in debug builds. Verify it's running:

```bash
curl -s http://localhost:9876/health
# {"ok":true}
```

> **Production builds:** The server is completely stripped — it's gated behind `#[cfg(debug_assertions)]`. No code, no port, no surface area.

## AI Agent Setup

### Install the skill (recommended — works with 30+ agents)

```bash
npx skills add Palanikannan1437/tauri-plugin-agent-control
```

This auto-detects your installed AI agents (Amp, Claude Code, Cursor, Copilot, Cline, etc.) and installs the skill to the right location.

### Or install manually

```bash
# For Amp / Gemini CLI / Codex
mkdir -p .agents/skills/tauri-agent-control
curl -o .agents/skills/tauri-agent-control/SKILL.md \
  https://raw.githubusercontent.com/Palanikannan1437/tauri-plugin-agent-control/main/SKILL.md

# For Claude Code
mkdir -p .claude/skills/tauri-agent-control
curl -o .claude/skills/tauri-agent-control/SKILL.md \
  https://raw.githubusercontent.com/Palanikannan1437/tauri-plugin-agent-control/main/SKILL.md

# For Cursor
mkdir -p .cursor/skills/tauri-agent-control
curl -o .cursor/skills/tauri-agent-control/SKILL.md \
  https://raw.githubusercontent.com/Palanikannan1437/tauri-plugin-agent-control/main/SKILL.md
```

### What the agent can do

Once installed, your AI agent can interact with your running Tauri app via natural language:

- *"Take a screenshot of the app"*
- *"Click the Submit button"*
- *"Fill in the email field with test@example.com"*
- *"Check if the sidebar is visible"*
- *"Intercept network requests to /api/users"*

The agent uses the skill to translate these into `curl` commands to the HTTP bridge.

## Quick Start

**1. Snapshot the page to see what's on screen:**

```bash
curl http://localhost:9876/snapshot?format=compact
```

```
[page] My App — http://localhost:1420/
  @e1 [button] "Submit" role="button"
  @e2 [input type="text"] placeholder="Enter name" name="username"
  @e3 [a] "Home" href="/"
```

**2. Interact with elements using their refs:**

```bash
# Fill an input
curl -X POST http://localhost:9876/fill \
  -d '{"ref":"@e2","text":"hello world"}'

# Click a button
curl -X POST http://localhost:9876/click \
  -d '{"ref":"@e1"}'
```

**3. Re-snapshot to see the updated state:**

```bash
curl http://localhost:9876/snapshot?format=compact
```

**4. Evaluate arbitrary JavaScript:**

```bash
curl -X POST http://localhost:9876/eval \
  -d '{"code":"document.title"}'
```

## API Reference

### Snapshot

| Method | Path | Query / Body | Description |
|--------|------|-------------|-------------|
| `GET` | `/snapshot` | `?format=compact\|json` `&scope=<selector>` `&depth=<n>` `&interactive=true\|false` | Snapshot visible elements. `compact` returns a text tree, `json` returns full element data. `interactive=false` includes all elements, not just interactive ones. |

### Interactions

| Method | Path | Body | Description |
|--------|------|------|-------------|
| `POST` | `/click` | `{"ref":"@e1"}` | Click an element |
| `POST` | `/dblclick` | `{"ref":"@e1"}` | Double-click an element |
| `POST` | `/hover` | `{"ref":"@e1"}` | Hover over an element (mouseenter + mouseover) |
| `POST` | `/focus` | `{"ref":"@e1"}` | Focus an element |
| `POST` | `/fill` | `{"ref":"@e1","text":"value"}` | Set input/textarea value directly (fires input + change) |
| `POST` | `/type` | `{"ref":"@e1","text":"value"}` | Type text character-by-character (fires keydown/keypress/input/keyup per char) |
| `POST` | `/press` | `{"key":"Enter"}` | Press a key. Supports modifiers: `"Control+a"`, `"Meta+Shift+z"` |
| `POST` | `/check` | `{"ref":"@e1"}` | Check a checkbox |
| `POST` | `/uncheck` | `{"ref":"@e1"}` | Uncheck a checkbox |
| `POST` | `/select` | `{"ref":"@e1","values":["opt1","opt2"]}` | Select options in a `<select>` element |
| `POST` | `/scroll` | `{"direction":"down","amount":500}` | Scroll the page. Optional `"selector"` to scroll a container. Directions: `up`, `down`, `left`, `right` |
| `POST` | `/scrollintoview` | `{"ref":"@e1"}` | Scroll an element into view (smooth, centered) |
| `POST` | `/drag` | `{"from":"@e1","to":"@e2"}` | Drag and drop between two elements |
| `POST` | `/upload` | `{"ref":"@e1","dataUrl":"data:..."}` | Upload a file to a file input via base64 data URL |

### Getters

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/get/title` | Get `document.title` |
| `GET` | `/get/url` | Get `location.href` |
| `GET` | `/get/text/{ref}` | Get `textContent` of an element |
| `GET` | `/get/html/{ref}` | Get `innerHTML` of an element |
| `GET` | `/get/value/{ref}` | Get `value` of an input/textarea |
| `GET` | `/get/attr/{ref}/{attr}` | Get an attribute value |
| `GET` | `/get/styles/{ref}` | Get computed styles (font, color, display, position, etc.) |
| `GET` | `/get/box/{ref}` | Get bounding box `{x, y, width, height}` |
| `GET` | `/get/count/{selector}` | Count elements matching a CSS selector (URL-encode the selector) |

### State Checks

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/is/visible/{ref}` | Check if element is visible |
| `GET` | `/is/enabled/{ref}` | Check if element is enabled (not disabled) |
| `GET` | `/is/checked/{ref}` | Check if checkbox/radio is checked |

### Wait

| Method | Path | Body | Description |
|--------|------|------|-------------|
| `POST` | `/wait` | `{"ms":1000}` | Wait for a fixed delay |
| `POST` | `/wait` | `{"selector":".loaded"}` | Wait for a CSS selector to appear |
| `POST` | `/wait` | `{"text":"Success"}` | Wait for text to appear on the page |
| `POST` | `/wait` | `{"url":"**/dashboard"}` | Wait for URL to match (glob patterns supported) |
| `POST` | `/wait` | `{"fn":"document.querySelector('.x')"}` | Wait for a JS expression to be truthy |
| `POST` | `/wait` | `{"selector":".x","timeout":10000}` | Custom timeout (default: 5000ms) |

### Eval

| Method | Path | Body | Description |
|--------|------|------|-------------|
| `POST` | `/eval` | `{"code":"document.title"}` | Evaluate arbitrary JavaScript and return the result |

### Console / Errors

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/console` | Get and drain captured console logs (log, warn, error, info, debug) |
| `GET` | `/errors` | Get and drain captured console errors only |

### Cookies

| Method | Path | Body | Description |
|--------|------|------|-------------|
| `GET` | `/cookies` | — | Get all cookies as `{name: value}` |
| `POST` | `/cookies/set` | `{"name":"x","value":"y"}` | Set a cookie |
| `POST` | `/cookies/clear` | — | Clear all cookies |

### Storage

| Method | Path | Body | Description |
|--------|------|------|-------------|
| `GET` | `/storage/local` | — | Get all localStorage entries |
| `GET` | `/storage/session` | — | Get all sessionStorage entries |
| `GET` | `/storage/local/{key}` | — | Get a single localStorage key |
| `GET` | `/storage/session/{key}` | — | Get a single sessionStorage key |
| `POST` | `/storage/set` | `{"type":"local","key":"k","value":"v"}` | Set a storage entry. `type` is `"local"` or `"session"` |
| `POST` | `/storage/clear` | `{"type":"local"}` | Clear all entries for a storage type |

### Navigation

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/back` | Navigate back (`history.back()`) |
| `POST` | `/forward` | Navigate forward (`history.forward()`) |
| `POST` | `/reload` | Reload the page (`location.reload()`) |

### Find (Semantic Locators)

| Method | Path | Body | Description |
|--------|------|------|-------------|
| `POST` | `/find` | `{"by":"role","value":"button","name":"Submit"}` | Find by ARIA role with optional accessible name |
| `POST` | `/find` | `{"by":"text","value":"Hello"}` | Find by visible text content |
| `POST` | `/find` | `{"by":"label","value":"Email"}` | Find by associated label text or `aria-label` |
| `POST` | `/find` | `{"by":"placeholder","value":"Search..."}` | Find by placeholder attribute |
| `POST` | `/find` | `{"by":"testid","value":"submit-btn"}` | Find by `data-testid` |
| `POST` | `/find` | `{"by":"alt","value":"Logo"}` | Find by `alt` attribute |
| `POST` | `/find` | `{"by":"title","value":"Close"}` | Find by `title` attribute |
| `POST` | `/find` | `{"by":"first","value":"button"}` | First element matching selector |
| `POST` | `/find` | `{"by":"last","value":".item"}` | Last element matching selector |
| `POST` | `/find` | `{"by":"nth","value":"li","index":2}` | Nth element matching selector (0-based) |

All `/find` calls return `{"ref":"@eN","tag":"...","text":"..."}` and accept optional `"action"` (`click`, `fill`, `type`, `hover`, `focus`), `"text"` (for fill/type), and `"exact":true` for exact text matching.

### Mouse / Keyboard

| Method | Path | Body | Description |
|--------|------|------|-------------|
| `POST` | `/mouse` | `{"action":"move","x":100,"y":200}` | Move mouse to coordinates |
| `POST` | `/mouse` | `{"action":"down","x":100,"y":200,"button":"left"}` | Mouse button down |
| `POST` | `/mouse` | `{"action":"up","x":100,"y":200}` | Mouse button up |
| `POST` | `/mouse` | `{"action":"wheel","deltaY":100}` | Mouse wheel scroll |
| `POST` | `/keydown` | `{"key":"Shift"}` | Dispatch keydown event |
| `POST` | `/keyup` | `{"key":"Shift"}` | Dispatch keyup event |

### Highlight

| Method | Path | Body | Description |
|--------|------|------|-------------|
| `POST` | `/highlight` | `{"ref":"@e1"}` | Visually highlight an element with a red overlay (2s duration) |

### Dialogs

| Method | Path | Body | Description |
|--------|------|------|-------------|
| `POST` | `/dialog` | `{"action":"accept"}` | Auto-accept future alert/confirm/prompt dialogs |
| `POST` | `/dialog` | `{"action":"accept","text":"input"}` | Accept with custom prompt text |
| `POST` | `/dialog` | `{"action":"dismiss"}` | Auto-dismiss future dialogs |
| `GET` | `/dialogs` | — | Get and drain captured dialog events |

### Viewport

| Method | Path | Body | Description |
|--------|------|------|-------------|
| `POST` | `/viewport` | `{"width":375,"height":812}` | Resize the webview window |

### State

| Method | Path | Body | Description |
|--------|------|------|-------------|
| `POST` | `/state/save` | — | Save current localStorage, sessionStorage, and cookies |
| `POST` | `/state/load` | `{"localStorage":{...},"sessionStorage":{...},"cookies":{...}}` | Restore previously saved state |

### Screenshot

| Method | Path | Query | Description |
|--------|------|-------|-------------|
| `GET` | `/screenshot` | `?path=/tmp/shot.png` `&full=true` | Take a screenshot. Default path: `/tmp/agent-control-screenshot-<ts>.png`. `full=true` resizes to full page dimensions first. |

### Recording

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/record/start` | Start screen recording (macOS `screencapture -v`). Returns `{pid, path}` |
| `POST` | `/record/stop` | Stop recording (sends SIGINT to screencapture process). Returns `{path}` |

### Download

| Method | Path | Body | Description |
|--------|------|------|-------------|
| `POST` | `/download` | `{"url":"https://...","path":"/tmp/file.zip"}` | Download a file using `curl` |

### Network

| Method | Path | Body | Description |
|--------|------|------|-------------|
| `POST` | `/network/intercept` | — | Start intercepting fetch/XHR requests |
| `POST` | `/network/reset` | — | Stop intercepting and restore original fetch/XHR |
| `GET` | `/network/requests` | — | Get captured network request log |
| `POST` | `/network/route` | `{"url":"/api/data","body":"{\"mock\":true}"}` | Mock responses for matching URLs |
| `POST` | `/network/route` | `{"url":"/api/data","abort":true}` | Block requests to matching URLs |

### Geolocation

| Method | Path | Body | Description |
|--------|------|------|-------------|
| `POST` | `/geo` | `{"lat":37.7749,"lng":-122.4194}` | Override `navigator.geolocation` |

### Offline

| Method | Path | Body | Description |
|--------|------|------|-------------|
| `POST` | `/offline` | `{"enabled":true}` | Simulate offline mode (`navigator.onLine` → false) |

### Headers

| Method | Path | Body | Description |
|--------|------|------|-------------|
| `POST` | `/headers` | `{"headers":{"Authorization":"Bearer token"}}` | Inject custom headers into all future fetch requests |

### Trace

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/trace/start` | Start a `PerformanceObserver` trace (resource, paint, LCP, layout-shift, etc.) |
| `POST` | `/trace/stop` | Stop tracing and return collected performance entries |

### Health

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/health` | Returns `{"ok":true}` — useful to check if the agent-control server is running |

## Ref Lifecycle

Every call to `/snapshot` or `/find` assigns elements a ref like `@e1`, `@e2`, etc. These refs are **invalidated whenever you take a new snapshot** — the ref map is cleared and rebuilt from scratch. If the page navigates or the DOM changes significantly, take a fresh snapshot before interacting.

```bash
# 1. Snapshot → get refs
curl http://localhost:9876/snapshot?format=compact
# @e1 [button] "Save"

# 2. Use the ref
curl -X POST http://localhost:9876/click -d '{"ref":"@e1"}'

# 3. After DOM changes, snapshot again for fresh refs
curl http://localhost:9876/snapshot?format=compact
# @e1 [button] "Saved ✓"   ← refs are reassigned
```

## Semantic Locators (Find)

The `/find` endpoint lets you locate elements without needing a snapshot first. It assigns a new ref to the found element and optionally performs an action in one call.

```bash
# Find a button by role and click it
curl -X POST http://localhost:9876/find \
  -d '{"by":"role","value":"button","name":"Submit","action":"click"}'

# Find an input by label and fill it
curl -X POST http://localhost:9876/find \
  -d '{"by":"label","value":"Email","action":"fill","text":"user@example.com"}'

# Find by test ID
curl -X POST http://localhost:9876/find \
  -d '{"by":"testid","value":"search-input","action":"type","text":"query"}'

# Find the 3rd list item (0-based index)
curl -X POST http://localhost:9876/find \
  -d '{"by":"nth","value":"li","index":2}'
```

Supported locator strategies: `role`, `text`, `label`, `placeholder`, `testid`, `alt`, `title`, `first`, `last`, `nth`.

Use `"exact":true` for exact text/name matching instead of substring matching.

## Network Interception

Intercept, mock, and block network requests made by the webview:

```bash
# Start intercepting
curl -X POST http://localhost:9876/network/intercept

# Mock an API response
curl -X POST http://localhost:9876/network/route \
  -d '{"url":"/api/users","body":"{\"users\":[{\"name\":\"Alice\"}]}"}'

# Block requests to analytics
curl -X POST http://localhost:9876/network/route \
  -d '{"url":"analytics.example.com","abort":true}'

# View captured requests
curl http://localhost:9876/network/requests

# Reset (restore original fetch/XHR)
curl -X POST http://localhost:9876/network/reset
```

The interceptor patches `window.fetch` and `XMLHttpRequest`. Tauri IPC calls (`ipc://` URLs) are automatically excluded to avoid breaking `invoke()`.

## Security

- **Debug-only**: The HTTP server is gated behind `#[cfg(debug_assertions)]` and is completely compiled out of release builds.
- **Localhost-only**: The server listens on `localhost:9876`. It is not accessible from other machines.
- **Not for production**: This plugin gives full control over the webview — eval, DOM manipulation, cookie/storage access. Never ship it in a release build.

## Platform Notes

| Feature | Platform | Dependency |
|---------|----------|------------|
| Screenshot (`/screenshot`) | macOS only | `screencapture` (built-in) |
| Recording (`/record/start`, `/record/stop`) | macOS only | `screencapture -v` (built-in) |
| Download (`/download`) | Cross-platform | `curl` (must be on PATH) |

## Architecture

The plugin is composed of three parts:

1. **Rust HTTP server** — A raw TCP server built with `tokio::net::TcpListener` that manually parses HTTP/1.1 requests. It runs as a spawned async task during plugin setup (debug builds only).

2. **JS shim** (`guest-js/index.js`) — Embedded at compile time via `include_str!("../guest-js/index.js")`. It's injected into the webview on every navigation using Tauri's `on_navigation` hook with a 200ms delay to let the new page settle. The shim exposes `window.__DEVTOOLS__` with all DOM interaction methods.

3. **Eval bridge** — The communication channel between Rust and JS:
   - Rust generates a unique 16-hex-char request ID
   - Rust calls `webview.eval()` with wrapped JS that executes the expression
   - The JS result is sent back via `window.__TAURI_INTERNALS__.invoke('plugin:agent-control|agent_control_respond', {requestId, data})`
   - Rust receives the result through a `tokio::sync::oneshot` channel
   - A configurable timeout (default 5s) cleans up pending requests

```
┌──────────┐   HTTP    ┌────────────┐  eval()   ┌───────────┐
│  Client  │ ───────── │ Rust TCP   │ ────────── │  Webview  │
│ (curl)   │           │ Server     │            │  (JS)     │
└──────────┘           └────────────┘  invoke()  └───────────┘
                             ▲ ──────────────────────┘
                             │  agent_control_respond(id, data)
                             │  via oneshot channel
```

## License

[MIT](https://opensource.org/licenses/MIT)
