use super::{
    config,
    utils::{construct_and_render_block, inset_rect},
    Alignment, Borders, Constraint, Frame, Gauge, Layout, Line, LineGauge, Modifier, Paragraph,
    PlaybackMetadata, Rect, SharedState, Span, Style, Text, UIStateGuard, Wrap,
};
#[cfg(feature = "image")]
use crate::state::ImageRenderInfo;
use crate::{
    state::Track,
    ui::utils::{format_genres, to_bidi_string},
};
use rspotify::model::Id;

const PANEL_SIDE_PADDING: u16 = 1;
const PLAYBACK_TEXT_LEFT_SHIFT: u16 = 1;
const VIS_BOX_RIGHT_SHRINK: u16 = 1;
const PROGRESS_BAR_HEIGHT: u16 = 1;
const RIGHT_PROGRESS_LEFT_EXTENSION: u16 = 1;
const RIGHT_PROGRESS_SIDE_EXTRA_PADDING: u16 = 1;
const RIGHT_TOP_PROGRESS_TEXT_PADDING: u16 = 1;
const BOTTOM_TRACK_INFO_NUDGE: u16 = 1;
const VISUALIZER_INNER_PADDING_X: u16 = 2;
const VISUALIZER_INNER_PADDING_Y: u16 = 1;
#[cfg(feature = "streaming")]
const HEADER_VIS_INNER_HEIGHT: u16 = 4;
#[cfg(feature = "streaming")]
const BOTTOM_VIS_INNER_HEIGHT: u16 = 8;
#[cfg(feature = "image")]
const COVER_IMG_LEFT_PADDING: u16 = 1;
#[cfg(feature = "image")]
const COVER_IMG_TOP_PADDING: u16 = 1;
#[cfg(feature = "image")]
const COVER_IMG_BOTTOM_PADDING: usize = 1;

#[derive(Clone, Copy, Debug, Default)]
struct PlaybackLayoutRects {
    #[cfg(feature = "image")]
    cover_img_rect: Option<Rect>,
    right_area_rect: Rect,
    playback_text_rect: Option<Rect>,
    progress_bar_rect: Rect,
    visualizer_rect: Option<Rect>,
}

#[derive(Clone, Copy, Debug, Default)]
struct RightColumnLayout {
    playback_text_rect: Option<Rect>,
    progress_bar_rect: Option<Rect>,
    visualizer_rect: Option<Rect>,
}

