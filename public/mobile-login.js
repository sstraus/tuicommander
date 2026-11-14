// TUICommander mobile login form. Posts the credentials as JSON to /auth/login;
// the server answers with the session cookie, then the page returns to the app.
(() => {
  const form = document.getElementById("login");
  const error = document.getElementById("error");
  const submit = document.getElementById("submit");

  const messages = {
    401: "Wrong username or password.",
    403: "Login refused: open this page from the TUICommander address.",
    429: "Too many attempts. Try again later, or sign in now with the QR code in TUICommander Settings → Remote Access.",
  };

  async function lockoutMessage(response) {
    try {
      const body = await response.json();
      return typeof body.error === "string" && body.error ? body.error : null;
    } catch {
      return null;
    }
  }

  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    error.textContent = "";
    submit.disabled = true;
    try {
      const next = new URLSearchParams(location.search).get("next");
      const response = await fetch("/auth/login", {
        method: "POST",
        credentials: "same-origin",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          username: form.elements.username.value,
          password: form.elements.password.value,
          next,
        }),
      });
      if (response.ok) {
        const body = await response.json();
        // Redirects inherit the original fragment; keep it after the form login.
        location.replace((body.next || "/mobile") + (location.hash || ""));
        return;
      }
      // A lockout refuses even the correct password; the server names the wait
      // and the QR recovery, so show its text when it sent one.
      const lockout = response.status === 429 ? await lockoutMessage(response) : null;
      error.textContent = lockout || messages[response.status] || `Login failed (${response.status}).`;
    } catch {
      error.textContent = "Server unreachable. Try again.";
    }
    submit.disabled = false;
  });
})();
