// Send to NyxProxy — Manifest V3 service worker.
//
// Adds two right-click context menu items:
//   * "Send page to NyxProxy"  → POSTs the current tab URL to NyxProxy's
//                                  /api/v1/import-url endpoint.
//   * "Send link to NyxProxy"  → POSTs the right-clicked link target.
//
// Defaults to http://127.0.0.1:8090 — configurable on the options page.

const DEFAULT_BRIDGE = "http://127.0.0.1:8090";

async function getBridgeBase() {
  return new Promise((resolve) => {
    chrome.storage.sync.get(["bridgeBase"], (res) => {
      const value = (res && res.bridgeBase) ? res.bridgeBase : DEFAULT_BRIDGE;
      resolve(String(value).replace(/\/+$/, ""));
    });
  });
}

async function importUrl(url) {
  const base = await getBridgeBase();
  const endpoint = `${base}/api/v1/import-url`;
  const resp = await fetch(endpoint, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ url, method: "GET", tags: ["source:browser-ext"] }),
  });
  if (!resp.ok) {
    throw new Error(`NyxProxy responded ${resp.status}`);
  }
  return await resp.json();
}

function notify(title, message) {
  try {
    chrome.notifications.create({
      type: "basic",
      iconUrl: "icons/icon128.png",
      title,
      message,
    });
  } catch (_err) {
    // Notifications permission can be missing on some browsers; ignore.
  }
}

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: "nyxproxy-send-page",
    title: "Send page to NyxProxy",
    contexts: ["page"],
  });
  chrome.contextMenus.create({
    id: "nyxproxy-send-link",
    title: "Send link to NyxProxy",
    contexts: ["link"],
  });
});

chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  let target = null;
  if (info.menuItemId === "nyxproxy-send-link" && info.linkUrl) {
    target = info.linkUrl;
  } else if (info.menuItemId === "nyxproxy-send-page" && tab && tab.url) {
    target = tab.url;
  }
  if (!target) return;
  try {
    const result = await importUrl(target);
    const flowId = result && result.data && result.data.flow_id ? result.data.flow_id : "?";
    notify("Sent to NyxProxy", `${target}\nflow_id=${flowId}`);
  } catch (err) {
    notify("NyxProxy import failed", `${err}\nIs NyxProxy running at ${await getBridgeBase()}?`);
  }
});

chrome.action.onClicked.addListener(async (tab) => {
  if (!tab || !tab.url) return;
  try {
    const result = await importUrl(tab.url);
    const flowId = result && result.data && result.data.flow_id ? result.data.flow_id : "?";
    notify("Sent to NyxProxy", `${tab.url}\nflow_id=${flowId}`);
  } catch (err) {
    notify("NyxProxy import failed", `${err}\nIs NyxProxy running at ${await getBridgeBase()}?`);
  }
});