/// Render a playback window showing information about the current playback, which includes
/// - track title, artists, album
/// - playback metadata (playing state, repeat state, shuffle state, volume, device, etc)
/// - cover image (if `image` feature is enabled)
/// - playback progress bar
pub fn render_playback_window(
    frame: &mut Frame,
    state: &SharedState,
    ui: &mut UIStateGuard,
    rect: Rect,
) -> Rect {
    #[cfg(feature = "streaming")]
    let visualizer_preset = ui.visualizer_preset.clone();

    #[cfg(feature = "streaming")]
    let (rect, other_rect) = split_rect_for_playback_window(state, rect, &visualizer_preset);
    #[cfg(not(feature = "streaming"))]
    let (rect, other_rect) = split_rect_for_playback_window(state, rect);
    let rect = construct_and_render_block("Playback", &ui.theme, Borders::ALL, frame, rect);

    let player = state.player.read();
    if let Some(ref playback) = player.playback {
        if let Some(item) = &playback.item {
            let playback_text = player
                .buffered_playback
                .as_ref()
                .map(|playback| construct_playback_text(ui, state, item, playback));

            #[cfg(feature = "streaming")]
            let render_visualization = should_show_visualizer_area(
                state,
                player.buffered_playback.as_ref(),
                &visualizer_preset,
            );

            #[cfg(not(feature = "streaming"))]
            let render_visualization = false;

            #[cfg(feature = "streaming")]
            let layout = split_rect_for_playback_layout(
                rect,
                playback_text
                    .as_ref()
                    .map_or(0, |text| text.lines.len() as u16),
                render_visualization,
                &visualizer_preset,
                #[cfg(feature = "image")]
                &ui.picker,
            );
            #[cfg(not(feature = "streaming"))]
            let layout = split_rect_for_playback_layout(
                rect,
                playback_text
                    .as_ref()
                    .map_or(0, |text| text.lines.len() as u16),
                render_visualization,
                #[cfg(feature = "image")]
                &ui.picker,
            );

            #[cfg(feature = "image")]
            if let Some(cover_img_rect) = layout.cover_img_rect {
                let url = match item {
                    rspotify::model::PlayableItem::Track(track) => {
                        crate::utils::get_track_album_image_url(track).map(String::from)
                    }
                    rspotify::model::PlayableItem::Episode(episode) => {
                        crate::utils::get_episode_show_image_url(episode).map(String::from)
                    }
                    rspotify::model::PlayableItem::Unknown(_) => None,
                };
                if let Some(url) = url {
                    let data = state.data.read();
                    if let Some(img) = data.caches.images.get(&url) {
                        if ui.last_cover_image_render_info.url != url
                            || ui.last_cover_image_render_info.render_area != cover_img_rect
                        {
                            let cover_state = match crate::ui::cover_image::CoverImage::new(
                                &ui.picker,
                                img,
                                cover_img_rect,
                            ) {
                                Ok(cover) => Some(cover),
                                Err(err) => {
                                    tracing::error!("Failed to encode cover image: {err:#}");
                                    None
                                }
                            };
                            ui.last_cover_image_render_info = ImageRenderInfo {
                                url,
                                render_area: cover_img_rect,
                                state: cover_state,
                            };
                        }
                        let area = ui.last_cover_image_render_info.render_area;
                        if let Some(cover) = ui.last_cover_image_render_info.state.as_mut() {
                            cover.render(frame, area);
                        }
                    }
                }
            }

            clear_area(frame, layout.right_area_rect, &ui.theme);

            if let (Some(playback_text), Some(playback_text_rect)) =
                (playback_text, layout.playback_text_rect)
            {
                let playback_desc = Paragraph::new(playback_text);
                frame.render_widget(
                    playback_desc,
                    inset_rect(playback_text_rect, PLAYBACK_TEXT_LEFT_SHIFT, 0, 0, 0),
                );
            }

            let duration = match item {
                rspotify::model::PlayableItem::Track(track) => track.duration,
                rspotify::model::PlayableItem::Episode(episode) => episode.duration,
                rspotify::model::PlayableItem::Unknown(item) => {
                    log::warn!("Unknown playback item: {item:?}");
                    return other_rect;
                }
            };

            let progress = std::cmp::min(
                player.playback_progress().expect("non-empty playback"),
                duration,
            );
            #[cfg(feature = "streaming")]
            let progress_side_padding = PANEL_SIDE_PADDING.saturating_add(
                if matches!(
                    visualizer_preset.progress_bar_placement,
                    config::ProgressBarPlacement::TopRightOfCover
                        | config::ProgressBarPlacement::BottomRightOfCover
                ) {
                    RIGHT_PROGRESS_SIDE_EXTRA_PADDING
                } else {
                    0
                },
            );
            #[cfg(not(feature = "streaming"))]
            let progress_side_padding = PANEL_SIDE_PADDING;
            let progress_bar_rect = inset_rect(
                layout.progress_bar_rect,
                progress_side_padding,
                progress_side_padding,
                0,
                0,
            );
            render_playback_progress_bar(frame, ui, progress, duration, progress_bar_rect);
            #[cfg(feature = "streaming")]
            if let Some(mut vis_r) = layout.visualizer_rect {
                if visualizer_preset.visualizer_border {
                    if visualizer_preset.visualizer_placement
                        == config::VisualizerPlacement::RightOfCover
                    {
                        vis_r = inset_rect(vis_r, 0, VIS_BOX_RIGHT_SHRINK, 0, 0);
                    }
                    vis_r = construct_and_render_block("", &ui.theme, Borders::ALL, frame, vis_r);
                    vis_r = inset_rect(
                        vis_r,
                        VISUALIZER_INNER_PADDING_X.saturating_sub(1),
                        VISUALIZER_INNER_PADDING_X.saturating_sub(1),
                        VISUALIZER_INNER_PADDING_Y.saturating_sub(1),
                        VISUALIZER_INNER_PADDING_Y.saturating_sub(1),
                    );
                } else {
                    vis_r = inset_rect(
                        vis_r,
                        VISUALIZER_INNER_PADDING_X,
                        VISUALIZER_INNER_PADDING_X,
                        VISUALIZER_INNER_PADDING_Y,
                        VISUALIZER_INNER_PADDING_Y,
                    );
                }

                if vis_r.width > 0 && vis_r.height > 0 {
                    super::streaming::render_audio_visualization(
                        frame,
                        state,
                        &ui.theme,
                        vis_r,
                        &visualizer_preset,
                    );
                }
            }
            return other_rect;
        }
    }

    // Previously rendered image can result in a weird rendering text,
    // clear the previous widget's area before rendering the text.
    #[cfg(feature = "image")]
    {
        ui.last_cover_image_render_info = ImageRenderInfo::default();
    }

    if player.playback_last_updated_time.is_none() {
        // Still waiting for the first successful playback fetch — show animated loading indicator
        const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let frame_idx = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            / 100) as usize
            % SPINNER_FRAMES.len();
        let vertical_chunks = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .split(rect);
        frame.render_widget(
            Paragraph::new(format!("{} Loading...", SPINNER_FRAMES[frame_idx]))
                .style(ui.theme.playback_metadata())
                .alignment(Alignment::Center),
            vertical_chunks[1],
        );
    } else {
        frame.render_widget(
            Paragraph::new(
                "No playback found. Please start a new playback.\n \
                 Make sure there is a running Spotify device and try to connect to one using the `SwitchDevice` command."
            )
            .wrap(Wrap { trim: true }),
            rect,
        );
    }

    other_rect
}

