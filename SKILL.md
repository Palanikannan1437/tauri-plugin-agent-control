---
name: tauri-agent-control
description: "Observe and control a running Tauri app via curl over HTTP. Use when asked to interact with the app UI, take screenshots, click buttons, fill inputs, inspect elements, debug the webview, intercept network requests, or automate any Tauri desktop app interaction. Triggers on: interact with app, click button, test UI, screenshot app, inspect element, agent-control, automate Tauri app."
---

# Tauri Agent Control HTTP Bridge

Observe and control a running Tauri 2.0 desktop app via HTTP. Works like agent-browser but for Tauri — no Playwright/Puppeteer needed.

## Prerequisites

### Plugin Setup (for Tauri app developers)

1. Add to `Cargo.toml`:
```toml
[dependencies]
tauri-plugin-agent-control = "0.1"
```

2. Register in `src-tauri/src/lib.rs` (where `tauri::Builder` is configured):
```rust
tauri::Builder::default()
    .plugin(tauri_plugin_agent_control::init())
```

3. Add permission in `src-tauri/capabilities/default.json`:
```json
{ "permissions": ["agent-control:default"] }
```

4. Run `cargo tauri dev` — the HTTP bridge starts automatically on port 9876 (debug builds only).

### Verify the bridge is running

```bash
curl -s http://localhost:9876/health
# {"ok":true}
```

## Core Workflow

Every interaction follows this pattern:

1. **Snapshot**: Get interactive elements with refs
2. **Interact**: Use refs to click, fill, press
3. **Re-snapshot**: After navigation or DOM changes, get fresh refs

```bash
# 1. Get compact snapshot (saves context!)
curl -s 'http://localhost:9876/snapshot?format=compact'

# 2. Interact using refs from snapshot
curl -s -X POST http://localhost:9876/click -d '{"ref":"@e1"}'

# 3. Re-snapshot after interaction
curl -s 'http://localhost:9876/snapshot?format=compact'
```

## Snapshot

**ALWAYS use compact format** — it's 90% fewer tokens than JSON.

```bash
# Compact snapshot (RECOMMENDED) — interactive elements only
curl -s 'http://localhost:9876/snapshot?format=compact'
# Output:
# [page] My App — http://localhost:1420/
# @e1 [button] "Open File…"
# @e2 [input] placeholder="Search..."
# @e3 [button] "Submit" disabled

# Scoped snapshot (specific container)
curl -s 'http://localhost:9876/snapshot?format=compact&scope=.sidebar'

# Limit depth
curl -s 'http://localhost:9876/snapshot?format=compact&depth=3'

# Full JSON snapshot (only when you need rect/attributes detail)
curl -s http://localhost:9876/snapshot
```

## Interactions

All interactions use refs from the most recent snapshot.

```bash
# Click
curl -s -X POST http://localhost:9876/click -d '{"ref":"@e1"}'

# Double-click
curl -s -X POST http://localhost:9876/dblclick -d '{"ref":"@e1"}'

# Hover
curl -s -X POST http://localhost:9876/hover -d '{"ref":"@e1"}'

# Focus
curl -s -X POST http://localhost:9876/focus -d '{"ref":"@e1"}'

# Fill input (clears existing value, sets new one)
curl -s -X POST http://localhost:9876/fill -d '{"ref":"@e2","text":"hello world"}'

# Type text character by character (dispatches key events per char)
curl -s -X POST http://localhost:9876/type -d '{"ref":"@e2","text":"hello"}'

# Press key (supports combos: Control+a, Shift+Enter, Meta+c)
curl -s -X POST http://localhost:9876/press -d '{"key":"Enter"}'
curl -s -X POST http://localhost:9876/press -d '{"key":"Control+a"}'

# Check / uncheck checkbox
curl -s -X POST http://localhost:9876/check -d '{"ref":"@e5"}'
curl -s -X POST http://localhost:9876/uncheck -d '{"ref":"@e5"}'

# Select dropdown option(s)
curl -s -X POST http://localhost:9876/select -d '{"ref":"@e3","values":["option1"]}'

# Scroll (direction: up/down/left/right, amount in pixels)
curl -s -X POST http://localhost:9876/scroll -d '{"direction":"down","amount":300}'

# Scroll specific container
curl -s -X POST http://localhost:9876/scroll -d '{"direction":"down","amount":300,"selector":".file-list"}'

# Scroll element into view
curl -s -X POST http://localhost:9876/scrollintoview -d '{"ref":"@e10"}'

# Drag and drop
curl -s -X POST http://localhost:9876/drag -d '{"from":"@e1","to":"@e5"}'

# Upload file (data URL)
curl -s -X POST http://localhost:9876/upload -d '{"ref":"@e1","dataUrl":"data:text/plain;base64,aGVsbG8="}'

# Highlight element (visual red overlay for 2s)
curl -s -X POST http://localhost:9876/highlight -d '{"ref":"@e1"}'
```

