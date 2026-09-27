const NATIVE_HOST = "com.zerotremor.host";

chrome.action.onClicked.addListener(() => {
  chrome.tabs.create({
    url: chrome.runtime.getURL("calibration.html")
  });
});

// Extension message type -> native host message type
const NATIVE_MESSAGE_TYPES = {
  ZEROTREMOR_NATIVE_PING: "ping",
  ZEROTREMOR_GET_PROFILE: "get_profile",
};

chrome.runtime.onMessage.addListener(
  (message, sender, sendResponse) => {
    const nativeType = NATIVE_MESSAGE_TYPES[message?.type];

    if (!nativeType) {
      return;
    }

    chrome.runtime.sendNativeMessage(
      NATIVE_HOST,
      { type: nativeType },
      (response) => {
        if (chrome.runtime.lastError) {
          sendResponse({
            ok: false,
            error: chrome.runtime.lastError.message,
          });

          return;
        }

        if (response?.type === "error") {
          sendResponse({
            ok: false,
            error: response.message,
          });

          return;
        }

        sendResponse({
          ok: true,
          response,
        });
      }
    );

    // Keep sendResponse alive while native messaging completes.
    return true;
  }
);