fn split_rect_for_playback_layout(
    rect: Rect,
    text_height: u16,
    render_visualization: bool,
    #[cfg(feature = "streaming")] visualizer_preset: &config::VisualizerPreset,
    #[cfg(feature = "image")] picker: &ratatui_image::picker::Picker,
) -> PlaybackLayoutRects {
    #[cfg(feature = "streaming")]
    let progress_bar_placement = visualizer_preset.progress_bar_placement.clone();
    #[cfg(not(feature = "streaming"))]
    let progress_bar_placement = config::ProgressBarPlacement::BottomOfCover;
    #[cfg(feature = "streaming")]
    let track_info_placement = visualizer_preset.track_info_placement;
    #[cfg(not(feature = "streaming"))]
    let track_info_placement = config::TrackInfoPlacement::TopRight;

    #[cfg(feature = "streaming")]
    let visualizer_in_right = render_visualization
        && visualizer_preset.visualizer_placement == config::VisualizerPlacement::RightOfCover;
    #[cfg(feature = "streaming")]
    let visualizer_in_bottom = render_visualization
        && visualizer_preset.visualizer_placement == config::VisualizerPlacement::BottomOfCover;
    #[cfg(not(feature = "streaming"))]
    let visualizer_in_right = false;
    #[cfg(not(feature = "streaming"))]
    let visualizer_in_bottom = false;

    let progress_in_right_top =
        progress_bar_placement == config::ProgressBarPlacement::TopRightOfCover;
    let progress_in_right_bottom =
        progress_bar_placement == config::ProgressBarPlacement::BottomRightOfCover;
    let progress_full_width = matches!(
        progress_bar_placement,
        config::ProgressBarPlacement::BottomOfCover
    );
    let resolved_track_info_placement =
        resolve_track_info_placement(track_info_placement, visualizer_in_right);
    let top_right_progress_gap = if progress_in_right_top
        && resolved_track_info_placement == config::TrackInfoPlacement::TopRight
    {
        RIGHT_TOP_PROGRESS_TEXT_PADDING
    } else {
        0
    };

    let bottom_visualizer_height = if visualizer_in_bottom {
        visualizer_total_height(config::VisualizerPlacement::BottomOfCover)
    } else {
        0
    };
    let bottom_progress_height = if progress_full_width {
        progress_bar_total_height()
    } else {
        0
    };
    let bottom_total_height = bottom_visualizer_height + bottom_progress_height;

    let (body_rect, bottom_rect) = if bottom_total_height > 0 {
        let chunks =
            Layout::vertical([Constraint::Fill(0), Constraint::Length(bottom_total_height)])
                .split(rect);
        (chunks[0], Some(chunks[1]))
    } else {
        (rect, None)
    };

    let mut bottom_progress_rect = None;
    let mut bottom_visualizer_rect = None;
    if let Some(bottom_rect) = bottom_rect {
        let mut remaining = bottom_rect;
        if progress_full_width {
            let chunks = Layout::vertical([
                Constraint::Length(progress_bar_total_height()),
                Constraint::Fill(0),
            ])
            .split(remaining);
            bottom_progress_rect = Some(inset_progress_slot(chunks[0]));
            remaining = chunks[1];
        }
        if visualizer_in_bottom {
            bottom_visualizer_rect = Some(remaining);
        }
    }

    #[cfg(feature = "image")]
    let (cover_img_rect, right_area_rect) = {
        let (cover_img_rect, right_area_rect) = split_rect_for_cover_img(body_rect, picker);
        (Some(cover_img_rect), right_area_rect)
    };
    #[cfg(not(feature = "image"))]
    let right_area_rect = body_rect;

    let right_layout = split_rect_for_right_column(
        right_area_rect,
        text_height,
        visualizer_in_right,
        progress_in_right_top,
        progress_in_right_bottom,
        progress_full_width,
        top_right_progress_gap,
        resolved_track_info_placement,
    );

    PlaybackLayoutRects {
        #[cfg(feature = "image")]
        cover_img_rect,
        right_area_rect,
        playback_text_rect: right_layout.playback_text_rect,
        progress_bar_rect: right_layout
            .progress_bar_rect
            .or(bottom_progress_rect)
            .unwrap_or_default(),
        visualizer_rect: right_layout.visualizer_rect.or(bottom_visualizer_rect),
    }
}

