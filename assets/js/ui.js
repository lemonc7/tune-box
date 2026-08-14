(() => {
  const openDropdowns = () =>
    document.querySelectorAll("details[data-dropdown][open]");

  document.addEventListener("click", (event) => {
    const current = event.target.closest("details[data-dropdown]");

    for (const dropdown of openDropdowns()) {
      if (dropdown !== current) {
        dropdown.removeAttribute("open");
      }
    }
  });

  document.addEventListener("keydown", (event) => {
    if (event.key !== "Escape") return;

    for (const dropdown of openDropdowns()) {
      dropdown.removeAttribute("open");
      dropdown.querySelector("summary")?.focus();
    }
  });
})();
