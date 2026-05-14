const DEFAULT_BRIDGE = "http://127.0.0.1:8090";

const input = document.getElementById("bridgeBase");
const status = document.getElementById("status");

chrome.storage.sync.get(["bridgeBase"], (res) => {
  input.value = (res && res.bridgeBase) ? res.bridgeBase : DEFAULT_BRIDGE;
});

document.getElementById("save").addEventListener("click", () => {
  const value = (input.value || DEFAULT_BRIDGE).replace(/\/+$/, "");
  chrome.storage.sync.set({ bridgeBase: value }, () => {
    status.textContent = `Saved: ${value}`;
  });
});

document.getElementById("test").addEventListener("click", async () => {
  const base = (input.value || DEFAULT_BRIDGE).replace(/\/+$/, "");
  status.textContent = `Pinging ${base}/api/v1/ping …`;
  try {
    const resp = await fetch(`${base}/api/v1/ping`);
    const body = await resp.json();
    if (body && body.ok) {
      status.textContent = `OK — NyxProxy ${body.data && body.data.version ? body.data.version : "?"} is responding at ${base}`;
    } else {
      status.textContent = `Bridge responded but with ok=false: ${JSON.stringify(body)}`;
    }
  } catch (err) {
    status.textContent = `Could not reach ${base}: ${err}`;
  }
});