#[cfg(feature = "streaming")]
fn should_show_visualizer_area(
    state: &SharedState,
    playback: Option<&PlaybackMetadata>,
    visualizer_preset: &config::VisualizerPreset,
) -> bool {
    let configs = config::get_config();
    if !configs.app_config.enable_audio_visualization {
        return false;
    }

    if state.is_local_streaming_active() {
        return true;
    }

    visualizer_preset.visualizer_show_on_pause
        && playback.is_some_and(|playback| playback.device_name == configs.app_config.device.name)
}

fn split_rect_for_right_column(
    rect: Rect,
    text_height: u16,
    visualizer_in_right: bool,
    progress_in_right_top: bool,
    progress_in_right_bottom: bool,
    progress_full_width: bool,
    top_right_progress_gap: u16,
    track_info_placement: config::TrackInfoPlacement,
) -> RightColumnLayout {
    if rect.width == 0 || rect.height == 0 {
        return RightColumnLayout::default();
    }

    match track_info_placement {
        config::TrackInfoPlacement::TopRight => {
            let text_rect = if text_height == 0 {
                None
            } else {
                let text_area_height = text_height.saturating_add(if progress_in_right_top {
                    top_right_progress_gap
                } else {
                    0
                });
                let chunks =
                    Layout::vertical([Constraint::Length(text_area_height), Constraint::Fill(0)])
                        .split(rect);

                let mut layout = split_rect_for_right_column_content(
                    chunks[1],
                    Some(chunks[0]),
                    visualizer_in_right,
                    progress_in_right_top,
                    progress_in_right_bottom,
                );
                layout.playback_text_rect = adjust_track_info_rect(
                    layout.playback_text_rect,
                    visualizer_in_right,
                    progress_in_right_bottom,
                    progress_full_width,
                    track_info_placement,
                );
                return layout;
            };

            let mut layout = split_rect_for_right_column_content(
                rect,
                text_rect,
                visualizer_in_right,
                progress_in_right_top,
                progress_in_right_bottom,
            );
            layout.playback_text_rect = adjust_track_info_rect(
                layout.playback_text_rect,
                visualizer_in_right,
                progress_in_right_bottom,
                progress_full_width,
                track_info_placement,
            );
            layout
        }
        config::TrackInfoPlacement::MiddleRight => {
            let mut layout = RightColumnLayout::default();
            let mut remaining = rect;

            if progress_in_right_top {
                let chunks = Layout::vertical([
                    Constraint::Length(progress_bar_total_height()),
                    Constraint::Fill(0),
                ])
                .split(remaining);
                layout.progress_bar_rect = Some(inset_right_progress_slot(chunks[0]));
                remaining = chunks[1];
            } else if progress_in_right_bottom {
                let chunks = Layout::vertical([
                    Constraint::Fill(0),
                    Constraint::Length(progress_bar_total_height()),
                ])
                .split(remaining);
                remaining = chunks[0];
                layout.progress_bar_rect = Some(inset_right_progress_slot(chunks[1]));
            }

            if visualizer_in_right {
                layout.visualizer_rect = Some(remaining);
            }

            if text_height > 0 && remaining.height > 0 {
                let height = text_height.min(remaining.height);
                let y = remaining
                    .y
                    .saturating_add(remaining.height.saturating_sub(height) / 2);
                layout.playback_text_rect =
                    Some(Rect::new(remaining.x, y, remaining.width, height));
            }

            layout
        }
        config::TrackInfoPlacement::BottomRight => {
            let mut layout = RightColumnLayout::default();
            let mut remaining = rect;

            if progress_in_right_bottom {
                let chunks = Layout::vertical([
                    Constraint::Fill(0),
                    Constraint::Length(progress_bar_total_height()),
                ])
                .split(remaining);
                remaining = chunks[0];
                layout.progress_bar_rect = Some(inset_right_progress_slot(chunks[1]));
            } else if progress_in_right_top {
                let chunks = Layout::vertical([
                    Constraint::Length(progress_bar_total_height()),
                    Constraint::Fill(0),
                ])
                .split(remaining);
                layout.progress_bar_rect = Some(inset_right_progress_slot(chunks[0]));
                remaining = chunks[1];
            }

            if visualizer_in_right {
                layout.visualizer_rect = Some(remaining);
            }

            if text_height > 0 && remaining.height > 0 {
                let height = text_height.min(remaining.height);
                let mut y = remaining.bottom().saturating_sub(height);
                if !visualizer_in_right && (progress_in_right_bottom || progress_full_width) {
                    y = y.saturating_sub(BOTTOM_TRACK_INFO_NUDGE);
                }
                layout.playback_text_rect =
                    Some(Rect::new(remaining.x, y, remaining.width, height));
            }

            layout
        }
    }
}

