(function () {
  const SESSION_COOKIE = "saccade_session=demo";
  const STORAGE_KEY = "saccade_storage";
  const STORAGE_VALUE = "shared";

  function setSession() {
    document.cookie = `${SESSION_COOKIE}; path=/; SameSite=Lax`;
    localStorage.setItem(STORAGE_KEY, STORAGE_VALUE);
  }

  function hasSession() {
    return document.cookie.includes("saccade_session=demo");
  }

  const params = new URLSearchParams(location.search);
  if (location.pathname.endsWith("/login.html") && params.get("auto") === "1") {
    setSession();
    location.href = "dashboard.html";
    return;
  }

  const form = document.getElementById("login-form");
  if (form) {
    form.addEventListener("submit", (event) => {
      event.preventDefault();
      setSession();
      location.href = "dashboard.html";
    });
  }

  const sessionState = document.getElementById("session-state");
  if (sessionState) {
    if (hasSession()) {
      sessionState.textContent = "LOGGED_IN user=wayne";
    } else {
      sessionState.textContent = "LOGGED_OUT";
    }
  }

  const storageState = document.getElementById("storage-state");
  if (storageState) {
    storageState.textContent = `storage=${localStorage.getItem(STORAGE_KEY) || ""}`;
  }
})();
