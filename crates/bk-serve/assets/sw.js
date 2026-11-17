/* SPDX-License-Identifier: GPL-3.0-only
 * Copyright (C) 2026 Vaibhav
 *
 * Service worker: makes the app installable and handles Android's share sheet.
 * Instagram/Chrome/YouTube "Share → bookmark" POSTs here, and we forward the
 * payload to /api/v1/share, then bounce back into the app.
 */
const CACHE = "bookmark-v1";
const SHELL = ["/static/app.css", "/static/app.js", "/static/icon-192.png"];

self.addEventListener("install", (e) => {
  e.waitUntil(
    caches
      .open(CACHE)
      .then((c) => c.addAll(SHELL))
      .then(() => self.skipWaiting())
      .catch(() => self.skipWaiting())
  );
});

self.addEventListener("activate", (e) => {
  e.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim())
  );
});

async function handleShare(request) {
  let target = "";
  try {
    const form = await request.formData();
    target = String(form.get("url") || form.get("text") || form.get("title") || "").trim();
  } catch (_) {
    /* fall through */
  }
  if (!target) return Response.redirect("/?share=empty", 303);
  try {
    const body = new URLSearchParams({ target });
    const res = await fetch("/api/v1/share", {
      method: "POST",
      credentials: "include",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: body.toString(),
    });
    return Response.redirect(res.ok ? "/?share=saved" : "/?share=failed", 303);
  } catch (_) {
    return Response.redirect("/?share=failed", 303);
  }
}

self.addEventListener("fetch", (e) => {
  const url = new URL(e.request.url);
  if (e.request.method === "POST" && url.pathname === "/share-target") {
    e.respondWith(handleShare(e.request));
    return;
  }
  if (url.origin !== self.location.origin) return;

  if (url.pathname.startsWith("/static/")) {
    e.respondWith(caches.match(e.request).then((r) => r || fetch(e.request)));
    return;
  }
  if (e.request.mode === "navigate") {
    e.respondWith(fetch(e.request).catch(() => caches.match("/")));
  }
});