fn split_rect_for_right_column_content(
    rect: Rect,
    playback_text_rect: Option<Rect>,
    visualizer_in_right: bool,
    progress_in_right_top: bool,
    progress_in_right_bottom: bool,
) -> RightColumnLayout {
    let mut layout = RightColumnLayout {
        playback_text_rect,
        ..RightColumnLayout::default()
    };

    if rect.width == 0 || rect.height == 0 {
        return layout;
    }

    match (
        progress_in_right_top,
        progress_in_right_bottom,
        visualizer_in_right,
    ) {
        (true, false, true) => {
            let chunks = Layout::vertical([
                Constraint::Length(progress_bar_total_height()),
                Constraint::Fill(0),
            ])
            .split(rect);
            layout.progress_bar_rect = Some(inset_right_progress_slot(chunks[0]));
            layout.visualizer_rect = Some(chunks[1]);
        }
        (true, false, false) => {
            let chunks = Layout::vertical([
                Constraint::Length(progress_bar_total_height()),
                Constraint::Fill(0),
            ])
            .split(rect);
            layout.progress_bar_rect = Some(inset_right_progress_slot(chunks[0]));
        }
        (false, true, true) => {
            let chunks = Layout::vertical([
                Constraint::Fill(0),
                Constraint::Length(progress_bar_total_height()),
            ])
            .split(rect);
            layout.visualizer_rect = Some(chunks[0]);
            layout.progress_bar_rect = Some(inset_right_progress_slot(chunks[1]));
        }
        (false, true, false) => {
            let chunks = Layout::vertical([
                Constraint::Fill(0),
                Constraint::Length(progress_bar_total_height()),
            ])
            .split(rect);
            layout.progress_bar_rect = Some(inset_right_progress_slot(chunks[1]));
        }
        _ if visualizer_in_right => {
            layout.visualizer_rect = Some(rect);
        }
        _ => {}
    }

    layout
}

fn adjust_track_info_rect(
    rect: Option<Rect>,
    visualizer_in_right: bool,
    progress_in_right_bottom: bool,
    progress_full_width: bool,
    track_info_placement: config::TrackInfoPlacement,
) -> Option<Rect> {
    rect.map(|rect| {
        if visualizer_in_right {
            return rect;
        }

        match track_info_placement {
            config::TrackInfoPlacement::TopRight
                if progress_in_right_bottom || progress_full_width =>
            {
                Rect::new(
                    rect.x,
                    rect.y.saturating_add(BOTTOM_TRACK_INFO_NUDGE),
                    rect.width,
                    rect.height,
                )
            }
            _ => rect,
        }
    })
}

fn visualizer_total_height(placement: config::VisualizerPlacement) -> u16 {
    let inner_height = match placement {
        config::VisualizerPlacement::BottomOfCover => BOTTOM_VIS_INNER_HEIGHT,
        config::VisualizerPlacement::RightOfCover => HEADER_VIS_INNER_HEIGHT,
    };

    inner_height + VISUALIZER_INNER_PADDING_Y.saturating_mul(2)
}

fn progress_bar_total_height() -> u16 {
    PROGRESS_BAR_HEIGHT
}

fn inset_progress_slot(rect: Rect) -> Rect {
    rect
}

fn inset_right_progress_slot(rect: Rect) -> Rect {
    extend_rect_left(inset_progress_slot(rect), RIGHT_PROGRESS_LEFT_EXTENSION)
}

