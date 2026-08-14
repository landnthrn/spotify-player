use crate::{
    command,
    config::{VisualizerDirection, VisualizerMode},
    state::{
        model::{Album, Artist, Episode, EpisodeId, Playlist, Show, Track, TrackId},
        ItemId,
    },
    ui::single_line_input::LineInput,
};
use ratatui::widgets::ListState;
use rspotify::model::PlaylistId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistCreateCurrentField {
    Name,
    Desc,
}

#[derive(Debug)]
pub enum PopupState {
    Search {
        query: String,
    },
    UserPlaylistList(PlaylistPopupAction, ListState),
    UserFollowedArtistList(ListState),
    UserSavedAlbumList(ListState),
    DeviceList(ListState),
    ArtistList(ArtistPopupAction, Vec<Artist>, ListState),
    ThemeList {
        themes: Vec<crate::config::Theme>,
        list_state: ListState,
        original_theme: crate::config::Theme,
    },
    VisualizerMenu(VisualizerMenuState),
    ActionList(Box<ActionListItem>, ListState),
    PlaylistCreate {
        name: LineInput,
        desc: LineInput,
        current_field: PlaylistCreateCurrentField,
    },
    ConfirmAction {
        message: String,
        action: ConfirmableAction,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualizerMenuColumn {
    Behavior,
    Look,
    Mode,
    Direction,
    Placement,
}

#[derive(Debug)]
pub struct VisualizerMenuState {
    pub config: crate::config::VisualizerConfigFile,
    pub original_visualizer: crate::config::VisualizerPreset,
    pub focus: VisualizerMenuColumn,
    pub behavior_state: ListState,
    pub look_state: ListState,
    pub placement_state: ListState,
    pub mode_state: ListState,
    pub direction_state: ListState,
}

#[derive(Debug, Clone)]
pub enum ConfirmableAction {
    DeleteTrackFromPlaylist {
        playlist_id: PlaylistId<'static>,
        track_id: TrackId<'static>,
    },
    DeleteFromLibrary(ItemId),
}

#[derive(Debug, Clone)]
pub enum ActionListItem {
    Track(Track, Vec<command::Action>),
    Artist(Artist, Vec<command::Action>),
    Album(Album, Vec<command::Action>),
    Playlist(Playlist, Vec<command::Action>),
    Show(Show, Vec<command::Action>),
    Episode(Episode, Vec<command::Action>),
}

/// An action on an item in a playlist popup list
#[derive(Debug)]
pub enum PlaylistPopupAction {
    Browse {
        folder_id: usize,
        search_query: String,
    },
    AddTrack {
        folder_id: usize,
        track_id: TrackId<'static>,
        search_query: String,
    },
    AddEpisode {
        folder_id: usize,
        episode_id: EpisodeId<'static>,
        search_query: String,
    },
}

/// An action on an item in an artist popup list
#[derive(Copy, Clone, Debug)]
pub enum ArtistPopupAction {
    Browse,
    ShowActions,
}

impl PopupState {
    /// gets the (immutable) list state of a (list) popup
    pub fn list_state(&self) -> Option<&ListState> {
        match self {
            Self::DeviceList(list_state)
            | Self::UserPlaylistList(.., list_state)
            | Self::UserFollowedArtistList(list_state)
            | Self::UserSavedAlbumList(list_state)
            | Self::ArtistList(.., list_state)
            | Self::ActionList(.., list_state) => Some(list_state),
            Self::ThemeList { list_state, .. } => Some(list_state),
            Self::Search { .. }
            | Self::PlaylistCreate { .. }
            | Self::ConfirmAction { .. }
            | Self::VisualizerMenu(..) => None,
        }
    }

    /// gets the (mutable) list state of a (list) popup
    pub fn list_state_mut(&mut self) -> Option<&mut ListState> {
        match self {
            Self::DeviceList(list_state)
            | Self::UserPlaylistList(.., list_state)
            | Self::UserFollowedArtistList(list_state)
            | Self::UserSavedAlbumList(list_state)
            | Self::ArtistList(.., list_state)
            | Self::ActionList(.., list_state) => Some(list_state),
            Self::ThemeList { list_state, .. } => Some(list_state),
            Self::Search { .. }
            | Self::PlaylistCreate { .. }
            | Self::ConfirmAction { .. }
            | Self::VisualizerMenu(..) => None,
        }
    }

    /// gets the selected position of a (list) popup
    pub fn list_selected(&self) -> Option<usize> {
        match self.list_state() {
            None => None,
            Some(state) => state.selected(),
        }
    }

    /// selects a position in a (list) popup
    pub fn list_select(&mut self, id: Option<usize>) {
        match self.list_state_mut() {
            None => {}
            Some(state) => state.select(id),
        }
    }
}

impl VisualizerMenuState {
    pub fn new(
        config: crate::config::VisualizerConfigFile,
        original_visualizer: crate::config::VisualizerPreset,
    ) -> Self {
        let mut state = Self {
            config,
            original_visualizer,
            focus: VisualizerMenuColumn::Behavior,
            behavior_state: ListState::default(),
            look_state: ListState::default(),
            placement_state: ListState::default(),
            mode_state: ListState::default(),
            direction_state: ListState::default(),
        };
        state.sync_selections_from_active();
        state
    }

    pub fn behavior_items(&self) -> Vec<String> {
        self.config
            .behavior_presets
            .iter()
            .map(|preset| preset.name.clone())
            .collect()
    }

    pub fn look_items(&self) -> Vec<String> {
        self.config
            .look_presets
            .iter()
            .map(|preset| preset.name.clone())
            .collect()
    }

    pub fn placement_items(&self) -> Vec<String> {
        self.config
            .placement_presets
            .iter()
            .map(|preset| preset.name.clone())
            .collect()
    }

    pub fn mode_items(&self) -> Vec<String> {
        VisualizerMode::ALL
            .iter()
            .map(|mode| mode.as_config_value().to_string())
            .collect()
    }

    pub fn direction_items(&self) -> Vec<String> {
        VisualizerDirection::ALL
            .iter()
            .map(|direction| direction.as_config_value().to_string())
            .collect()
    }

    pub fn focus_next(&mut self) {
        self.focus = match self.focus {
            VisualizerMenuColumn::Behavior => VisualizerMenuColumn::Look,
            VisualizerMenuColumn::Look => VisualizerMenuColumn::Mode,
            VisualizerMenuColumn::Mode => VisualizerMenuColumn::Direction,
            VisualizerMenuColumn::Direction => VisualizerMenuColumn::Placement,
            VisualizerMenuColumn::Placement => VisualizerMenuColumn::Behavior,
        };
    }

    pub fn focus_previous(&mut self) {
        self.focus = match self.focus {
            VisualizerMenuColumn::Behavior => VisualizerMenuColumn::Placement,
            VisualizerMenuColumn::Look => VisualizerMenuColumn::Behavior,
            VisualizerMenuColumn::Mode => VisualizerMenuColumn::Look,
            VisualizerMenuColumn::Direction => VisualizerMenuColumn::Mode,
            VisualizerMenuColumn::Placement => VisualizerMenuColumn::Direction,
        };
    }

    pub fn select_next(&mut self) {
        let len = self.focus_len();
        let current = self.focus_selected().unwrap_or_default();
        if current + 1 < len {
            self.focus_state_mut().select(Some(current + 1));
        }
    }

    pub fn select_previous(&mut self) {
        let current = self.focus_selected().unwrap_or_default();
        if current > 0 {
            self.focus_state_mut().select(Some(current - 1));
        }
    }

    pub fn select_first(&mut self) {
        if self.focus_len() > 0 {
            self.focus_state_mut().select(Some(0));
        }
    }

    pub fn select_last(&mut self) {
        let len = self.focus_len();
        if len > 0 {
            self.focus_state_mut().select(Some(len - 1));
        }
    }

    pub fn preview_preset(&mut self) -> crate::config::VisualizerPreset {
        self.sync_active_from_selections();
        self.config.active_visualizer_preset()
    }

    fn sync_selections_from_active(&mut self) {
        let behavior_id = self
            .config
            .behavior_presets
            .iter()
            .position(|preset| preset.name == self.config.active.behavior_preset)
            .unwrap_or_default();
        let look_id = self
            .config
            .look_presets
            .iter()
            .position(|preset| preset.name == self.config.active.look_preset)
            .unwrap_or_default();
        let placement_id = self
            .config
            .placement_presets
            .iter()
            .position(|preset| preset.name == self.config.active.placement_preset)
            .unwrap_or_default();
        let mode_id = VisualizerMode::ALL
            .iter()
            .position(|mode| *mode == self.config.active.visualizer_mode)
            .unwrap_or_default();
        let direction_id = VisualizerDirection::ALL
            .iter()
            .position(|direction| *direction == self.config.active.visualizer_direction)
            .unwrap_or_default();

        self.behavior_state.select(Some(behavior_id));
        self.look_state.select(Some(look_id));
        self.placement_state.select(Some(placement_id));
        self.mode_state.select(Some(mode_id));
        self.direction_state.select(Some(direction_id));
    }

    fn sync_active_from_selections(&mut self) {
        if let Some(id) = self.behavior_state.selected() {
            if let Some(preset) = self.config.behavior_presets.get(id) {
                self.config.active.behavior_preset = preset.name.clone();
            }
        }
        if let Some(id) = self.look_state.selected() {
            if let Some(preset) = self.config.look_presets.get(id) {
                self.config.active.look_preset = preset.name.clone();
            }
        }
        if let Some(id) = self.placement_state.selected() {
            if let Some(preset) = self.config.placement_presets.get(id) {
                self.config.active.placement_preset = preset.name.clone();
            }
        }
        if let Some(id) = self.mode_state.selected() {
            if let Some(mode) = VisualizerMode::ALL.get(id) {
                self.config.active.visualizer_mode = *mode;
            }
        }
        if let Some(id) = self.direction_state.selected() {
            if let Some(direction) = VisualizerDirection::ALL.get(id) {
                self.config.active.visualizer_direction = *direction;
            }
        }
    }

    fn focus_len(&self) -> usize {
        match self.focus {
            VisualizerMenuColumn::Behavior => self.config.behavior_presets.len(),
            VisualizerMenuColumn::Look => self.config.look_presets.len(),
            VisualizerMenuColumn::Placement => self.config.placement_presets.len(),
            VisualizerMenuColumn::Mode => VisualizerMode::ALL.len(),
            VisualizerMenuColumn::Direction => VisualizerDirection::ALL.len(),
        }
    }

    fn focus_selected(&self) -> Option<usize> {
        match self.focus {
            VisualizerMenuColumn::Behavior => self.behavior_state.selected(),
            VisualizerMenuColumn::Look => self.look_state.selected(),
            VisualizerMenuColumn::Placement => self.placement_state.selected(),
            VisualizerMenuColumn::Mode => self.mode_state.selected(),
            VisualizerMenuColumn::Direction => self.direction_state.selected(),
        }
    }

    fn focus_state_mut(&mut self) -> &mut ListState {
        match self.focus {
            VisualizerMenuColumn::Behavior => &mut self.behavior_state,
            VisualizerMenuColumn::Look => &mut self.look_state,
            VisualizerMenuColumn::Placement => &mut self.placement_state,
            VisualizerMenuColumn::Mode => &mut self.mode_state,
            VisualizerMenuColumn::Direction => &mut self.direction_state,
        }
    }
}

impl ActionListItem {
    pub fn n_actions(&self) -> usize {
        match self {
            ActionListItem::Track(.., actions)
            | ActionListItem::Artist(.., actions)
            | ActionListItem::Album(.., actions)
            | ActionListItem::Playlist(.., actions)
            | ActionListItem::Show(.., actions)
            | ActionListItem::Episode(.., actions) => actions.len(),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            ActionListItem::Track(track, ..) => &track.name,
            ActionListItem::Artist(artist, ..) => &artist.name,
            ActionListItem::Album(album, ..) => &album.name,
            ActionListItem::Playlist(playlist, ..) => &playlist.name,
            ActionListItem::Show(show, ..) => &show.name,
            ActionListItem::Episode(episode, ..) => &episode.name,
        }
    }

    pub fn actions_desc(&self) -> Vec<String> {
        match self {
            ActionListItem::Track(.., actions)
            | ActionListItem::Artist(.., actions)
            | ActionListItem::Album(.., actions)
            | ActionListItem::Playlist(.., actions)
            | ActionListItem::Show(.., actions)
            | ActionListItem::Episode(.., actions) => {
                actions.iter().map(|a| format!("{a:?}")).collect::<Vec<_>>()
            }
        }
    }
}
