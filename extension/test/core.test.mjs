// Unit tests for the extension core logic. Run: node --test extension/test
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  shouldIntercept,
  formatCookies,
  basename,
  buildAddDownload,
  summarize,
} from "../chrome/lib/udm-core.js";

test("shouldIntercept accepts http(s)/ftp, rejects others", () => {
  assert.equal(shouldIntercept({ url: "https://x.com/a.zip" }), true);
  assert.equal(shouldIntercept({ url: "http://x.com/a.zip" }), true);
  assert.equal(shouldIntercept({ url: "ftp://x.com/a.zip" }), true);
  assert.equal(shouldIntercept({ url: "blob:https://x.com/123" }), false);
  assert.equal(shouldIntercept({ url: "data:text/plain,hi" }), false);
  assert.equal(shouldIntercept({ url: "file:///c:/x" }), false);
  assert.equal(shouldIntercept({}), false);
});

test("shouldIntercept prefers finalUrl", () => {
  assert.equal(
    shouldIntercept({ url: "blob:x", finalUrl: "https://x.com/a" }),
    true
  );
});

test("formatCookies builds a Cookie header", () => {
  assert.equal(
    formatCookies([{ name: "a", value: "1" }, { name: "b", value: "2" }]),
    "a=1; b=2"
  );
  assert.equal(formatCookies([]), undefined);
  assert.equal(formatCookies(null), undefined);
});

test("basename handles / and \\", () => {
  assert.equal(basename("C:\\Users\\me\\file.zip"), "file.zip");
  assert.equal(basename("/home/me/file.zip"), "file.zip");
  assert.equal(basename("file.zip"), "file.zip");
});

test("buildAddDownload produces the daemon payload", () => {
  const item = {
    url: "https://x.com/a/b.zip",
    finalUrl: "https://cdn.x.com/a/b.zip",
    filename: "C:\\dl\\b.zip",
    referrer: "https://x.com/page",
  };
  const msg = buildAddDownload(item, [{ name: "s", value: "1" }], "chrome");
  assert.equal(msg.type, "ADD_DOWNLOAD");
  assert.deepEqual(msg.payload, {
    url: "https://cdn.x.com/a/b.zip", // finalUrl wins
    sourceBrowser: "chrome",
    filename: "b.zip",
    referrer: "https://x.com/page",
    cookies: "s=1",
  });
});

test("buildAddDownload omits optional fields when absent", () => {
  const msg = buildAddDownload({ url: "https://x.com/a" }, [], "chrome");
  assert.deepEqual(msg.payload, { url: "https://x.com/a", sourceBrowser: "chrome" });
});

test("summarize counts active jobs and sums speed", () => {
  const jobs = {
    a: { status: "active", speedBps: 1000 },
    b: { status: "active", speedBps: 500 },
    c: { status: "queued", speedBps: 0 },
    d: { status: "completed", speedBps: 0 },
  };
  assert.deepEqual(summarize(jobs), { activeCount: 2, totalSpeed: 1500, totalJobs: 4 });
});
