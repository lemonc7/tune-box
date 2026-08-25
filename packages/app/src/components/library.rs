use contracts::library::Song;
use dioxus::{CapturedError, prelude::*};
use dioxus_icons::lucide::{FileMusic, LoaderCircle, Trash2};

use crate::{api, view};

#[component]
pub fn SongItem(song: Song, on_deleted: EventHandler<i64>) -> Element {
    let mut confirming_delete = use_signal(|| false);
    let song_id = song.id;
    let mut delete_song = use_action(move |_: ()| async move {
        api::delete_song(song_id)
            .await
            .map_err(|error| CapturedError::msg(view::server_error_message(error)))?;
        on_deleted.call(song_id);
        Ok::<_, CapturedError>(())
    });
    let artists = song
        .metadata
        .as_ref()
        .map(|metadata| metadata.artists.as_slice())
        .filter(|artists| !artists.is_empty())
        .unwrap_or(&song.artists);
    let artists = view::artists(artists);
    let cover_url = song
        .metadata
        .as_ref()
        .map(|metadata| metadata.cover_url.clone())
        .unwrap_or_default();
    let extension = if song.extension.is_empty() {
        "音频".to_owned()
    } else {
        song.extension.to_ascii_uppercase()
    };
    let details = song
        .metadata
        .as_ref()
        .into_iter()
        .flat_map(|metadata| {
            [metadata.album.clone(), metadata.date.clone()]
                .into_iter()
                .filter(|value| !value.is_empty())
        })
        .chain(std::iter::once(extension.clone()))
        .chain(std::iter::once(view::format_bytes(song.size)))
        .collect::<Vec<_>>()
        .join(" • ");
    let confirm_song_name = song.name.clone();
    let pending = delete_song.pending();

    rsx! {
      article { class: "py-5",
        div { class: "flex items-center gap-4",
          if cover_url.is_empty() {
            div {
              class: "flex size-14 shrink-0 items-center justify-center",
              class: "rounded-lg bg-muted text-muted-foreground",
              aria_hidden: true,
              FileMusic { size: 20 }
            }
          } else {
            img {
              class: "size-14 shrink-0 rounded-lg bg-muted object-cover",
              src: cover_url,
              alt: "",
              loading: "lazy",
            }
          }

          div { class: "min-w-0 flex-1",
            h2 { class: "truncate text-sm font-medium text-foreground",
              "{song.name}"
            }
            p { class: "mt-1 truncate text-sm text-muted-foreground",
              "{artists}"
            }
            p { class: "mt-1 text-xs text-muted-foreground",
              "{details}"
            }
          }

          div { class: "flex shrink-0 items-center gap-2",
            button {
              class: "inline-flex size-9 items-center justify-center rounded-md",
              class: "border border-destructive/30 text-destructive",
              class: "transition-colors hover:bg-destructive/10 focus-visible:outline-none",
              class: "focus-visible:ring-2 focus-visible:ring-destructive/40",
              class: "disabled:pointer-events-none disabled:opacity-50",
              r#type: "button",
              aria_label: "删除 {song.name}",
              disabled: pending,
              onclick: move |_| confirming_delete.set(true),
              span { class: "inline-flex", aria_hidden: true,
                if pending {
                  LoaderCircle { size: 16, class: "animate-spin" }
                } else {
                  Trash2 { size: 16 }
                }
              }
            }
          }
        }

        if let Some(Err(error)) = delete_song.value() {
          p {
            class: "mt-3 rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive",
            role: "alert",
            "{error}"
          }
        }

        if confirming_delete() {
          ConfirmDeleteSong {
            dialog_id: format!("delete-song-{song_id}"),
            song_name: confirm_song_name,
            pending,
            onconfirm: move |_| {
                confirming_delete.set(false);
                delete_song.call(());
            },
            oncancel: move |_| confirming_delete.set(false),
          }
        }
      }
    }
}

#[component]
fn ConfirmDeleteSong(
    dialog_id: String,
    song_name: String,
    pending: bool,
    onconfirm: EventHandler<()>,
    oncancel: EventHandler<()>,
) -> Element {
    let title_id = format!("{dialog_id}-title");
    let description_id = format!("{dialog_id}-description");
    #[cfg(feature = "web")]
    let mounted_dialog_id = dialog_id.clone();

    rsx! {
      dialog {
        id: dialog_id,
        class: "m-auto w-[min(28rem,calc(100%-2rem))] rounded-xl",
        class: "border border-border bg-background p-0 text-foreground shadow-xl",
        class: "backdrop:bg-zinc-950/40 backdrop:backdrop-blur-sm",
        aria_labelledby: title_id.clone(),
        aria_describedby: description_id.clone(),
        onmounted: move |_| {
            #[cfg(feature = "web")]
            {
                let _ = document::eval(
                    &format!("document.getElementById({mounted_dialog_id:?})?.showModal();"),
                );
            }
        },
        oncancel: move |_| oncancel.call(()),

        form { method: "dialog", class: "p-6",
          h2 { id: title_id, class: "text-base font-semibold", "确认删除歌曲？" }
          p {
            id: description_id,
            class: "mt-2 text-sm leading-6 text-muted-foreground",
            "“{song_name}”及对应的本地文件会被永久删除，此操作无法撤销。"
          }
          div { class: "mt-6 flex justify-end gap-2",
            button {
              class: "inline-flex h-9 items-center justify-center rounded-md",
              class: "border border-border bg-background px-4 text-sm font-medium hover:bg-muted",
              r#type: "submit",
              value: "cancel",
              autofocus: true,
              disabled: pending,
              onclick: move |_| oncancel.call(()),
              "返回"
            }
            button {
              class: "inline-flex h-9 items-center justify-center rounded-md",
              class: "bg-destructive px-4 text-sm font-medium text-white hover:opacity-90",
              class: "disabled:pointer-events-none disabled:opacity-50",
              r#type: "submit",
              value: "confirm",
              disabled: pending,
              onclick: move |_| onconfirm.call(()),
              if pending {
                "删除中…"
              } else {
                "确认删除"
              }
            }
          }
        }
      }
    }
}
