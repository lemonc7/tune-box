use contracts::music::Track;
use dioxus::prelude::*;

#[derive(Clone, PartialEq, Default)]
pub struct PlayerState {
    pub track: Option<Track>,
    pub revision: u64,
    pub error: Option<String>,
}

#[derive(Clone, Copy)]
pub struct PlayerController {
    state: Signal<PlayerState>,
}

impl PlayerController {
    pub fn snapshot(&self) -> PlayerState {
        self.state.read().clone()
    }

    pub fn is_active(&self) -> bool {
        self.state.read().track.is_some()
    }

    pub fn play(&mut self, track: Track) {
        self.state.with_mut(|state| {
            state.track = Some(track);
            state.error = None;

            // 再次点击同一首歌，重新创建audio并从头播放
            state.revision = state.revision.wrapping_add(1);
        })
    }

    pub fn close(&mut self) {
        self.state.with_mut(|state| {
            state.track = None;
            state.error = None;
        })
    }

    pub fn set_error(&mut self, message: impl Into<String>) {
        self.state.write().error = Some(message.into())
    }
}

pub fn use_player_provider() -> PlayerController {
    let player = PlayerController {
        state: use_signal(PlayerState::default),
    };
    use_context_provider(|| player);
    player
}

pub fn use_player() -> PlayerController {
    use_context::<PlayerController>()
}
