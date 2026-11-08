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
    if (how === "push") { history.pushState({ qsnav: 1 }, "", url); }
    else if (how === "replace") { history.replaceState({ qsnav: 1 }, "", url); }
    var at = hash && document.getElementById(hash.slice(1));
    if (at) { at.scrollIntoView(); } else if (how !== "replace") { window.scrollTo(0, 0); }
    refresh(doc);
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
})();
