/* SPDX-License-Identifier: GPL-3.0-only
 * Copyright (C) 2026 Vaibhav
 *
 * ~4 KB of vanilla JS: live search, inline pin/delete, and instant add.
 * Everything is progressive enhancement — the app works with JS disabled.
 */
(function () {
  "use strict";

  var csrfMeta = document.querySelector('meta[name="csrf-token"]');
  var CSRF = csrfMeta ? csrfMeta.getAttribute("content") : "";

  function hdrs(json) {
    var h = { "X-CSRF-Token": CSRF };
    if (json) h["Content-Type"] = "application/json";
    return h;
  }

  // ---- live search -------------------------------------------------------
  var input = document.getElementById("q");
  var wrap = document.getElementById("results-wrap");
  var timer = null;
  var inflight = null;

  function liveSearch() {
    if (!input || !wrap) return;
    var q = input.value;
    if (inflight) inflight.abort();
    var ctrl = new AbortController();
    inflight = ctrl;
    var url = "/?partial=1&q=" + encodeURIComponent(q);
    fetch(url, { signal: ctrl.signal, headers: { "X-Requested-With": "fetch" } })
      .then(function (r) { return r.ok ? r.text() : Promise.reject(r.status); })
      .then(function (html) {
        wrap.innerHTML = html;
        wrap.setAttribute("data-query", q);
        var u = q ? "/?q=" + encodeURIComponent(q) : "/";
        history.replaceState(null, "", u);
        bindActions();
      })
      .catch(function () { /* aborted or offline: ignore */ })
      .finally(function () { inflight = null; });
  }

  if (input) {
    input.addEventListener("input", function () {
      clearTimeout(timer);
      timer = setTimeout(liveSearch, 180);
    });
    // "/" focuses search from anywhere.
    document.addEventListener("keydown", function (e) {
      if (e.key === "/" && document.activeElement !== input) {
        e.preventDefault();
        input.focus();
      }
    });
  }

  // ---- inline actions ----------------------------------------------------
  function refresh() {
    if (input) liveSearch();
    else window.location.reload();
  }

  function bindActions() {
    document.querySelectorAll("[data-pin]").forEach(function (btn) {
      if (btn.dataset.bound) return;
      btn.dataset.bound = "1";
      btn.addEventListener("click", function (e) {
        e.preventDefault();
        var id = btn.getAttribute("data-pin");
        var next = btn.getAttribute("data-pinned") !== "true";
        fetch("/bookmarks/" + id + "/pin", {
          method: "POST",
          headers: hdrs(true),
          body: JSON.stringify({ pinned: next }),
        }).then(function (r) {
          if (r.ok) {
            btn.setAttribute("data-pinned", String(next));
            btn.textContent = next ? "Unpin" : "Pin";
            refresh();
          }
        });
      });
    });

    document.querySelectorAll("[data-delete]").forEach(function (btn) {
      if (btn.dataset.bound) return;
      btn.dataset.bound = "1";
      btn.addEventListener("click", function (e) {
        e.preventDefault();
        var id = btn.getAttribute("data-delete");
        if (!confirm("Delete this bookmark? This cannot be undone.")) return;
        fetch("/bookmarks/" + id, { method: "DELETE", headers: hdrs(false) }).then(function (r) {
          if (r.ok) {
            var card = btn.closest(".card");
            if (card) card.remove();
            if (location.pathname.indexOf("/b/") === 0) location.href = "/";
            else refresh();
          }
        });
      });
    });
  }

  // ---- add form ----------------------------------------------------------
  var addForm = document.getElementById("add-form");
  var addStatus = document.getElementById("add-status");
  if (addForm) {
    addForm.addEventListener("submit", function (e) {
      e.preventDefault();
      var target = addForm.querySelector('[name="target"]');
      var tags = addForm.querySelector('[name="tags"]');
      if (!target || !target.value.trim()) return;
      var body = new URLSearchParams();
      body.set("target", target.value.trim());
      if (tags) body.set("tags", tags.value.trim());
      if (addStatus) addStatus.textContent = "Saving…";
      fetch("/bookmarks", {
        method: "POST",
        headers: { "X-CSRF-Token": CSRF, "Content-Type": "application/x-www-form-urlencoded" },
        body: body.toString(),
      })
        .then(function (r) {
          if (!r.ok) throw new Error("save failed");
          return r.json().catch(function () { return {}; });
        })
        .then(function () {
          if (target) target.value = "";
          if (tags) tags.value = "";
          if (addStatus) addStatus.textContent = "Saved.";
          setTimeout(function () { if (addStatus) addStatus.textContent = ""; }, 1600);
          refresh();
        })
        .catch(function () {
          if (addStatus) addStatus.textContent = "Could not save — check the URL and try again.";
        });
    });
  }

  bindActions();
})();