fn resolve_track_info_placement(
    requested: config::TrackInfoPlacement,
    visualizer_in_right: bool,
) -> config::TrackInfoPlacement {
    if visualizer_in_right {
        config::TrackInfoPlacement::TopRight
    } else {
        requested
    }
}

fn extend_rect_left(rect: Rect, amount: u16) -> Rect {
    let shift = rect.x.min(amount);
    Rect::new(
        rect.x.saturating_sub(shift),
        rect.y,
        rect.width.saturating_add(shift),
        rect.height,
    )
}

#[cfg(feature = "image")]
fn split_rect_for_cover_img(rect: Rect, picker: &ratatui_image::picker::Picker) -> (Rect, Rect) {
    let configs = config::get_config();
    let hor_chunks = Layout::horizontal([
        Constraint::Length(cover_img_length(configs, picker) + COVER_IMG_LEFT_PADDING),
        Constraint::Fill(0), // metadata_rect
    ])
    .spacing(1)
    .split(rect);
    let ver_chunks = Layout::vertical([Constraint::Length(
        configs.app_config.cover_img_width as u16
            + COVER_IMG_TOP_PADDING
            + COVER_IMG_BOTTOM_PADDING as u16,
    )])
    .split(hor_chunks[0]);

    (
        inset_rect(
            ver_chunks[0],
            COVER_IMG_LEFT_PADDING,
            0,
            COVER_IMG_TOP_PADDING,
            COVER_IMG_BOTTOM_PADDING as u16,
        ),
        hor_chunks[1],
    )
}

/// Determine the cover image box's width in columns.
#[cfg(feature = "image")]
fn cover_img_length(configs: &config::Configs, picker: &ratatui_image::picker::Picker) -> u16 {
    match configs.app_config.cover_img_length {
        // When `cover_img_length` is `0` (the default), derive it from the terminal's cell aspect ratio
        0 => {
            let font_size = picker.font_size();
            let rows = configs.app_config.cover_img_width as u16;
            if font_size.width == 0 {
                rows
            } else {
                rows * font_size.height / font_size.width
            }
        }
        length => length as u16,
    }
}

fn clear_area(frame: &mut Frame, rect: Rect, theme: &config::Theme) {
    for x in rect.left()..rect.right() {
        for y in rect.top()..rect.bottom() {
            frame
                .buffer_mut()
                .cell_mut((x, y))
                .expect("invalid cell")
                .set_char(' ')
                .set_style(theme.app());
        }
    }
}

