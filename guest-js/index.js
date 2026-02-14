((window) => {
  if (window.__DEVTOOLS__) return;

  // ── Ref system ──────────────────────────────────────────────────────
  const refMap = new Map();
  let refCounter = 0;

  const INTERACTIVE_SELECTOR = [
    "button",
    "input",
    "select",
    "textarea",
    "a",
    '[role="button"]',
    '[role="tab"]',
    '[role="link"]',
    '[role="menuitem"]',
    '[role="option"]',
    '[role="checkbox"]',
    '[role="radio"]',
    "[onclick]",
    "[tabindex]",
  ].join(",");

  function resolveRef(ref) {
    const el = refMap.get(ref);
    if (!el) throw new Error(`Ref not found: ${ref}`);
    return el;
  }

  // ── Console capture ─────────────────────────────────────────────────

  const MAX_LOG_ENTRIES = 200;
  const logBuffer = [];

  const originalConsole = {
    log: console.log.bind(console),
    warn: console.warn.bind(console),
    error: console.error.bind(console),
    info: console.info.bind(console),
    debug: console.debug.bind(console),
  };

  function stringify(val) {
    if (val === null) return "null";
    if (val === undefined) return "undefined";
    if (typeof val === "string") return val;
    if (val instanceof Error) return `${val.name}: ${val.message}`;
    try {
      return JSON.stringify(val);
    } catch {
      return String(val);
    }
  }

  for (const level of Object.keys(originalConsole)) {
    console[level] = (...args) => {
      originalConsole[level](...args);
      logBuffer.push({
        level,
        args: args.map(stringify),
        timestamp: Date.now(),
      });
      if (logBuffer.length > MAX_LOG_ENTRIES) logBuffer.shift();
    };
  }

  // ── DOM helpers ─────────────────────────────────────────────────────
  function truncate(text, max) {
    const clean = text.replace(/\s+/g, " ").trim();
    return clean.length > max ? clean.slice(0, max) + "…" : clean;
  }

  function visibleText(el) {
    const text =
      el.innerText ?? el.textContent ?? "";
    return truncate(text, 80);
  }

  function isElementVisible(el) {
    const html = el;
    if (html.offsetParent === null && getComputedStyle(el).position !== "fixed")
      return false;
    const style = getComputedStyle(el);
    if (style.display === "none" || style.visibility === "hidden") return false;
    if (parseFloat(style.opacity) === 0) return false;
    const rect = el.getBoundingClientRect();
    return rect.width > 0 && rect.height > 0;
  }

  function rectObj(el) {
    const r = el.getBoundingClientRect();
    return { x: r.x, y: r.y, width: r.width, height: r.height };
  }

  function keyAttributes(el) {
    const attrs = {};
    const pick = [
      "placeholder",
      "name",
      "href",
      "aria-label",
      "role",
      "value",
      "title",
      "alt",
      "data-testid",
    ];
    for (const a of pick) {
      const v = el.getAttribute(a);
      if (v != null) attrs[a] = v;
    }
    return attrs;
  }

  // ── Snapshot ────────────────────────────────────────────────────────

  function collectElements(
    root,
    interactiveOnly,
    maxDepth,
  ) {
    const results = [];

    function walk(node, depth) {
      if (maxDepth != null && depth > maxDepth) return;

      if (interactiveOnly) {
        if (node.matches(INTERACTIVE_SELECTOR)) results.push(node);
      } else {
        results.push(node);
      }

      for (const child of Array.from(node.children)) {
        walk(child, depth + 1);
      }
    }

    walk(root, 0);
    return results;
  }

  function snapshot(opts) {
    const format = opts?.format ?? "json";
    const scope = opts?.scope;
    const interactiveOnly = opts?.interactive !== false;
    const depth = opts?.depth;

    refMap.clear();
    refCounter = 0;

    const root = scope
      ? document.querySelector(scope) ?? document.body
      : document.body;

    const elements = collectElements(root, interactiveOnly, depth);
    const infos = [];

    for (const el of elements) {
      refCounter++;
      const ref = `@e${refCounter}`;
      refMap.set(ref, el);

      const tag = el.tagName.toLowerCase();
      const info = {
        ref,
        tag,
        rect: rectObj(el),
        attributes: keyAttributes(el),
      };

      const inputType = el.getAttribute("type");
      if (inputType) info.type = inputType;

      const text = visibleText(el);
      if (text) info.text = text;

      if (el.disabled !== undefined && el.disabled) {
        info.disabled = true;
      }

      if (el.checked !== undefined && el.checked) {
        info.checked = true;
      }

      infos.push(info);
    }

    if (format === "compact") {
      const lines = [
        `[page] ${document.title} — ${location.href}`,
      ];

      for (const info of infos) {
        let line = `  ${info.ref} [${info.tag}`;
        if (info.type) line += ` type="${info.type}"`;
        line += "]";

        if (info.text) line += ` "${info.text}"`;

        const { placeholder, name, href, "aria-label": ariaLabel, role } =
          info.attributes;
        if (placeholder) line += ` placeholder="${placeholder}"`;
        if (name) line += ` name="${name}"`;
        if (href) line += ` href="${href}"`;
        if (ariaLabel) line += ` aria-label="${ariaLabel}"`;
        if (role) line += ` role="${role}"`;

        if (info.disabled) line += " disabled";
        if (info.checked) line += " checked";

        lines.push(line);
      }

      return lines.join("\n");
    }

    return infos;
  }

  // ── Interaction helpers ─────────────────────────────────────────────
  function click(ref) {
    const el = resolveRef(ref);
    el.click();
  }

  function dblclick(ref) {
    const el = resolveRef(ref);
    el.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
  }

  function hover(ref) {
    const el = resolveRef(ref);
    el.dispatchEvent(new MouseEvent("mouseenter", { bubbles: true }));
    el.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
  }

  function focus(ref) {
    const el = resolveRef(ref);
    el.focus();
  }

  function fill(ref, text) {
    const el = resolveRef(ref);

    if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
      el.value = text;
      el.dispatchEvent(new Event("input", { bubbles: true }));
      el.dispatchEvent(new Event("change", { bubbles: true }));
    } else if (el.isContentEditable) {
      el.textContent = text;
      el.dispatchEvent(new Event("input", { bubbles: true }));
    }
  }

  function type(ref, text) {
    const el = resolveRef(ref);
    el.focus();

    for (const char of text) {
      const shared = { key: char, bubbles: true };
      el.dispatchEvent(new KeyboardEvent("keydown", shared));
      el.dispatchEvent(new KeyboardEvent("keypress", shared));
      if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
        el.value += char;
      }
      el.dispatchEvent(new InputEvent("input", { bubbles: true, data: char }));
      el.dispatchEvent(new KeyboardEvent("keyup", shared));
    }
  }

  function press(key) {
    const parts = key.split("+");
    const mainKey = parts.pop();

    const modifiers = new Set(parts.map((p) => p.toLowerCase()));

    const opts = {
      key: mainKey,
      bubbles: true,
      ctrlKey: modifiers.has("control") || modifiers.has("ctrl"),
      shiftKey: modifiers.has("shift"),
      altKey: modifiers.has("alt"),
      metaKey: modifiers.has("meta") || modifiers.has("command") || modifiers.has("cmd"),
    };

    const target = document.activeElement || document.body;
    target.dispatchEvent(new KeyboardEvent("keydown", opts));
    target.dispatchEvent(new KeyboardEvent("keyup", opts));
  }

  function check(ref) {
    const el = resolveRef(ref);
    el.checked = true;
    el.dispatchEvent(new Event("change", { bubbles: true }));
    el.dispatchEvent(new Event("input", { bubbles: true }));
  }

  function uncheck(ref) {
    const el = resolveRef(ref);
    el.checked = false;
    el.dispatchEvent(new Event("change", { bubbles: true }));
    el.dispatchEvent(new Event("input", { bubbles: true }));
  }

  function select(ref, values) {
    const el = resolveRef(ref);
    if (el.tagName !== "SELECT") throw new Error("Element is not a <select>");
    const valueSet = new Set(values);

    for (const option of Array.from(el.options)) {
      option.selected = valueSet.has(option.value);
    }

    el.dispatchEvent(new Event("change", { bubbles: true }));
  }

  function scroll(opts) {
    const dx =
      opts.direction === "left"
        ? -opts.amount
        : opts.direction === "right"
          ? opts.amount
          : 0;
    const dy =
      opts.direction === "up"
        ? -opts.amount
        : opts.direction === "down"
          ? opts.amount
          : 0;

    if (opts.selector) {
      const container = document.querySelector(opts.selector);
      container?.scrollBy(dx, dy);
    } else {
      window.scrollBy(dx, dy);
    }
  }

  function scrollIntoView(ref) {
    const el = resolveRef(ref);
    el.scrollIntoView({ behavior: "smooth", block: "center" });
  }

  function drag(fromRef, toRef) {
    const from = resolveRef(fromRef);
    const to = resolveRef(toRef);

    const dt = new DataTransfer();

    from.dispatchEvent(
      new DragEvent("dragstart", { bubbles: true, dataTransfer: dt }),
    );
    to.dispatchEvent(
      new DragEvent("dragenter", { bubbles: true, dataTransfer: dt }),
    );
    to.dispatchEvent(
      new DragEvent("dragover", { bubbles: true, dataTransfer: dt }),
    );
    to.dispatchEvent(
      new DragEvent("drop", { bubbles: true, dataTransfer: dt }),
    );
    from.dispatchEvent(
      new DragEvent("dragend", { bubbles: true, dataTransfer: dt }),
    );
  }

  function upload(ref, dataUrl) {
    const el = resolveRef(ref);

    const [header, base64] = dataUrl.split(",");
    const mimeMatch = header.match(/data:([^;]+)/);
    const mime = mimeMatch ? mimeMatch[1] : "application/octet-stream";

    const binary = atob(base64);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) {
      bytes[i] = binary.charCodeAt(i);
    }

    const file = new File([bytes], "upload", { type: mime });
    const dt = new DataTransfer();
    dt.items.add(file);
    el.files = dt.files;
    el.dispatchEvent(new Event("change", { bubbles: true }));
  }

  // ── Getters ─────────────────────────────────────────────────────────
  function getText(ref) {
    return resolveRef(ref).textContent ?? "";
  }

  function getHtml(ref) {
    return resolveRef(ref).innerHTML;
  }

  function getValue(ref) {
    return resolveRef(ref).value;
  }

  function getAttr(ref, attr) {
    return resolveRef(ref).getAttribute(attr);
  }

  function getStyles(ref) {
    const cs = getComputedStyle(resolveRef(ref));
    return {
      font: cs.font,
      fontSize: cs.fontSize,
      fontWeight: cs.fontWeight,
      color: cs.color,
      backgroundColor: cs.backgroundColor,
      display: cs.display,
      visibility: cs.visibility,
      opacity: cs.opacity,
      position: cs.position,
      overflow: cs.overflow,
      zIndex: cs.zIndex,
      margin: cs.margin,
      padding: cs.padding,
      border: cs.border,
      width: cs.width,
      height: cs.height,
    };
  }

  function getBox(ref) {
    return rectObj(resolveRef(ref));
  }

  function getCount(selector) {
    return document.querySelectorAll(selector).length;
  }

  function isVisible(ref) {
    return isElementVisible(resolveRef(ref));
  }

  function isEnabled(ref) {
    return !resolveRef(ref).disabled;
  }

  function isChecked(ref) {
    const el = resolveRef(ref);
    return el.checked ?? false;
  }

  // ── Wait ────────────────────────────────────────────────────────────
  function wait(opts) {
    const timeout = opts.timeout ?? 5000;
    const interval = 100;

    return new Promise((resolve, reject) => {
      const start = Date.now();

      function check() {
        if (opts.selector) return !!document.querySelector(opts.selector);
        if (opts.text)
          return (document.body.textContent ?? "").includes(opts.text);
        if (opts.url) {
          const pattern = opts.url
            .replace(/[.+^${}()|[\]\\]/g, "\\$&")
            .replace(/\*\*/g, "@@GLOBSTAR@@")
            .replace(/\*/g, "[^/]*")
            .replace(/@@GLOBSTAR@@/g, ".*");
          return new RegExp(`^${pattern}$`).test(location.href);
        }
        if (opts.fn) {
          try {
            return !!eval(opts.fn);
          } catch {
            return false;
          }
        }
        return true;
      }

      function poll() {
        if (check()) return resolve(true);
        if (Date.now() - start >= timeout) return reject(new Error(`Wait timed out after ${timeout}ms`));
        setTimeout(poll, interval);
      }

      poll();
    });
  }

  // ── Console access ──────────────────────────────────────────────────
  function getConsole() {
    const entries = logBuffer.splice(0, logBuffer.length);
    return entries;
  }

  function getErrors() {
    const errors = [];
    for (let i = logBuffer.length - 1; i >= 0; i--) {
      if (logBuffer[i].level === "error") {
        errors.push(logBuffer[i]);
        logBuffer.splice(i, 1);
      }
    }
    return errors.reverse();
  }

  // ── Cookies & Storage ───────────────────────────────────────────────
  function getCookies() {
    const cookies = {};
    if (!document.cookie) return cookies;
    for (const pair of document.cookie.split(";")) {
      const idx = pair.indexOf("=");
      if (idx === -1) continue;
      const name = pair.slice(0, idx).trim();
      const value = pair.slice(idx + 1).trim();
      cookies[name] = decodeURIComponent(value);
    }
    return cookies;
  }

  function setCookie(name, value) {
    document.cookie = `${encodeURIComponent(name)}=${encodeURIComponent(value)};path=/`;
  }

  function clearCookies() {
    for (const name of Object.keys(getCookies())) {
      document.cookie = `${name}=;expires=${new Date(0).toUTCString()};path=/`;
    }
  }

  function resolveStorage(type) {
    return type === "session" ? sessionStorage : localStorage;
  }

  function getStorage(type) {
    const s = resolveStorage(type);
    const result = {};
    for (let i = 0; i < s.length; i++) {
      const key = s.key(i);
      result[key] = s.getItem(key);
    }
    return result;
  }

  function getStorageKey(type, key) {
    return resolveStorage(type).getItem(key);
  }

  function setStorage(type, key, value) {
    resolveStorage(type).setItem(key, value);
  }

  function clearStorage(type) {
    resolveStorage(type).clear();
  }

  // ── Semantic locators (find) ──────────────────────────────────────
  function find(opts) {
    let el = null;

    switch (opts.by) {
      case "role": {
        const candidates = Array.from(
          document.querySelectorAll(`[role="${opts.value}"], ${opts.value ?? ""}`)
        );
        if (opts.name) {
          const nameLC = opts.name.toLowerCase();
          el = candidates.find((c) => {
            const label =
              c.getAttribute("aria-label") ??
              c.textContent ??
              "";
            return opts.exact
              ? label === opts.name
              : label.toLowerCase().includes(nameLC);
          }) ?? null;
        } else {
          el = candidates[0] ?? null;
        }
        break;
      }
      case "text": {
        const walker = document.createTreeWalker(
          document.body,
          NodeFilter.SHOW_ELEMENT,
        );
        let node;
        let candidate = null;
        while ((node = walker.nextNode())) {
          const text = node.innerText ?? node.textContent ?? "";
          if (opts.exact ? text.trim() === opts.value : text.includes(opts.value ?? "")) {
            candidate = node;
          }
        }
        el = candidate;
        break;
      }
      case "label": {
        const labels = Array.from(document.querySelectorAll("label"));
        for (const label of labels) {
          const text = label.textContent ?? "";
          if (text.includes(opts.value ?? "")) {
            const forId = label.getAttribute("for");
            if (forId) {
              el = document.getElementById(forId);
            } else {
              el = label.querySelector("input,select,textarea");
            }
            if (el) break;
          }
        }
        if (!el) {
          el = document.querySelector(`[aria-label="${opts.value}"]`);
        }
        break;
      }
      case "placeholder":
        el = document.querySelector(`[placeholder="${opts.value}"]`);
        break;
      case "testid":
        el = document.querySelector(`[data-testid="${opts.value}"]`);
        break;
      case "first":
        el = document.querySelector(opts.value ?? "*");
        break;
      case "last": {
        const all = document.querySelectorAll(opts.value ?? "*");
        el = all.length > 0 ? all[all.length - 1] : null;
        break;
      }
      case "nth": {
        const all = document.querySelectorAll(opts.value ?? "*");
        const n = opts.index ?? opts.n ?? 0;
        el = all[n] ?? null;
        break;
      }
      case "alt":
        el = document.querySelector(`[alt="${opts.value}"]`);
        break;
      case "title":
        el = document.querySelector(`[title="${opts.value}"]`);
        break;
      default:
        throw new Error(`Unknown find by: ${opts.by}`);
    }

    if (!el) throw new Error(`Element not found: by=${opts.by} value=${opts.value}`);

    refCounter++;
    const ref = `@e${refCounter}`;
    refMap.set(ref, el);

    switch (opts.action) {
      case "click":
        el.click();
        break;
      case "fill":
        if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
          el.value = opts.text ?? "";
          el.dispatchEvent(new Event("input", { bubbles: true }));
          el.dispatchEvent(new Event("change", { bubbles: true }));
        }
        break;
      case "type":
        el.focus();
        for (const char of (opts.text ?? "")) {
          const shared = { key: char, bubbles: true };
          el.dispatchEvent(new KeyboardEvent("keydown", shared));
          el.dispatchEvent(new KeyboardEvent("keypress", shared));
          if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
            el.value += char;
          }
          el.dispatchEvent(new InputEvent("input", { bubbles: true, data: char }));
          el.dispatchEvent(new KeyboardEvent("keyup", shared));
        }
        break;
      case "hover":
        el.dispatchEvent(new MouseEvent("mouseenter", { bubbles: true }));
        el.dispatchEvent(new MouseEvent("mouseover", { bubbles: true }));
        break;
      case "focus":
        el.focus();
        break;
      default:
        break;
    }

    return { ref, tag: el.tagName.toLowerCase(), text: visibleText(el) };
  }

  // ── Raw mouse control ─────────────────────────────────────────────
  function mouse(opts) {
    const buttonMap = { left: 0, middle: 1, right: 2 };
    const btn = buttonMap[opts.button ?? "left"] ?? 0;

    switch (opts.action) {
      case "move":
        document.dispatchEvent(
          new MouseEvent("mousemove", {
            clientX: opts.x ?? 0,
            clientY: opts.y ?? 0,
            bubbles: true,
          }),
        );
        break;
      case "down":
        document.dispatchEvent(
          new MouseEvent("mousedown", {
            clientX: opts.x ?? 0,
            clientY: opts.y ?? 0,
            button: btn,
            bubbles: true,
          }),
        );
        break;
      case "up":
        document.dispatchEvent(
          new MouseEvent("mouseup", {
            clientX: opts.x ?? 0,
            clientY: opts.y ?? 0,
            button: btn,
            bubbles: true,
          }),
        );
        break;
      case "wheel":
        document.dispatchEvent(
          new WheelEvent("wheel", {
            deltaY: opts.deltaY ?? 0,
            clientX: opts.x ?? 0,
            clientY: opts.y ?? 0,
            bubbles: true,
          }),
        );
        break;
      default:
        throw new Error(`Unknown mouse action: ${opts.action}`);
    }
  }

  // ── Separate keydown/keyup ────────────────────────────────────────
  function keydown(key) {
    const target = document.activeElement || document.body;
    target.dispatchEvent(
      new KeyboardEvent("keydown", { key, bubbles: true }),
    );
  }

  function keyup(key) {
    const target = document.activeElement || document.body;
    target.dispatchEvent(
      new KeyboardEvent("keyup", { key, bubbles: true }),
    );
  }

  // ── Highlight element ─────────────────────────────────────────────
  function highlight(ref) {
    const el = resolveRef(ref);
    const rect = el.getBoundingClientRect();
    const overlay = document.createElement("div");
    overlay.style.cssText = [
      "position:fixed",
      `top:${rect.top}px`,
      `left:${rect.left}px`,
      `width:${rect.width}px`,
      `height:${rect.height}px`,
      "border:2px solid red",
      "background:rgba(255,0,0,0.1)",
      "z-index:2147483647",
      "pointer-events:none",
    ].join(";");
    document.body.appendChild(overlay);
    setTimeout(() => overlay.remove(), 2000);
  }

  // ── Dialog handling ───────────────────────────────────────────────
  const dialogQueue = [];
  let dialogAction = "accept";
  let dialogText = undefined;

  const originalAlert = window.alert.bind(window);
  const originalConfirm = window.confirm.bind(window);
  const originalPrompt = window.prompt.bind(window);

  window.alert = (message) => {
    dialogQueue.push({
      type: "alert",
      message: String(message ?? ""),
      timestamp: Date.now(),
    });
  };

  window.confirm = (message) => {
    dialogQueue.push({
      type: "confirm",
      message: String(message ?? ""),
      timestamp: Date.now(),
    });
    return dialogAction === "accept";
  };

  window.prompt = (message, defaultValue) => {
    dialogQueue.push({
      type: "prompt",
      message: String(message ?? ""),
      defaultValue,
      timestamp: Date.now(),
    });
    if (dialogAction === "dismiss") return null;
    return dialogText ?? defaultValue ?? "";
  };

  function dialogAccept(text) {
    dialogAction = "accept";
    dialogText = text;
  }

  function dialogDismiss() {
    dialogAction = "dismiss";
    dialogText = undefined;
  }

  function getDialogs() {
    return dialogQueue.splice(0, dialogQueue.length);
  }

  // ── State save/load ───────────────────────────────────────────────
  function saveState() {
    return {
      localStorage: getStorage("local"),
      sessionStorage: getStorage("session"),
      cookies: getCookies(),
    };
  }

  function loadState(state) {
    if (state.localStorage) {
      localStorage.clear();
      for (const [k, v] of Object.entries(state.localStorage)) {
        localStorage.setItem(k, v);
      }
    }
    if (state.sessionStorage) {
      sessionStorage.clear();
      for (const [k, v] of Object.entries(state.sessionStorage)) {
        sessionStorage.setItem(k, v);
      }
    }
    if (state.cookies) {
      clearCookies();
      for (const [k, v] of Object.entries(state.cookies)) {
        setCookie(k, v);
      }
    }
  }

  // ── Network interception ─────────────────────────────────────────
  const networkLog = [];
  const networkRoutes = [];
  let networkIntercepting = false;
  const originalFetch = window.fetch.bind(window);
  const originalXHROpen = XMLHttpRequest.prototype.open;
  const originalXHRSend = XMLHttpRequest.prototype.send;

  function networkIntercept() {
    if (networkIntercepting) return;
    networkIntercepting = true;
    networkLog.length = 0;

    window.fetch = async (input, init) => {
      const url = typeof input === "string" ? input : input.url;
      // Skip Tauri IPC calls to avoid breaking invoke
      if (url && url.startsWith("ipc://")) return originalFetch(input, init);
      if (offlineEnabled) {
        const entry = { url, method: init?.method ?? "GET", type: "fetch", timestamp: Date.now(), blocked: true };
        networkLog.push(entry);
        throw new TypeError("Failed to fetch");
      }
      const method = init?.method ?? "GET";
      const entry = { url, method, type: "fetch", timestamp: Date.now() };

      for (const route of networkRoutes) {
        if (url.includes(route.url)) {
          if (route.abort) {
            entry.aborted = true;
            networkLog.push(entry);
            throw new TypeError("Failed to fetch (blocked by devtools)");
          }
          if (route.body !== undefined) {
            entry.mocked = true;
            networkLog.push(entry);
            return new Response(route.body, { status: 200, headers: { "Content-Type": "application/json" } });
          }
        }
      }

      const mergedInit = Object.keys(extraHeaders).length > 0
        ? { ...init, headers: { ...(init?.headers || {}), ...extraHeaders } }
        : init;
      const resp = await originalFetch(input, mergedInit);
      entry.status = resp.status;
      networkLog.push(entry);
      return resp;
    };

    XMLHttpRequest.prototype.open = function (method, url, ...rest) {
      this.__dt_method = method;
      this.__dt_url = url;
      return originalXHROpen.call(this, method, url, ...rest);
    };

    XMLHttpRequest.prototype.send = function (body) {
      const entry = { url: this.__dt_url, method: this.__dt_method, type: "xhr", timestamp: Date.now() };

      for (const route of networkRoutes) {
        if (this.__dt_url && this.__dt_url.includes(route.url)) {
          if (route.abort) {
            entry.aborted = true;
            networkLog.push(entry);
            this.dispatchEvent(new Event("error"));
            return;
          }
        }
      }

      this.addEventListener("load", () => {
        entry.status = this.status;
        networkLog.push(entry);
      });
      return originalXHRSend.call(this, body);
    };
  }

  function networkReset() {
    networkIntercepting = false;
    networkRoutes.length = 0;
    window.fetch = originalFetch;
    XMLHttpRequest.prototype.open = originalXHROpen;
    XMLHttpRequest.prototype.send = originalXHRSend;
  }

  function networkRequests() {
    return networkLog.slice();
  }

  function networkRoute(opts) {
    networkRoutes.push(opts);
    if (!networkIntercepting) networkIntercept();
  }

  // ── Geolocation override ─────────────────────────────────────────
  const originalGetCurrentPosition = navigator.geolocation.getCurrentPosition.bind(navigator.geolocation);
  const originalWatchPosition = navigator.geolocation.watchPosition.bind(navigator.geolocation);
  let geoOverride = null;

  function setGeo(lat, lng) {
    geoOverride = { lat, lng };
    navigator.geolocation.getCurrentPosition = (success) => {
      success({ coords: { latitude: lat, longitude: lng, accuracy: 1, altitude: null, altitudeAccuracy: null, heading: null, speed: null }, timestamp: Date.now() });
    };
    navigator.geolocation.watchPosition = (success) => {
      success({ coords: { latitude: lat, longitude: lng, accuracy: 1, altitude: null, altitudeAccuracy: null, heading: null, speed: null }, timestamp: Date.now() });
      return 0;
    };
  }

  // ── Offline mode ─────────────────────────────────────────────────
  let offlineEnabled = false;
  const originalOnLineGetter = Object.getOwnPropertyDescriptor(Navigator.prototype, "onLine")?.get;

  function setOffline(enabled) {
    offlineEnabled = enabled;
    if (enabled) {
      if (!networkIntercepting) networkIntercept();
      Object.defineProperty(navigator, "onLine", { get: () => false, configurable: true });
    } else {
      if (originalOnLineGetter) {
        Object.defineProperty(navigator, "onLine", { get: originalOnLineGetter, configurable: true });
      }
    }
  }

  // ── Header injection ─────────────────────────────────────────────
  let extraHeaders = {};

  function setHeaders(headers) {
    extraHeaders = headers;
    if (!networkIntercepting) networkIntercept();
  }

  // ── Performance tracing ──────────────────────────────────────────
  let perfObserver = null;
  let perfEntries = [];

  function traceStart() {
    perfEntries = [];
    const supported = PerformanceObserver.supportedEntryTypes || [];
    const wanted = ["resource", "mark", "measure", "navigation", "paint", "longtask", "largest-contentful-paint", "layout-shift"];
    const types = wanted.filter(t => supported.includes(t));
    if (types.length === 0) types.push("resource");
    perfObserver = new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        perfEntries.push({ name: entry.name, type: entry.entryType, startTime: entry.startTime, duration: entry.duration });
      }
    });
    perfObserver.observe({ entryTypes: types });
  }

  function traceStop() {
    if (perfObserver) {
      perfObserver.disconnect();
      perfObserver = null;
    }
    const result = perfEntries.slice();
    perfEntries = [];
    return result;
  }

  // ── Expose ──────────────────────────────────────────────────────────
  window.__DEVTOOLS__ = {
    snapshot,
    click,
    dblclick,
    hover,
    focus,
    fill,
    type,
    press,
    check,
    uncheck,
    select,
    scroll,
    scrollIntoView,
    drag,
    upload,
    getText,
    getHtml,
    getValue,
    getAttr,
    getStyles,
    getBox,
    getCount,
    isVisible,
    isEnabled,
    isChecked,
    wait,
    getConsole,
    getErrors,
    getCookies,
    setCookie,
    clearCookies,
    getStorage,
    getStorageKey,
    setStorage,
    clearStorage,
    find,
    mouse,
    keydown,
    keyup,
    highlight,
    dialogAccept,
    dialogDismiss,
    getDialogs,
    saveState,
    loadState,
    networkIntercept,
    networkReset,
    networkRequests,
    networkRoute,
    setGeo,
    setOffline,
    setHeaders,
    traceStart,
    traceStop,
  };
})(window);
