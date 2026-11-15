const button = document.querySelector("[data-copy]");
if (button) {
  button.addEventListener("click", async () => {
    const status = document.querySelector(".copy-status");
    status.textContent = "";
    try {
      await navigator.clipboard.writeText(document.querySelector("#install-command").textContent);
      status.textContent = "Command copied. Paste it into your terminal to install.";
    } catch {
      status.textContent = "Copy is unavailable. Select the command and copy it manually.";
    }
  });
}
