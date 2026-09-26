const defaults = { companionUrl: "http://127.0.0.1:8765", companionToken: "" };

async function load() {
  const saved = await chrome.storage.local.get(defaults);
  document.querySelector("#url").value = saved.companionUrl;
  document.querySelector("#token").value = saved.companionToken;
}

document.querySelector("#save").addEventListener("click", async () => {
  const companionUrl = document.querySelector("#url").value.replace(/\/$/, "");
  const companionToken = document.querySelector("#token").value.trim();
  await chrome.storage.local.set({ companionUrl, companionToken });
  document.querySelector("#saved").textContent = " Saved.";
});

void load();