## Get Information

```bash
# Get element text
curl -s http://localhost:9876/get/text/@e1

# Get innerHTML
curl -s http://localhost:9876/get/html/@e1

# Get input value
curl -s http://localhost:9876/get/value/@e2

# Get attribute
curl -s http://localhost:9876/get/attr/@e1/class

# Get computed styles
curl -s http://localhost:9876/get/styles/@e1

# Get bounding box
curl -s http://localhost:9876/get/box/@e1

# Get page title
curl -s http://localhost:9876/get/title

# Get current URL
curl -s http://localhost:9876/get/url

# Count elements matching selector (URL-encode the selector)
curl -s http://localhost:9876/get/count/button
```

## Check State

```bash
curl -s http://localhost:9876/is/visible/@e1    # true/false
curl -s http://localhost:9876/is/enabled/@e1    # true/false
curl -s http://localhost:9876/is/checked/@e5    # true/false
```

## Wait

Wait for conditions with polling (default timeout: 5000ms).

```bash
# Wait for element to appear
curl -s -X POST http://localhost:9876/wait -d '{"selector":".loaded"}'

# Wait for text to appear
curl -s -X POST http://localhost:9876/wait -d '{"text":"Success"}'

# Wait for URL to match (** glob)
curl -s -X POST http://localhost:9876/wait -d '{"url":"**/dashboard**"}'

# Wait for JS condition
curl -s -X POST http://localhost:9876/wait -d '{"fn":"window.ready"}'

# Custom timeout
curl -s -X POST http://localhost:9876/wait -d '{"selector":".data","timeout":10000}'
```

## Screenshot

Takes a native macOS screenshot of the app window. Returns a file path.

```bash
curl -s http://localhost:9876/screenshot
# {"path":"/var/folders/.../agent-control-screenshot.png"}
```

Then analyze the returned path with your image viewing tool.

## Console & Errors

```bash
# Get captured console output (and clear buffer)
curl -s http://localhost:9876/console

# Get errors only
curl -s http://localhost:9876/errors
```

## Cookies & Storage

```bash
# Get all cookies
curl -s http://localhost:9876/cookies

# Set cookie
curl -s -X POST http://localhost:9876/cookies/set -d '{"name":"theme","value":"dark"}'

# Clear cookies
curl -s -X POST http://localhost:9876/cookies/clear

# Get all localStorage
curl -s http://localhost:9876/storage/local

# Get specific key
curl -s http://localhost:9876/storage/local/theme

# Get sessionStorage
curl -s http://localhost:9876/storage/session

# Set storage
curl -s -X POST http://localhost:9876/storage/set -d '{"type":"local","key":"theme","value":"dark"}'

# Clear storage
curl -s -X POST http://localhost:9876/storage/clear -d '{"type":"local"}'
```

## JavaScript Eval

Run arbitrary JS in the webview. Use expressions (not `return` statements).

```bash
curl -s -X POST http://localhost:9876/eval -d '{"code":"document.title"}'
curl -s -X POST http://localhost:9876/eval -d '{"code":"document.querySelectorAll(\"button\").length"}'
```

## Find (Semantic Locators)

When refs are unavailable or you need to locate elements semantically:

