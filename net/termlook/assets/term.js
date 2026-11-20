// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

// The terminal page's script (net/termlook: the exchange is
// src/exchange/, the drawing rules src/look/render.rs). It opens the
// page's WebSocket, draws each screen into a canvas of the picture's
// exact size with the console's font and rules, and sends keys by name.
// Without it the page works by posting the keypad.
//
// Its <script> element says where things are:
//   data-socket    the socket's path (or a ws:/wss: URL); without it the
//                  script runs as the 5250 session page it was first
//                  written for: the socket at <base>/5250/ws/<session>,
//                  the 5250 extras on, the keyboard map from
//                  <base>/5250/<session>/keymap/json
//   data-session   the session's identifier
//   data-scale     the integer scale (1)
//   data-scheme    "bw" for black on white
//   data-image     the id of the picture the canvas replaces (else the
//                  img alt="5250 screen")
//   data-5250      the 5250 extras: hotspots, the cursor placed with the
//                  mouse, selection with Copy and Paste
//   data-keymap    a keyboard map's address (JSON, key name to function);
//                  without one the console's own keys
(function () {
  "use strict";
  var me = document.currentScript;
  var id = me.getAttribute("data-session");
  var scale = parseInt(me.getAttribute("data-scale") || "1", 10);
  // The product's path: me.src is absolute, the socket's URL wants a path.
  var base = new URL(me.src, location.href).pathname.replace(/\/5250\/script\.js.*$/, "");
  var socketAt = me.getAttribute("data-socket");
  var legacy = socketAt === null;
  var extras = legacy || me.hasAttribute("data-5250");
  var keymapAt = legacy ? base + "/5250/" + id + "/keymap/json" : me.getAttribute("data-keymap");
  if (legacy) { socketAt = base + "/5250/ws/" + id; }
  var PAL = { bg: "#000000", green: "#33cc33", white: "#e6e6e6", red: "#e03030", blue: "#5080ff", turq: "#20c8c8", pink: "#e060c0", yellow: "#e6e040",
    redhi: "#ff5050", bluehi: "#80a8ff", turqhi: "#40ffff", pinkhi: "#ff80e0", yellowhi: "#ffff60" };
  var COLORS = ["green", "white", "red", "blue", "turq", "pink", "yellow", "green"];
  // The console's own keys, when no keyboard map is given.
  var CONSOLE_KEYS = { Enter: "Enter", Tab: "Tab", "Shift+Tab": "BackTab", "Right Ctrl": "FieldExit", "Keypad +": "FieldExit",
    "Keypad Enter": "Enter", Backspace: "Backspace", Home: "Home", Insert: "Insert", Delete: "Delete", Esc: "Attention",
    "Shift+Esc": "SysReq", "Ctrl+Left": "Reset", Left: "Left", Right: "Right", Up: "Up", Down: "Down",
    "Page Up": "PageUp", "Page Down": "PageDown" };
  for (var f = 1; f <= 12; f++) { CONSOLE_KEYS["F" + f] = "F:" + f; CONSOLE_KEYS["Shift+F" + f] = "F:" + (f + 12); }
  var W = 9, H = 16;
  var screen = null, keymap = keymapAt ? {} : CONSOLE_KEYS, sock = null, img = null, canvas = null, ctx = null;
  var sel = null, drag = null, bw = false;

  // A cell's style: its character code less 0x100 holds the bits.
  function style(code) {
    var b = code - 0x100;
    return { color: COLORS[b & 7], high: (b & 8) !== 0, reverse: (b & 16) !== 0, underline: (b & 32) !== 0,
      sep: (b & 128) !== 0, nd: (b & 256) !== 0, cont: (b & 512) !== 0, dbl: (b & 1024) !== 0 && (b & 512) === 0 };
  }
  function plain(color) { return { color: color, high: false, reverse: false, underline: false, sep: false, nd: false, cont: false, dbl: false }; }
  function fg(st) {
    var c = st.color;
    if (st.high) { c = (c === "green" || c === "white") ? "white" : c + "hi"; }
    return bw && (c === "green" || c === "white") ? PAL.bg : PAL[c];
  }
  function back() { return bw ? PAL.white : PAL.bg; }
  function grey(hex) {
    var b = parseInt(back().slice(1), 16), v = parseInt(hex.slice(1), 16), o = 0;
    for (var s = 0; s <= 16; s += 8) { o |= ((((v >> s) & 255) + ((b >> s) & 255)) >> 1) << s; }
    return "#" + ("000000" + o.toString(16)).slice(-6);
  }
  // Each glyph in its colours is drawn once into a small canvas and
  // copied after; each cell is repainted only when what it shows changed.
  var glyphs = {}, drawn = [];
  function glyph(ch, st, f, b, cur, cursor) {
    var k = ch + "|" + f + "|" + b + "|" + (st.nd ? 1 : 0) + (st.underline ? 1 : 0) + (st.sep ? 1 : 0) + (st.dbl ? 1 : 0) + "|" + (cursor ? cur : "");
    var g = glyphs[k];
    if (g) { return g; }
    var n = st.dbl ? 2 : 1, w = W * n;
    g = document.createElement("canvas");
    g.width = w * scale; g.height = H * scale;
    var x = g.getContext("2d");
    x.fillStyle = b; x.fillRect(0, 0, g.width, g.height);
    var rows = st.nd ? null : (QS_FONT[ch.charCodeAt(0)] || QS_FONT.box);
    x.fillStyle = f;
    for (var gy = 0; gy < H; gy++) {
      if (cursor && gy === H - 1) { x.fillStyle = cur; x.fillRect(0, gy * scale, w * scale, scale); x.fillStyle = f; continue; }
      if (st.underline && gy === H - 2) { x.fillRect(0, gy * scale, w * scale, scale); continue; }
      var bits = rows ? rows[gy] : 0, sep = st.sep && gy % 4 === 0;
      for (var gx = 0; gx < w; gx++) {
        if ((bits & (1 << (W - 1 - Math.floor(gx / n)))) || (sep && (gx === 0 || gx === w - 1))) { x.fillRect(gx * scale, gy * scale, scale, scale); }
      }
    }
    glyphs[k] = g;
    return g;
  }
  function cell(r, c, ch, st, cursor, selected, off) {
    var i = r * 1000 + c;
    if (st.cont) { drawn[i] = null; return; }
    var f = fg(st), b = back();
    if (st.reverse) { var t = f; f = b; b = t; }
    if (selected) { var u = f; f = b; b = u; }
    var cur = bw ? PAL.bg : PAL.white;
    if (off) { f = grey(f); if (b !== back()) { b = grey(b); } cur = grey(cur); }
    var g = glyph(ch, st, f, b, cur, cursor);
    if (drawn[i] === g) { return; }
    drawn[i] = g;
    ctx.drawImage(g, c * W * scale, r * H * scale);
  }
  function oia(s) {
    var cols = s.cols, line = [];
    for (var i = 0; i < cols; i++) { line.push([" ", "green"]); }
    function put(col, text, color, max) { for (var i = 0; i < text.length && i < max; i++) { if (col - 1 + i < cols) { line[col - 1 + i] = [text[i], color]; } } }
    var on = s.state !== "disconnected";
    if (on) { put(1, s.kb, "white", 9); if (s.mw) { put(12, "MW", "green", 2); } if (s.ins) { put(16, "INS", "green", 3); } if (s.tls) { put(21, "TLS", "green", 3); } }
    put(26, on ? s.system : s.system + " Not connected", "green", Math.max(0, cols - 33));
    var rc = ("0" + s.cursor[0]).slice(-2) + "/" + ("00" + s.cursor[1]).slice(-3);
    put(Math.max(1, cols - 5), rc, "green", 6);
    return line;
  }
  function draw() {
    if (!screen) { return; }
    var off = screen.state === "disconnected";
    for (var r = 0; r < screen.rows; r++) {
      var row = screen.cells[r];
      for (var c = 0; c < screen.cols; c++) {
        var st = style(row[1].charCodeAt(c));
        var isSel = sel && r >= sel[0] && r <= sel[2] && c >= sel[1] && c <= sel[3];
        var here = r === screen.cursor[0] - 1 && c === screen.cursor[1] - 1;
        if (here && screen.shape === "block") { isSel = !isSel; }
        cell(r, c, row[0][c], st, here && screen.shape !== "block" && screen.shape !== "hidden", isSel, off);
      }
    }
    var st = oia(screen);
    for (var i = 0; i < st.length; i++) { cell(screen.rows, i, st[i][0], plain(st[i][1]), false, false, off); }
  }
  function apply(m) {
    if (m.t === "screen" || !screen) {
      if (m.t !== "screen" && m.t !== "delta") { return; }
      screen = { rows: m.rows, cols: m.cols, cells: [] };
      for (var r = 0; r < m.rows; r++) { screen.cells.push([new Array(m.cols + 1).join(" "), new Array(m.cols + 1).join("Ā")]); }
      size(m.rows, m.cols);
    }
    var rows = m.cells || [];
    for (var i = 0; i < rows.length; i++) { screen.cells[rows[i][0]] = [rows[i][1], rows[i][2]]; }
    screen.cursor = m.cursor; screen.shape = m.shape || "underline"; screen.state = m.state; screen.system = m.system; screen.tls = m.tls;
    screen.kb = m.kb; screen.ins = m.ins; screen.mw = m.mw;
    draw();
    status();
  }
  function closed(reason) {
    if (!screen) { return; }
    screen.state = "disconnected";
    if (reason) { canvas.title = reason; }
    draw();
  }
  // The status bar's cursor indicator, as the server would write it.
  function status() {
    var lists = document.querySelectorAll("td.content table.list"), bar = lists[lists.length - 1];
    var num = bar && bar.querySelector("td.num");
    if (!num || screen.state === "disconnected") { return; }
    var rc = ("0" + screen.cursor[0]).slice(-2) + "/" + ("00" + screen.cursor[1]).slice(-3);
    num.textContent = (screen.tls ? "TLS   " : "") + rc;
  }
  // Menu names and Scale fetch their page and swap only what changed,
  // so the screen and its socket stay; anything else navigates.
  function inPlace(a) {
    var u = new URL(a.href, location.href);
    if (u.pathname !== location.pathname) { return false; }
    var menu = u.searchParams.get("menu"), sc = u.searchParams.get("scale");
    if (!menu && !sc) { return false; }
    var x = new XMLHttpRequest();
    x.open("GET", u.href);
    x.onload = function () {
      if (x.status !== 200) { location.href = u.href; return; }
      var doc = new DOMParser().parseFromString(x.responseText, "text/html");
      if (menu) {
        var mine = document.querySelector("td.content table.list"), theirs = doc.querySelector("td.content table.list");
        if (mine && theirs) { mine.innerHTML = theirs.innerHTML; }
      }
      if (sc) {
        var tb = document.querySelector("div.toolbar"), ntb = doc.querySelector("div.toolbar");
        if (tb && ntb) { tb.innerHTML = ntb.innerHTML; }
        scale = parseInt(sc, 10) || 1;
        if (screen) { size(screen.rows, screen.cols); draw(); }
      }
      canvas.focus();
    };
    x.onerror = function () { location.href = u.href; };
    x.send();
    return true;
  }
  function size(rows, cols) {
    canvas.width = cols * W * scale; canvas.height = (rows + 1) * H * scale;
    canvas.style.width = canvas.width + "px"; canvas.style.height = canvas.height + "px";
    glyphs = {}; drawn = [];
  }
  function send(o) { if (sock && sock.readyState === 1) { sock.send(JSON.stringify(o)); return true; } return false; }
  function keyName(e) {
    var k = e.key, n;
    if (k === "Control" || k === "Shift" || k === "Alt" || k === "Meta") { return e.code === "ControlRight" ? "Right Ctrl" : null; }
    var named = { Enter: "Enter", Escape: "Esc", Tab: "Tab", Insert: "Insert", Delete: "Delete", Backspace: "Backspace", Home: "Home", End: "End",
      PageUp: "Page Up", PageDown: "Page Down", ArrowLeft: "Left", ArrowRight: "Right", ArrowUp: "Up", ArrowDown: "Down", Pause: "Pause" };
    if (e.code === "NumpadAdd") { n = "Keypad +"; } else if (e.code === "NumpadSubtract") { n = "Keypad -"; } else if (e.code === "NumpadEnter") { n = "Keypad Enter"; }
    else if (/^F[0-9]+$/.test(k)) { n = k; } else if (named[k]) { n = named[k]; } else if (k.length === 1) { n = k.toUpperCase(); } else { return null; }
    var mods = (e.ctrlKey ? "Ctrl+" : "") + (e.shiftKey ? "Shift+" : "") + (e.altKey ? "Alt+" : "");
    return mods + n;
  }
  function onKey(e) {
    var name = keyName(e);
    if (name && keymap[name]) { e.preventDefault(); send({ t: "key", k: keymap[name] }); return; }
    if (!e.ctrlKey && !e.altKey && !e.metaKey && e.key.length === 1) { e.preventDefault(); send({ t: "type", s: e.key }); }
  }
  function at(e) {
    var r = canvas.getBoundingClientRect();
    var c = Math.floor((e.clientX - r.left) / (W * scale)), row = Math.floor((e.clientY - r.top) / (H * scale));
    return screen && row < screen.rows && c < screen.cols ? [row, c] : null;
  }
  function hotspot(p) {
    var line = screen.cells[p[0]][0], s = p[1];
    while (s > 0 && line[s - 1] !== " ") { s--; }
    var word = line.slice(s).split(" ")[0], m = /^F([0-9]{1,2})=/.exec(word);
    if (m) { return "F:" + m[1]; }
    if (/^More\.\.\./.test(line.slice(Math.max(0, p[1] - 7), p[1] + 7).trim()) || word === "More...") { return "PageDown"; }
    if (word === "Bottom") { return "PageUp"; }
    return null;
  }
  function selectedText() {
    if (!sel) { return ""; }
    var out = [];
    for (var r = sel[0]; r <= sel[2]; r++) { out.push(screen.cells[r][0].slice(sel[1], sel[3] + 1).replace(/\s+$/, "")); }
    return out.join("\n");
  }
  function start() {
    var named = me.getAttribute("data-image");
    img = named ? document.getElementById(named) : document.querySelector("img[alt='5250 screen']");
    if (!img || !window.WebSocket) { return; }
    canvas = document.createElement("canvas");
    canvas.width = img.width; canvas.height = img.height;
    canvas.setAttribute("tabindex", "0");
    canvas.style.display = "block"; canvas.style.outline = "none";
    ctx = canvas.getContext("2d");
    var proto = location.protocol === "https:" ? "wss:" : "ws:";
    sock = new WebSocket(/^wss?:/.test(socketAt) ? socketAt : proto + "//" + location.host + socketAt);
    sock.onopen = function () { img.parentNode.replaceChild(canvas, img); canvas.focus(); };
    sock.onmessage = function (ev) {
      var m = JSON.parse(ev.data);
      if (m.t === "screen" || m.t === "delta" || m.t === "status") { apply(m); } else if (m.t === "closed") { closed(m.reason); }
    };
    sock.onclose = function () { closed(""); };
    canvas.addEventListener("keydown", onKey);
    if (extras) {
      canvas.addEventListener("mousedown", function (e) { drag = at(e); sel = null; });
      canvas.addEventListener("mousemove", function (e) {
        var p = at(e);
        if (drag && p && (p[0] !== drag[0] || p[1] !== drag[1])) { sel = [Math.min(drag[0], p[0]), Math.min(drag[1], p[1]), Math.max(drag[0], p[0]), Math.max(drag[1], p[1])]; draw(); }
        canvas.style.cursor = p && hotspot(p) ? "pointer" : "default";
      });
      canvas.addEventListener("mouseup", function (e) {
        var p = at(e);
        if (drag && p && !sel) { var k = hotspot(p); if (k) { send({ t: "key", k: k }); } else { send({ t: "cursor", r: p[0] + 1, c: p[1] + 1 }); } }
        drag = null; canvas.focus();
      });
    }
    if (keymapAt) {
      var x = new XMLHttpRequest();
      x.open("GET", keymapAt);
      x.onload = function () { try { keymap = JSON.parse(x.responseText); } catch (err) { keymap = {}; } };
      x.send();
    }
    // The keypad (and with the 5250 extras Copy and Paste) go over the
    // socket while it is open; the menu bar and Scale switch in place.
    document.addEventListener("click", function (e) {
      var t = e.target;
      var a = t && t.closest ? t.closest("a[href]") : null;
      if (a && sock && sock.readyState === 1 && !e.ctrlKey && !e.shiftKey && !e.metaKey && e.button === 0 && inPlace(a)) { e.preventDefault(); return; }
      if (!t || t.tagName !== "INPUT" || t.type !== "submit") { return; }
      if (t.name === "k" && send({ t: "key", k: KEYPAD[t.value] || t.value })) { e.preventDefault(); canvas.focus(); }
      if (!extras) { return; }
      if (t.name === "tb" && t.value === "Copy" && navigator.clipboard && sel) { e.preventDefault(); navigator.clipboard.writeText(selectedText()); }
      if (t.name === "tb" && t.value === "Paste" && navigator.clipboard && navigator.clipboard.readText) {
        e.preventDefault(); navigator.clipboard.readText().then(function (s) { send({ t: "type", s: s.replace(/[\r\n]/g, "") }); });
      }
    });
  }
  bw = me.getAttribute("data-scheme") === "bw";
  if (document.readyState === "loading") { document.addEventListener("DOMContentLoaded", start); } else { start(); }
})();
