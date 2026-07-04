// Extension contract e2e: feed a realistic chrome downloadItem through the
// extension's OWN buildAddDownload, send it to the live daemon over a real
// WebSocket (Node 22 global WebSocket), and confirm the file downloads.
//
// Usage: node extension/test/e2e.mjs <savePath>
import { buildAddDownload, ServerMessageType } from "../chrome/lib/udm-core.js";

const savePath = process.argv[2];
if (!savePath) {
  console.error("usage: node e2e.mjs <savePath>");
  process.exit(2);
}

// Simulate what chrome.downloads.onCreated would give us for a clicked link,
// plus cookies from chrome.cookies.getAll().
const downloadItem = {
  url: "http://127.0.0.1:8799/ranged",
  finalUrl: "http://127.0.0.1:8799/ranged",
  filename: savePath, // chrome gives a full path; basename() extracts the name
  referrer: "http://127.0.0.1:8799/page",
};
const cookies = [{ name: "session", value: "abc123" }];

const msg = buildAddDownload(downloadItem, cookies, "chrome");
// Override savePath so the daemon writes where the test expects.
msg.payload.savePath = savePath;

const ws = new WebSocket("ws://127.0.0.1:60123");
let added = false;
let progress = 0;
const timeout = setTimeout(() => fail("timeout"), 30000);

function done(ok, why) {
  clearTimeout(timeout);
  try { ws.close(); } catch {}
  if (ok) { console.log("E2E_OK"); process.exit(0); }
  else { console.error("E2E_FAIL: " + why); process.exit(1); }
}
function fail(why) { done(false, why); }

ws.addEventListener("open", () => {
  console.log("sending:", JSON.stringify(msg));
  ws.send(JSON.stringify(msg));
});
ws.addEventListener("message", (ev) => {
  let m;
  try { m = JSON.parse(ev.data); } catch { return; }
  if (m.type === ServerMessageType.JOB_ADDED) {
    added = true;
    // Verify the daemon parsed the extension's payload correctly.
    if (m.job.referrer !== "http://127.0.0.1:8799/page") return fail("referrer not forwarded");
    if (m.job.cookies !== "session=abc123") return fail("cookies not forwarded");
    if (m.job.filename !== "ranged") return fail("filename basename wrong: " + m.job.filename);
    console.log("JOB_ADDED ok (referrer + cookies + filename forwarded)");
  } else if (m.type === ServerMessageType.JOB_PROGRESS) {
    progress++;
  } else if (m.type === ServerMessageType.JOB_COMPLETED) {
    if (!added) return fail("completed without JOB_ADDED");
    console.log(`JOB_COMPLETED after ${progress} progress events`);
    done(true);
  } else if (m.type === ServerMessageType.JOB_FAILED) {
    fail("job failed: " + m.error);
  }
});
ws.addEventListener("error", () => fail("websocket error (is the daemon running?)"));