```bash
# Find by text and click
curl -s -X POST http://localhost:9876/find -d '{"by":"text","value":"Sign In","action":"click"}'

# Find by label and fill
curl -s -X POST http://localhost:9876/find -d '{"by":"label","value":"Email","action":"fill","text":"user@test.com"}'

# Find by role with name filter
curl -s -X POST http://localhost:9876/find -d '{"by":"role","value":"button","name":"Submit","action":"click"}'

# Find by placeholder
curl -s -X POST http://localhost:9876/find -d '{"by":"placeholder","value":"Search...","action":"type","text":"query"}'

# Find by test ID
curl -s -X POST http://localhost:9876/find -d '{"by":"testid","value":"submit-btn","action":"click"}'

# Find by alt text
curl -s -X POST http://localhost:9876/find -d '{"by":"alt","value":"Logo"}'

# Find first/last/nth matching selector
curl -s -X POST http://localhost:9876/find -d '{"by":"first","value":".item"}'
curl -s -X POST http://localhost:9876/find -d '{"by":"last","value":".item"}'
curl -s -X POST http://localhost:9876/find -d '{"by":"nth","value":".item","index":2}'

# Exact match (default is partial/contains)
curl -s -X POST http://localhost:9876/find -d '{"by":"text","value":"Submit","exact":true,"action":"click"}'
```

Supported `by` values: `text`, `label`, `role`, `placeholder`, `testid`, `alt`, `title`, `first`, `last`, `nth`.
Supported `action` values: `click`, `fill`, `type`, `hover`, `focus` (or omit for ref-only).

## Network

Intercept and monitor fetch/XHR requests from the webview.

```bash
# Start intercepting network requests
curl -s -X POST http://localhost:9876/network/intercept

# Get all captured requests
curl -s http://localhost:9876/network/requests
# [{"url":"/api/data","method":"GET","type":"fetch","status":200,"timestamp":...}, ...]

# Route: mock a response
curl -s -X POST http://localhost:9876/network/route -d '{"url":"/api/users","body":"{\"users\":[]}"}'

# Route: block a request
curl -s -X POST http://localhost:9876/network/route -d '{"url":"/api/analytics","abort":true}'

# Reset network interception (restore original fetch/XHR)
curl -s -X POST http://localhost:9876/network/reset
```

## Geolocation, Offline, Headers

```bash
# Override geolocation
curl -s -X POST http://localhost:9876/geo -d '{"lat":37.7749,"lng":-122.4194}'

# Simulate offline mode (blocks fetch, navigator.onLine = false)
curl -s -X POST http://localhost:9876/offline -d '{"enabled":true}'

# Restore online
curl -s -X POST http://localhost:9876/offline -d '{"enabled":false}'

# Inject extra headers on all fetch requests
curl -s -X POST http://localhost:9876/headers -d '{"headers":{"Authorization":"Bearer token123"}}'
```

## State Save/Load

Capture and restore the complete app state (localStorage, sessionStorage, cookies).

```bash
# Save current state
curl -s -X POST http://localhost:9876/state/save
# {"localStorage":{...},"sessionStorage":{...},"cookies":{...}}

# Load previously saved state
curl -s -X POST http://localhost:9876/state/load -d '{"localStorage":{"theme":"dark"},"sessionStorage":{},"cookies":{}}'
```

## Performance Trace

```bash
# Start collecting performance entries (resource, paint, LCP, layout-shift, etc.)
curl -s -X POST http://localhost:9876/trace/start

# ... interact with the app ...

# Stop and get results
curl -s -X POST http://localhost:9876/trace/stop
# [{"name":"http://...","type":"resource","startTime":123,"duration":45}, ...]
```

## Recording

Record the app window as a video (macOS screencapture).

```bash
# Start recording
curl -s -X POST http://localhost:9876/record/start
# {"ok":true,"pid":12345,"path":"/tmp/agent-control-recording-1234567890.mov"}

# Stop recording
curl -s -X POST http://localhost:9876/record/stop
# {"ok":true,"path":"/tmp/agent-control-recording-1234567890.mov"}
```

## Download

Download a file from a URL to a local path.

```bash
curl -s -X POST http://localhost:9876/download -d '{"url":"https://example.com/file.zip","path":"/tmp/file.zip"}'
# {"ok":true,"path":"/tmp/file.zip"}
```

