// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

// The 5250 display page's script. It opens the session's WebSocket,
// draws each screen into a canvas of the picture's exact size with the console's font and rules
// (the server's render() in net/accessweb/src/s5250/look/render.rs),
// maps keys through the session's keyboard map, and handles the mouse:
// hotspots, cursor placing, rectangle selection, Copy and Paste.
// Without it the page works by posting the keypad and toolbar.
(function () {
  "use strict";
  var me = document.currentScript;
  var id = me.getAttribute("data-session");
  var scale = parseInt(me.getAttribute("data-scale") || "1", 10);
  // The product's path: me.src is absolute, the socket's URL wants a path.
  var base = new URL(me.src, location.href).pathname.replace(/\/5250\/script\.js.*$/, "");
  var PAL = { bg: "#000000", green: "#33cc33", white: "#e6e6e6", red: "#e03030", blue: "#5080ff", turq: "#20c8c8", pink: "#e060c0" };
  var W = 9, H = 16;
  var screen = null, keymap = {}, sock = null, img = null, canvas = null, ctx = null;
  var sel = null, drag = null, bw = false;

  function style(a) {
    var c;
    switch (a & 0x18) {
      case 0x00: c = (a & 0x02) ? "white" : "green"; break;
      case 0x08: c = "red"; break;
      case 0x10: c = "turq"; break;
      default: c = (a & 0x02) ? "blue" : "pink";
    }
    var nd = (a & 0x07) === 0x07;
    return { color: c, reverse: !nd && (a & 0x01) !== 0, underline: !nd && (a & 0x04) !== 0, sep: a >= 0x30 && a <= 0x37, nd: nd };
  }
  function fg(c) { return bw && (c === "green" || c === "white") ? PAL.bg : PAL[c]; }
  function back() { return bw ? PAL.white : PAL.bg; }
  function grey(hex) {
    var b = parseInt(back().slice(1), 16), v = parseInt(hex.slice(1), 16), o = 0;
    for (var s = 0; s <= 16; s += 8) { o |= ((((v >> s) & 255) + ((b >> s) & 255)) >> 1) << s; }
    return "#" + ("000000" + o.toString(16)).slice(-6);
  }
  function cell(r, c, ch, st, cursor, selected, off) {
    var f = fg(st.color), b = back();
    if (st.reverse) { var t = f; f = b; b = t; }
    if (selected) { var u = f; f = b; b = u; }
    var cur = bw ? PAL.bg : PAL.white;
    if (off) { f = grey(f); if (b !== back()) { b = grey(b); } cur = grey(cur); }
    var x0 = c * W * scale, y0 = r * H * scale;
    var rows = st.nd ? null : (QS_FONT[ch.charCodeAt(0)] || QS_FONT.box);
    for (var gy = 0; gy < H; gy++) {
      if (cursor && gy === H - 1) { ctx.fillStyle = cur; ctx.fillRect(x0, y0 + gy * scale, W * scale, scale); continue; }
      if (st.underline && gy === H - 2) { ctx.fillStyle = f; ctx.fillRect(x0, y0 + gy * scale, W * scale, scale); continue; }
      var bits = rows ? rows[gy] : 0;
      if (st.sep && gy % 4 === 0) { bits |= 0x101; }
      for (var gx = 0; gx < W; gx++) {
        ctx.fillStyle = (bits & (1 << (W - 1 - gx))) ? f : b;
        ctx.fillRect(x0 + gx * scale, y0 + gy * scale, scale, scale);
      }
    }
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
        var code = row[1].charCodeAt(c);
        var st = row[1][c] === "." ? style(0x20) : style(0x20 | (code - 64));
        var isSel = sel && r >= sel[0] && r <= sel[2] && c >= sel[1] && c <= sel[3];
        cell(r, c, row[1][c] === "." ? " " : row[0][c], st, r === screen.cursor[0] - 1 && c === screen.cursor[1] - 1, isSel, off);
      }
    }
    var st = oia(screen);
    for (var i = 0; i < st.length; i++) { cell(screen.rows, i, st[i][0], style(st[i][1] === "white" ? 0x22 : 0x20), false, false, off); }
  }
  function apply(m) {
    if (m.t === "screen" || !screen) {
      screen = { rows: m.rows, cols: m.cols, cells: [] };
      for (var r = 0; r < m.rows; r++) { screen.cells.push([new Array(m.cols + 1).join(" "), new Array(m.cols + 1).join("@")]); }
      size(m.rows, m.cols);
    }
    for (var i = 0; i < m.cells.length; i++) { screen.cells[m.cells[i][0]] = [m.cells[i][1], m.cells[i][2]]; }
    screen.cursor = m.cursor; screen.state = m.state; screen.system = m.system; screen.tls = m.tls;
    screen.kb = m.kb; screen.ins = m.ins; screen.mw = m.mw;
    draw();
    status();
  }
  // The status bar's cursor indicator, as the server would write it.
  function status() {
    var lists = document.querySelectorAll("td.content table.list"), bar = lists[lists.length - 1];
    var num = bar && bar.querySelector("td.num");
    if (!num || screen.state === "disconnected") { return; }
    var rc = ("0" + screen.cursor[0]).slice(-2) + "/" + ("00" + screen.cursor[1]).slice(-3);
    num.textContent = (screen.tls ? "TLS   " : "") + rc;
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
    img = document.querySelector("img[alt='5250 screen']");
    if (!img || !window.WebSocket) { return; }
    canvas = document.createElement("canvas");
    canvas.width = img.width; canvas.height = img.height;
    canvas.setAttribute("tabindex", "0");
    canvas.style.display = "block"; canvas.style.outline = "none";
    ctx = canvas.getContext("2d");
    var proto = location.protocol === "https:" ? "wss:" : "ws:";
    sock = new WebSocket(proto + "//" + location.host + base + "/5250/ws/" + id);
    sock.onopen = function () { img.parentNode.replaceChild(canvas, img); canvas.focus(); };
    sock.onmessage = function (ev) { var m = JSON.parse(ev.data); if (m.t === "screen" || m.t === "delta") { apply(m); } };
    sock.onclose = function () { if (screen) { screen.state = "disconnected"; draw(); } };
    canvas.addEventListener("keydown", onKey);
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
    var x = new XMLHttpRequest();
    x.open("GET", base + "/5250/" + id + "/keymap/json");
    x.onload = function () { try { keymap = JSON.parse(x.responseText); } catch (err) { keymap = {}; } };
    x.send();
    // The keypad and Copy and Paste go over the socket while it is open;
    // the menu bar and Scale switch in place.
    document.addEventListener("click", function (e) {
      var t = e.target;
      var a = t && t.closest ? t.closest("a[href]") : null;
      if (a && sock && sock.readyState === 1 && !e.ctrlKey && !e.shiftKey && !e.metaKey && e.button === 0 && inPlace(a)) { e.preventDefault(); return; }
      if (!t || t.tagName !== "INPUT" || t.type !== "submit") { return; }
      if (t.name === "k" && send({ t: "key", k: KEYPAD[t.value] || t.value })) { e.preventDefault(); canvas.focus(); }
      if (t.name === "tb" && t.value === "Copy" && navigator.clipboard && sel) { e.preventDefault(); navigator.clipboard.writeText(selectedText()); }
      if (t.name === "tb" && t.value === "Paste" && navigator.clipboard && navigator.clipboard.readText) {
        e.preventDefault(); navigator.clipboard.readText().then(function (s) { send({ t: "type", s: s.replace(/[\r\n]/g, "") }); });
      }
    });
  }
  bw = me.getAttribute("data-scheme") === "bw";
  if (document.readyState === "loading") { document.addEventListener("DOMContentLoaded", start); } else { start(); }
})();