fn construct_playback_text(
    ui: &UIStateGuard,
    state: &SharedState,
    playable: &rspotify::model::PlayableItem,
    playback: &PlaybackMetadata,
) -> Text<'static> {
    // Construct a "styled" text (`playback_text`) from playback's data
    // based on a user-configurable format string (app_config.playback_format)
    let configs = config::get_config();
    let format_str = &configs.app_config.playback_format;
    let data = state.data.read();

    let mut playback_text = Text::default();
    let mut spans = vec![];

    // this regex is to handle a format argument or a newline
    let re = regex::Regex::new(r"\{.*?\}|\n").unwrap();

    let mut ptr = 0;
    for m in re.find_iter(format_str) {
        let s = m.start();
        let e = m.end();
        if ptr < s {
            spans.push(Span::raw(format_str[ptr..s].to_string()));
        }
        ptr = e;

        let (text, style) = match m.as_str() {
            // upon encountering a newline, create a new `Spans`
            "\n" => {
                let mut tmp = vec![];
                std::mem::swap(&mut tmp, &mut spans);
                playback_text.lines.push(Line::from(tmp));
                continue;
            }
            "{status}" => (
                if playback.is_playing {
                    &configs.app_config.play_icon
                } else {
                    &configs.app_config.pause_icon
                }
                .to_owned(),
                ui.theme.playback_status(),
            ),
            "{liked}" => match playable {
                rspotify::model::PlayableItem::Track(track) => match &track.id {
                    Some(id) => {
                        if data.user_data.saved_tracks.contains_key(&id.uri()) {
                            (configs.app_config.liked_icon.clone(), ui.theme.like())
                        } else {
                            continue;
                        }
                    }
                    None => continue,
                },
                rspotify::model::PlayableItem::Episode(_)
                | rspotify::model::PlayableItem::Unknown(_) => continue,
            },
            "{track}" => match playable {
                rspotify::model::PlayableItem::Track(track) => (
                    {
                        let display = Track::try_from_full_track(track.clone()).map_or_else(
                            || "Unknown Track".to_string(),
                            |t| to_bidi_string(&t.display_name()),
                        );
                        display
                    },
                    ui.theme.playback_track(),
                ),
                rspotify::model::PlayableItem::Episode(episode) => (
                    {
                        let bidi_string = to_bidi_string(&episode.name);
                        if episode.explicit {
                            format!("{bidi_string} (E)")
                        } else {
                            bidi_string
                        }
                    },
                    ui.theme.playback_track(),
                ),
                rspotify::model::PlayableItem::Unknown(_) => {
                    continue;
                }
            },
            "{track_number}" => match playable {
                rspotify::model::PlayableItem::Track(track) => (
                    { to_bidi_string(&track.track_number.to_string()) },
                    ui.theme.playback_track(),
                ),
                rspotify::model::PlayableItem::Episode(_)
                | rspotify::model::PlayableItem::Unknown(_) => {
                    continue;
                }
            },
            "{artists}" => match playable {
                rspotify::model::PlayableItem::Track(track) => (
                    to_bidi_string(&crate::utils::map_join(&track.artists, |a| &a.name, ", ")),
                    ui.theme.playback_artists(),
                ),
                #[allow(deprecated)]
                rspotify::model::PlayableItem::Episode(episode) => {
                    (episode.show.publisher.clone(), ui.theme.playback_artists())
                }
                rspotify::model::PlayableItem::Unknown(_) => {
                    continue;
                }
            },
            "{album}" => match playable {
                rspotify::model::PlayableItem::Track(track) => {
                    (to_bidi_string(&track.album.name), ui.theme.playback_album())
                }
                rspotify::model::PlayableItem::Episode(episode) => (
                    to_bidi_string(&episode.show.name),
                    ui.theme.playback_album(),
                ),
                rspotify::model::PlayableItem::Unknown(_) => {
                    continue;
                }
            },
            "{genres}" => match playable {
                rspotify::model::PlayableItem::Track(full_track) => {
                    let genre = match data.caches.genres.get(&full_track.artists[0].name) {
                        Some(genres) => &format_genres(genres, configs.app_config.genre_num),
                        None => "no genre",
                    };
                    (to_bidi_string(genre), ui.theme.playback_genres())
                }
                rspotify::model::PlayableItem::Episode(_) => {
                    (to_bidi_string("no genre"), ui.theme.playback_genres())
                }
                rspotify::model::PlayableItem::Unknown(_) => {
                    continue;
                }
            },
            "{metadata}" => {
                let repeat_value = <&'static str>::from(playback.repeat_state).to_string();

                let volume_value = if let Some(volume) = playback.mute_state {
                    format!("{volume}% (muted)")
                } else {
                    format!("{}%", playback.volume.unwrap_or_default())
                };

                let mut parts = vec![];

                for field in &configs.app_config.playback_metadata_fields {
                    match field.as_str() {
                        "repeat" => parts.push(format!("repeat: {repeat_value}")),
                        "shuffle" => parts.push(format!("shuffle: {}", playback.shuffle_state)),
                        "volume" => parts.push(format!("volume: {volume_value}")),
                        "device" => parts.push(format!("device: {}", playback.device_name)),
                        _ => {}
                    }
                }

                let metadata_str = parts.join(" | ");
                (metadata_str, ui.theme.playback_metadata())
            }
            _ => continue,
        };

        spans.push(Span::styled(text, style));
    }
    if ptr < format_str.len() {
        spans.push(Span::raw(format_str[ptr..].to_string()));
    }
    if !spans.is_empty() {
        playback_text.lines.push(Line::from(spans));
    }

    playback_text
}

fn render_playback_progress_bar(
    frame: &mut Frame,
    ui: &mut UIStateGuard,
    progress: chrono::Duration,
    duration: chrono::Duration,
    rect: Rect,
) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }

    // Negative numbers can sometimes appear from progress.num_seconds() so this stops
    // them coming through into the ratios
    let ratio = (progress.num_seconds() as f64 / duration.num_seconds() as f64).clamp(0.0, 1.0);

    match config::get_config().app_config.progress_bar_type {
        config::ProgressBarType::Line => frame.render_widget(
            LineGauge::default()
                .filled_style(ui.theme.playback_progress_bar())
                .unfilled_style(ui.theme.playback_progress_bar_unfilled())
                .ratio(ratio)
                .label(Span::styled(
                    format!(
                        "{}/{}",
                        crate::utils::format_duration(&progress),
                        crate::utils::format_duration(&duration),
                    ),
                    Style::default().add_modifier(Modifier::BOLD),
                )),
            rect,
        ),
        config::ProgressBarType::Rectangle => frame.render_widget(
            Gauge::default()
                .gauge_style(ui.theme.playback_progress_bar())
                .ratio(ratio)
                .label(Span::styled(
                    format!(
                        "{}/{}",
                        crate::utils::format_duration(&progress),
                        crate::utils::format_duration(&duration),
                    ),
                    Style::default().add_modifier(Modifier::BOLD),
                )),
            rect,
        ),
    }

    // update the progress bar's position stored inside the UI state
    ui.playback_progress_bar_rect = rect;
}

