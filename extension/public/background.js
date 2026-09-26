const DEFAULT_SETTINGS = {
  companionUrl: "http://127.0.0.1:8765",
  companionToken: "",
};
const QUEUE_KEY = "steadyuiPendingRecords";
const MAX_QUEUE_SIZE = 100;

async function settings() {
  return chrome.storage.local.get(DEFAULT_SETTINGS);
}

async function sendToCompanion(kind, payload) {
  const { companionUrl, companionToken } = await settings();
  if (!companionToken) throw new Error("Configure the local companion token in extension options.");
  const paths = { calibration: "/calibration-trials", interaction: "/interactions", predict: "/predict" };
  const response = await fetch(`${companionUrl}${paths[kind]}`, {
    method: "POST",
    headers: { "Content-Type": "application/json", "X-SteadyUI-Token": companionToken },
    body: JSON.stringify(payload),
  });
  const result = await response.json();
  if (!response.ok) throw new Error(result.error || "The local companion rejected the request.");
  return result;
}

async function queue(kind, payload) {
  const stored = await chrome.storage.local.get({ [QUEUE_KEY]: [] });
  await chrome.storage.local.set({
    [QUEUE_KEY]: [...stored[QUEUE_KEY], { kind, payload }].slice(-MAX_QUEUE_SIZE),
  });
}

async function flushQueue() {
  const stored = await chrome.storage.local.get({ [QUEUE_KEY]: [] });
  const remaining = [];
  for (const item of stored[QUEUE_KEY]) {
    try { await sendToCompanion(item.kind, item.payload); } catch { remaining.push(item); }
  }
  await chrome.storage.local.set({ [QUEUE_KEY]: remaining });
}

chrome.runtime.onStartup.addListener(() => void flushQueue());
chrome.runtime.onInstalled.addListener(() => void flushQueue());

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (!message || !["calibration", "interaction", "predict"].includes(message.type)) return false;
  sendToCompanion(message.type, message.payload)
    .then((result) => sendResponse({ ok: true, result }))
    .catch(async (error) => {
      if (message.type !== "predict") await queue(message.type, message.payload);
      sendResponse({ ok: false, queued: message.type !== "predict", error: error.message });
    });
  return true;
});
