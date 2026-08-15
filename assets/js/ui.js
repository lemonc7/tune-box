(() => {
  const content = document.querySelector("#app-content");
  const appError = document.querySelector("#app-error");
  const pageCache = new Map();
  const pagePaths = new Set(["/", "/downloads", "/library"]);
  const cacheablePaths = new Set(["/"]);

  const clearError = () => {
    if (appError) appError.replaceChildren();
  };

  const pagePath = (value = window.location.href) => {
    const path = new URL(value, window.location.href).pathname.replace(/\/+$/, "");
    return path || "/";
  };

  let currentPath = pagePath();

  const snapshotFormState = (source, clone) => {
    const sourceFields = source.querySelectorAll("input, textarea, select");
    const cloneFields = clone.querySelectorAll("input, textarea, select");

    sourceFields.forEach((field, index) => {
      const copy = cloneFields[index];
      if (!copy) return;

      if (field instanceof HTMLInputElement) {
        if (field.type !== "file") {
          copy.value = field.value;
          copy.setAttribute("value", field.value);
        }
        copy.toggleAttribute("checked", field.checked);
      } else if (field instanceof HTMLTextAreaElement) {
        copy.value = field.value;
        copy.textContent = field.value;
      } else if (field instanceof HTMLSelectElement) {
        copy.value = field.value;
        Array.from(copy.options).forEach((option) => {
          option.toggleAttribute("selected", option.value === field.value);
        });
      }
    });
  };

  const savePage = (path = currentPath) => {
    if (!content || !cacheablePaths.has(path)) return;

    const clone = content.cloneNode(true);
    snapshotFormState(content, clone);
    pageCache.set(path, {
      html: clone.innerHTML,
      title: document.title,
      scrollY: window.scrollY,
    });
  };

  const restorePage = (path, entry) => {
    clearError();
    content.innerHTML = entry.html;
    document.title = entry.title;
    currentPath = path;
    window.htmx?.process(content);
    requestAnimationFrame(() => window.scrollTo(0, entry.scrollY));
  };

  if (content && pagePaths.has(currentPath)) {
    history.replaceState({ tunePageNavigation: true, path: currentPath }, "", location.href);

    document.addEventListener(
      "click",
      (event) => {
        const link = event.target.closest("a[data-page-link]");
        if (
          !link ||
          event.defaultPrevented ||
          event.button !== 0 ||
          event.metaKey ||
          event.ctrlKey ||
          event.shiftKey ||
          event.altKey
        ) {
          return;
        }

        const destination = pagePath(link.href);
        if (!pagePaths.has(destination)) return;

        clearError();

        if (destination === currentPath && cacheablePaths.has(destination)) {
          event.preventDefault();
          event.stopImmediatePropagation();
          return;
        }

        savePage();
        const cached = pageCache.get(destination);
        if (!cached) return;

        event.preventDefault();
        event.stopImmediatePropagation();
        restorePage(destination, cached);
        history.pushState({ tunePageNavigation: true, path: destination }, "", link.href);
      },
      true,
    );

    window.addEventListener(
      "popstate",
      (event) => {
        const destination = event.state?.tunePageNavigation && pagePath(event.state.path);
        const cached = destination && pageCache.get(destination);
        if (!destination || destination === currentPath) return;

        event.stopImmediatePropagation();
        savePage();
        if (cached) {
          restorePage(destination, cached);
        } else {
          window.location.reload();
        }
      },
      true,
    );

    document.addEventListener("htmx:afterSettle", (event) => {
      const request = event.detail?.requestConfig;
      if (!request?.boosted || event.detail?.target !== content) return;

      currentPath = pagePath();
      if (pagePaths.has(currentPath)) {
        history.replaceState(
          { tunePageNavigation: true, path: currentPath },
          "",
          location.href,
        );
      }
    });
  }

  document.addEventListener("htmx:afterRequest", (event) => {
    if (event.detail?.successful && event.detail?.target !== appError) {
      clearError();
    }
  });

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