## Viewport

Resize the app window.

```bash
curl -s -X POST http://localhost:9876/viewport -d '{"width":1280,"height":720}'
# {"ok":true}
```

## Dialog Handling

Handle alert/confirm/prompt dialogs.

```bash
# Accept next dialog (optionally provide text for prompt dialogs)
curl -s -X POST http://localhost:9876/dialog -d '{"action":"accept"}'
curl -s -X POST http://localhost:9876/dialog -d '{"action":"accept","text":"my input"}'

# Dismiss next dialog
curl -s -X POST http://localhost:9876/dialog -d '{"action":"dismiss"}'

# Get captured dialogs
curl -s http://localhost:9876/dialogs
# [{"type":"alert","message":"Hello!","timestamp":...}]
```

## Raw Input

Low-level mouse and keyboard control.

```bash
# Mouse move/down/up/wheel
curl -s -X POST http://localhost:9876/mouse -d '{"action":"move","x":100,"y":200}'
curl -s -X POST http://localhost:9876/mouse -d '{"action":"down","x":100,"y":200,"button":"left"}'
curl -s -X POST http://localhost:9876/mouse -d '{"action":"up","x":100,"y":200}'
curl -s -X POST http://localhost:9876/mouse -d '{"action":"wheel","deltaY":100,"x":400,"y":300}'

# Keydown / keyup (for hold-key scenarios)
curl -s -X POST http://localhost:9876/keydown -d '{"key":"Shift"}'
curl -s -X POST http://localhost:9876/keyup -d '{"key":"Shift"}'
```

## Example Workflow

```bash
# 1. Check app is running
curl -s http://localhost:9876/health

# 2. See what's on screen
curl -s 'http://localhost:9876/snapshot?format=compact'
# [page] My App — http://localhost:1420/
# @e1 [button] "Settings"
# @e2 [input] placeholder="Search..."
# @e3 [button] "New Project"

# 3. Click a button
curl -s -X POST http://localhost:9876/click -d '{"ref":"@e3"}'

# 4. Wait for modal to appear
curl -s -X POST http://localhost:9876/wait -d '{"selector":".modal"}'

# 5. Re-snapshot the modal
curl -s 'http://localhost:9876/snapshot?format=compact&scope=.modal'

# 6. Fill in the form
curl -s -X POST http://localhost:9876/fill -d '{"ref":"@e4","text":"My New Project"}'
curl -s -X POST http://localhost:9876/click -d '{"ref":"@e5"}'

# 7. Take a screenshot to verify
curl -s http://localhost:9876/screenshot
```

## Ref Lifecycle

Refs (`@e1`, `@e2`, etc.) are **invalidated on each new snapshot**. Always re-snapshot before interacting after:

- Clicking links or buttons that navigate
- Form submissions
- Dynamic content loading (modals, dropdowns, tabs)

```bash
curl -s -X POST http://localhost:9876/click -d '{"ref":"@e5"}'   # May trigger navigation
curl -s 'http://localhost:9876/snapshot?format=compact'           # MUST re-snapshot
curl -s -X POST http://localhost:9876/click -d '{"ref":"@e1"}'   # Use new refs
```

## Troubleshooting

### Server not responding
The agent-control HTTP server must be enabled in the app. Ensure the plugin is registered and the app is running. Check `curl -s http://localhost:9876/health`.

### Ref not found
Refs are reset on each snapshot. Always re-snapshot before interacting after navigation or DOM changes.

### Timeout on eval
If eval returns `undefined` (e.g., `window.scrollBy()`), the bridge handles it with null coalescing. If you still get timeouts, the JS shim may not be loaded — check `/health` first.

### Virtualized lists
Some lists use virtualization (only render visible items). Scrolling may not change the snapshot immediately. Use `/eval` to check `scrollTop` or scroll the specific container with the `selector` parameter.

### Screenshot not working
Screenshot uses macOS `screencapture` — requires the app to be on screen (not minimized). Only works on macOS.

### Network intercept not capturing
Call `/network/intercept` before the requests you want to capture. Tauri IPC calls (`ipc://`) are automatically excluded to avoid breaking `invoke`.