/// Split the given area into two, the first one for the playback window
/// and the second one for the main application's layout (popup, page, etc).
fn split_rect_for_playback_window(
    state: &SharedState,
    rect: Rect,
    #[cfg(feature = "streaming")] visualizer_preset: &config::VisualizerPreset,
) -> (Rect, Rect) {
    let configs = config::get_config();
    #[cfg(feature = "streaming")]
    let progress_bar_placement = visualizer_preset.progress_bar_placement.clone();
    #[cfg(not(feature = "streaming"))]
    let progress_bar_placement = config::ProgressBarPlacement::BottomOfCover;
    let mut inner_height = configs.app_config.layout.playback_window_height as u16;

    #[cfg(feature = "image")]
    {
        inner_height = inner_height.max(
            configs.app_config.cover_img_width as u16
                + COVER_IMG_TOP_PADDING
                + COVER_IMG_BOTTOM_PADDING as u16,
        );
    }

    let playback_text_height = configs.app_config.playback_format.lines().count() as u16;

    #[cfg(feature = "streaming")]
    let render_visualization = {
        let player = state.player.read();
        should_show_visualizer_area(state, player.buffered_playback.as_ref(), visualizer_preset)
    };
    #[cfg(not(feature = "streaming"))]
    let render_visualization = false;
    #[cfg(feature = "streaming")]
    let track_info_placement = resolve_track_info_placement(
        visualizer_preset.track_info_placement,
        render_visualization
            && visualizer_preset.visualizer_placement == config::VisualizerPlacement::RightOfCover,
    );
    #[cfg(not(feature = "streaming"))]
    let track_info_placement = config::TrackInfoPlacement::TopRight;

    let right_progress_height = if matches!(
        progress_bar_placement,
        config::ProgressBarPlacement::TopRightOfCover
            | config::ProgressBarPlacement::BottomRightOfCover
    ) {
        progress_bar_total_height()
    } else {
        0
    };
    let top_right_progress_gap = if progress_bar_placement
        == config::ProgressBarPlacement::TopRightOfCover
        && track_info_placement == config::TrackInfoPlacement::TopRight
    {
        RIGHT_TOP_PROGRESS_TEXT_PADDING
    } else {
        0
    };

    #[cfg(feature = "streaming")]
    let right_visualizer_height = if render_visualization
        && visualizer_preset.visualizer_placement == config::VisualizerPlacement::RightOfCover
    {
        visualizer_total_height(config::VisualizerPlacement::RightOfCover)
    } else {
        0
    };
    #[cfg(not(feature = "streaming"))]
    let right_visualizer_height = 0;

    inner_height = inner_height.max(
        playback_text_height
            + top_right_progress_gap
            + right_progress_height
            + right_visualizer_height,
    );

    let full_width_progress_height = if matches!(
        progress_bar_placement,
        config::ProgressBarPlacement::BottomOfCover
    ) {
        progress_bar_total_height()
    } else {
        0
    };

    #[cfg(feature = "streaming")]
    let full_width_visualizer_height = if render_visualization
        && visualizer_preset.visualizer_placement == config::VisualizerPlacement::BottomOfCover
    {
        visualizer_total_height(config::VisualizerPlacement::BottomOfCover)
    } else {
        0
    };
    #[cfg(not(feature = "streaming"))]
    let full_width_visualizer_height = 0;

    let playback_height =
        inner_height + full_width_progress_height + full_width_visualizer_height + 2;

    match configs.app_config.layout.playback_window_position {
        config::Position::Top => {
            let chunks =
                Layout::vertical([Constraint::Length(playback_height), Constraint::Fill(0)])
                    .split(rect);

            (chunks[0], chunks[1])
        }
        config::Position::Bottom => {
            let chunks =
                Layout::vertical([Constraint::Fill(0), Constraint::Length(playback_height)])
                    .split(rect);

            (chunks[1], chunks[0])
        }
    }
}
