(() => {
  const player = document.querySelector("#global-player");
  const audio = document.querySelector("#global-audio");
  const title = document.querySelector("#player-title");
  const artists = document.querySelector("#player-artists");
  const cover = document.querySelector("#player-cover");
  const coverPlaceholder = document.querySelector("#player-cover-placeholder");
  const close = document.querySelector("#player-close");

  if (!player || !audio || !title || !artists || !cover || !coverPlaceholder || !close) {
    return;
  }

  document.addEventListener("click", (event) => {
    const button = event.target.closest("[data-preview-url]");
    if (!button) return;

    const url = button.dataset.previewUrl;
    if (!url) return;

    const nextUrl = new URL(url, window.location.href).href;
    if (audio.src !== nextUrl) {
      audio.src = nextUrl;
      title.textContent = button.dataset.previewTitle || "未知歌曲";
      artists.textContent = button.dataset.previewArtists || "";

      const coverUrl = button.dataset.previewCover;
      if (coverUrl) {
        cover.src = coverUrl;
        cover.classList.remove("hidden");
        coverPlaceholder.classList.add("hidden");
      } else {
        cover.removeAttribute("src");
        cover.classList.add("hidden");
        coverPlaceholder.classList.remove("hidden");
      }
    }

    player.classList.remove("hidden");
    audio.play().catch(() => {});
  });

  close.addEventListener("click", () => {
    audio.pause();
    audio.removeAttribute("src");
    audio.load();
    player.classList.add("hidden");
  });
})();
