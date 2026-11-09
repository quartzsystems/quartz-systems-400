// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

// In-place navigation for a product that turns it on (Product::nav_script).
// Links and forms within the product fetch the next page and swap the
// body in, so the frame never blanks. Every page still works without it:
// a page with a script of its own (the 5250 display), anything that is not
// HTML, and any failure fall back to the browser's own navigation.
(function () {
  "use strict";
  if (!window.fetch || !window.history || !history.pushState || !window.DOMParser) { return; }
  var me = document.currentScript;
  var mine = me ? new URL(me.src, location.href).pathname : "";
  var prefix = location.pathname.split("/").slice(0, 3).join("/") + "/";
  var timer = null, busy = 0;
  // A page loaded with its own refresh keeps the browser's: navigating
  // from it is a real load, or the refresh would fire on the next page.
  var held = !!document.querySelector("meta[http-equiv='refresh' i]");

  // Whether a document has scripts besides this one.
  function scripted(doc) {
    var s = doc.querySelectorAll("script");
    for (var i = 0; i < s.length; i++) {
      var src = s[i].getAttribute("src");
      if (!src || new URL(src, location.href).pathname !== mine) { return true; }
    }
    return false;
  }
  function ours(u) { return u.origin === location.origin && u.pathname.indexOf(prefix) === 0; }

  // The page's <meta http-equiv="refresh">, kept as a timer.
  function refresh(doc) {
    if (timer) { clearTimeout(timer); timer = null; }
    var m = doc.querySelector("meta[http-equiv='refresh' i]");
    var n = m ? parseInt(m.getAttribute("content"), 10) : NaN;
    if (n > 0) { timer = setTimeout(function () { load(location.href, null, "replace"); }, n * 1000); }
  }

  // A download: save the body under the name the server gave.
  function save(r) {
    return r.blob().then(function (b) {
      var cd = r.headers.get("content-disposition") || "", m = /filename\*?=(?:UTF-8'')?"?([^";]+)"?/i.exec(cd);
      var a = document.createElement("a");
      a.href = URL.createObjectURL(b);
      a.download = m ? decodeURIComponent(m[1]) : "";
      document.body.appendChild(a); a.click(); a.remove();
      setTimeout(function () { URL.revokeObjectURL(a.href); }, 60000);
    });
  }

  function swap(doc, url, how, hash) {
    document.title = doc.title;
    document.body.innerHTML = doc.body.innerHTML;
    // Landing on the same page (a tree toggle, a posted form) adds no entry.
    if (how === "push" && url === location.href) { how = "replace"; }
    if (how === "push") { history.pushState({ qsnav: 1 }, "", url); }
    else if (how === "replace") { history.replaceState({ qsnav: 1 }, "", url); }
    var at = hash && document.getElementById(hash.slice(1));
    if (at) { at.scrollIntoView(); } else if (how !== "replace") { window.scrollTo(0, 0); }
    refresh(doc);
    lists();
    var f = document.querySelector("[autofocus]");
    if (f) { f.focus(); }
  }

  // Fetch `url` (with `init` for a form) and show it; `how` is push,
  // replace or none (history already moved).
  function load(url, init, how) {
    var u = new URL(url, location.href), seq = ++busy;
    document.documentElement.style.cursor = "progress";
    var opts = init || {};
    opts.credentials = "same-origin";
    return fetch(u.href, opts).then(function (r) {
      var type = r.headers.get("content-type") || "";
      if (seq !== busy) { return; }
      if (type.indexOf("text/html") < 0) {
        if (r.ok && /attachment/i.test(r.headers.get("content-disposition") || "")) { return save(r); }
        location.href = r.url;
        return;
      }
      return r.text().then(function (text) {
        if (seq !== busy) { return; }
        var doc = new DOMParser().parseFromString(text, "text/html");
        var final = new URL(r.url || u.href);
        final.hash = u.hash;
        if (scripted(doc) || !ours(final)) {
          // The page needs a real load: a GET result loads again, a
          // posted answer is written whole.
          if (!init || r.redirected) { location.href = final.href; return; }
          document.open(); document.write(text); document.close();
          history.replaceState(null, "", final.href);
          return;
        }
        swap(doc, final.href, how, u.hash);
      });
    }).catch(function () {
      if (!init) { location.href = u.href; } else { location.reload(); }
    }).then(function () {
      if (seq === busy) { document.documentElement.style.cursor = ""; }
    });
  }

  // Every list on the page: a column's header sorts the rows shown A-Z,
  // again Z-A, a third time back to the server's order; the row under
  // the headers filters them, a row staying when each of its cells holds
  // that column's text. The page's own sort links work without script.
  var collate = window.Intl && Intl.Collator
    ? new Intl.Collator(undefined, { numeric: true, sensitivity: "base" }).compare
    : function (a, b) { return a < b ? -1 : (a > b ? 1 : 0); };
  function text(c) {
    var i = c.querySelector("input[type=text], select");
    return (i ? i.value : c.textContent).replace(/\s+/g, " ").trim();
  }
  function lists() {
    var ts = document.querySelectorAll("table.list");
    for (var i = 0; i < ts.length; i++) { list(ts[i]); }
  }
  function list(t) {
    if (t.getAttribute("data-qslist")) { return; }
    var rows = t.rows, head = null, i, c;
    for (i = 0; i < rows.length && !head; i++) { if (rows[i].querySelector("th")) { head = rows[i]; } }
    if (!head) { return; }
    var n = head.cells.length, data = [];
    for (i = 0; i < n; i++) { if (head.cells[i].colSpan > 1) { return; } }
    for (i = head.rowIndex + 1; i < rows.length; i++) {
      if (rows[i].cells.length === n && !rows[i].querySelector("th") && rows[i].parentNode === head.parentNode) { data.push(rows[i]); }
    }
    if (data.length < 2) { return; }
    t.setAttribute("data-qslist", "1");
    var body = head.parentNode, tail = data[data.length - 1].nextSibling;
    var by = -1, dir = 0, cols = [], filters = [], marks = [];
    function shown() {
      var order = data.slice();
      if (dir) {
        order.sort(function (a, b) {
          return dir * collate(text(a.cells[by]), text(b.cells[by])) || data.indexOf(a) - data.indexOf(b);
        });
      }
      var k = 0;
      for (var r = 0; r < order.length; r++) {
        var row = order[r], keep = true;
        for (var f = 0; f < n && keep; f++) {
          var v = filters[f] && filters[f].value.trim().toLowerCase();
          if (v && text(row.cells[f]).toLowerCase().indexOf(v) < 0) { keep = false; }
        }
        row.style.display = keep ? "" : "none";
        if (keep) { row.classList.toggle("alt", k % 2 === 1); k++; }
        body.insertBefore(row, tail);
      }
      for (var m = 0; m < marks.length; m++) {
        if (marks[m]) { marks[m].textContent = m === by && dir ? (dir > 0 ? " ▲" : " ▼") : ""; }
      }
    }
    var fr = document.createElement("tr");
    fr.className = "qsfilter";
    for (c = 0; c < n; c++) {
      var h = head.cells[c], label = h.textContent.trim();
      cols.push(!!label && !/^(actions|select|opt)$/i.test(label));
      var td = document.createElement("td");
      if (cols[c]) {
        var inp = document.createElement("input");
        inp.type = "search";
        inp.placeholder = "Filter";
        inp.setAttribute("aria-label", "Filter " + label);
        inp.addEventListener("input", shown);
        inp.addEventListener("keydown", function (e) { if (e.key === "Enter") { e.preventDefault(); } });
        td.appendChild(inp);
        filters.push(inp);
        var mark = document.createElement("span");
        h.appendChild(mark);
        marks.push(mark);
        h.classList.add("qssort");
        h.title = "Sort A-Z, then Z-A";
        h.addEventListener("click", (function (col) {
          return function (e) {
            if (e.button !== 0 || e.ctrlKey || e.shiftKey || e.metaKey) { return; }
            e.preventDefault();
            e.stopPropagation();
            if (by === col) { dir = dir === 1 ? -1 : (dir === -1 ? 0 : 1); } else { by = col; dir = 1; }
            shown();
          };
        })(c));
      } else {
        filters.push(null);
        marks.push(null);
      }
      fr.appendChild(td);
    }
    body.insertBefore(fr, head.nextSibling);
  }

  // Text that follows a form's choices: an element with data-by (the
  // fields' names, "t,d") and data-v (JSON, the values joined by "|"
  // to the text) shows the text of the choices made, e.g. a
  // confirmation's sentence and equivalent command.
  function follow(f) {
    var els = document.querySelectorAll("[data-by][data-v]");
    for (var i = 0; i < els.length; i++) {
      var map;
      try { map = JSON.parse(els[i].getAttribute("data-v")); } catch (err) { continue; }
      var key = els[i].getAttribute("data-by").split(",").map(function (n) {
        var x = f.elements[n.trim()];
        return x ? x.value : "";
      }).join("|");
      if (Object.prototype.hasOwnProperty.call(map, key)) { els[i].textContent = map[key]; }
    }
  }
  window.addEventListener("change", function (e) {
    var f = e.target && e.target.form;
    if (f) { follow(f); }
  });

  function plain(e) { return e.button === 0 && !e.ctrlKey && !e.shiftKey && !e.metaKey && !e.altKey; }

  window.addEventListener("click", function (e) {
    if (held || e.defaultPrevented || !plain(e) || scripted(document)) { return; }
    var a = e.target && e.target.closest ? e.target.closest("a[href]") : null;
    if (!a || a.target || a.hasAttribute("download")) { return; }
    var u = new URL(a.href, location.href);
    if (!ours(u)) { return; }
    if (u.pathname === location.pathname && u.search === location.search && u.hash) { return; }
    e.preventDefault();
    load(u.href, null, "push");
  });

  window.addEventListener("submit", function (e) {
    if (held || e.defaultPrevented || scripted(document)) { return; }
    var f = e.target, by = e.submitter;
    if (!f || f.target || (by && by.formTarget)) { return; }
    var action = new URL((by && by.getAttribute("formaction")) || f.getAttribute("action") || location.href, location.href);
    if (!ours(action)) { return; }
    var method = ((by && by.getAttribute("formmethod")) || f.getAttribute("method") || "get").toLowerCase();
    var data;
    try { data = by ? new FormData(f, by) : new FormData(f); } catch (err) { return; }
    e.preventDefault();
    if (method === "get") {
      action.search = new URLSearchParams(data).toString();
      load(action.href, null, "push");
    } else {
      // Multipart only when a file is sent; otherwise the urlencoded
      // form the server expects of a plain browser.
      var multi = (f.getAttribute("enctype") || "").toLowerCase() === "multipart/form-data";
      load(action.href, { method: "POST", body: multi ? data : new URLSearchParams(data) }, "push");
    }
  });

  window.addEventListener("popstate", function () {
    if (!held && !scripted(document)) { load(location.href, null, "none"); } else { location.reload(); }
  });

  history.replaceState({ qsnav: 1 }, "", location.href);
  lists();
})();
