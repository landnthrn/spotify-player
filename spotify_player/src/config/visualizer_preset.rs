use anyhow::Result;
use serde::{Deserialize, Serialize};

const DEFAULT_VISUALIZER_CONFIG_TOML: &str = r#"[active]
behavior_preset = "balanced"
look_preset = "boxed"
placement_preset = "vislzr_mr_bar_b_info_tr"
visualizer_mode = "multi_peak_l_c_r"
visualizer_direction = "up"
visualizer_peak_reach = "medium"
visualizer_peak_spread = "regular"

[[behavior_presets]]
name = "balanced"
visualizer_frequency_bias = 1.0
visualizer_sensitivity = 1.0
visualizer_rise_speed = 1.0
visualizer_fall_speed = 1.0
visualizer_low_min_max_length = [0.0, 0.33333334,]
visualizer_mid_min_max_length = [0.33333334, 0.6666667,]
visualizer_high_min_max_length = [0.6666667, 0.85,]
visualizer_peak_min_max_length = [0.85, 1.0,]
visualizer_mid_threshold = 0.33333334
visualizer_high_threshold = 0.6666667
visualizer_peak_threshold = 0.85

[[behavior_presets]]
name = "punch-even"
visualizer_frequency_bias = 1.2
visualizer_sensitivity = 1.0
visualizer_rise_speed = 1.0
visualizer_fall_speed = 1.0
visualizer_low_min_max_length = [0.0, 0.3,]
visualizer_mid_min_max_length = [0.3, 0.6,]
visualizer_high_min_max_length = [0.6, 0.8,]
visualizer_peak_min_max_length = [0.8, 1.0,]
visualizer_mid_threshold = 0.4
visualizer_high_threshold = 0.6
visualizer_peak_threshold = 0.8

[[behavior_presets]]
name = "gradient_blend"
visualizer_frequency_bias = 1.05
visualizer_sensitivity = 0.92
visualizer_rise_speed = 1.0
visualizer_fall_speed = 0.8
visualizer_low_min_max_length = [0.08, 0.34,]
visualizer_mid_min_max_length = [0.22, 0.56,]
visualizer_high_min_max_length = [0.42, 0.8,]
visualizer_peak_min_max_length = [0.68, 1.0,]
visualizer_mid_threshold = 0.5
visualizer_high_threshold = 0.72
visualizer_peak_threshold = 0.93

[[behavior_presets]]
name = "airy_peaks"
visualizer_frequency_bias = 0.9
visualizer_sensitivity = 1.08
visualizer_rise_speed = 1.0
visualizer_fall_speed = 0.72
visualizer_low_min_max_length = [0.0, 0.18,]
visualizer_mid_min_max_length = [0.12, 0.42,]
visualizer_high_min_max_length = [0.32, 0.72,]
visualizer_peak_min_max_length = [0.55, 1.0,]
visualizer_mid_threshold = 0.38
visualizer_high_threshold = 0.62
visualizer_peak_threshold = 0.86

[[behavior_presets]]
name = "slow_fall_swell"
visualizer_frequency_bias = 1.0
visualizer_sensitivity = 0.98
visualizer_rise_speed = 0.9
visualizer_fall_speed = 0.45
visualizer_low_min_max_length = [0.04, 0.26,]
visualizer_mid_min_max_length = [0.22, 0.52,]
visualizer_high_min_max_length = [0.44, 0.76,]
visualizer_peak_min_max_length = [0.66, 1.0,]
visualizer_mid_threshold = 0.45
visualizer_high_threshold = 0.68
visualizer_peak_threshold = 0.9

[[look_presets]]
name = "boxed"
visualizer_bar_thickness = 1
visualizer_bar_gap = 0
visualizer_center_offset = 0
visualizer_border = true
visualizer_show_on_pause = true

[[look_presets]]
name = "tight_boxed"
visualizer_bar_thickness = 1
visualizer_bar_gap = 0
visualizer_center_offset = 0
visualizer_border = true
visualizer_show_on_pause = true

[[look_presets]]
name = "open_air"
visualizer_bar_thickness = 1
visualizer_bar_gap = 1
visualizer_center_offset = 0
visualizer_border = false
visualizer_show_on_pause = true

[[look_presets]]
name = "boxed_spaced"
visualizer_bar_thickness = 1
visualizer_bar_gap = 1
visualizer_center_offset = 0
visualizer_border = true
visualizer_show_on_pause = true
"#;

use super::{
    AppConfig, ProgressBarPlacement, TrackInfoPlacement, VisualizerDirection,
    VisualizerLengthRange, VisualizerMode, VisualizerPeakReach, VisualizerPeakSpread,
    VisualizerPlacement,
};

fn default_visualizer_placement_presets() -> Vec<VisualizerPlacementPreset> {
    vec![
        VisualizerPlacementPreset {
            name: "vislzr_mr_bar_b_info_tr".to_string(),
            visualizer_placement: VisualizerPlacement::RightOfCover,
            progress_bar_placement: ProgressBarPlacement::BottomOfCover,
            track_info_placement: TrackInfoPlacement::TopRight,
        },
        VisualizerPlacementPreset {
            name: "vislzr_mr_bar_tr_info_tr".to_string(),
            visualizer_placement: VisualizerPlacement::RightOfCover,
            progress_bar_placement: ProgressBarPlacement::TopRightOfCover,
            track_info_placement: TrackInfoPlacement::TopRight,
        },
        VisualizerPlacementPreset {
            name: "vislzr_mr_bar_br_info_tr".to_string(),
            visualizer_placement: VisualizerPlacement::RightOfCover,
            progress_bar_placement: ProgressBarPlacement::BottomRightOfCover,
            track_info_placement: TrackInfoPlacement::TopRight,
        },
        VisualizerPlacementPreset {
            name: "vislzr_b_bar_b_info_tr".to_string(),
            visualizer_placement: VisualizerPlacement::BottomOfCover,
            progress_bar_placement: ProgressBarPlacement::BottomOfCover,
            track_info_placement: TrackInfoPlacement::TopRight,
        },
        VisualizerPlacementPreset {
            name: "vislzr_b_bar_b_info_mr".to_string(),
            visualizer_placement: VisualizerPlacement::BottomOfCover,
            progress_bar_placement: ProgressBarPlacement::BottomOfCover,
            track_info_placement: TrackInfoPlacement::MiddleRight,
        },
        VisualizerPlacementPreset {
            name: "vislzr_b_bar_b_info_br".to_string(),
            visualizer_placement: VisualizerPlacement::BottomOfCover,
            progress_bar_placement: ProgressBarPlacement::BottomOfCover,
            track_info_placement: TrackInfoPlacement::BottomRight,
        },
        VisualizerPlacementPreset {
            name: "vislzr_b_bar_br_info_tr".to_string(),
            visualizer_placement: VisualizerPlacement::BottomOfCover,
            progress_bar_placement: ProgressBarPlacement::BottomRightOfCover,
            track_info_placement: TrackInfoPlacement::TopRight,
        },
        VisualizerPlacementPreset {
            name: "vislzr_b_bar_br_info_mr".to_string(),
            visualizer_placement: VisualizerPlacement::BottomOfCover,
            progress_bar_placement: ProgressBarPlacement::BottomRightOfCover,
            track_info_placement: TrackInfoPlacement::MiddleRight,
        },
        VisualizerPlacementPreset {
            name: "vislzr_b_bar_br_info_br".to_string(),
            visualizer_placement: VisualizerPlacement::BottomOfCover,
            progress_bar_placement: ProgressBarPlacement::BottomRightOfCover,
            track_info_placement: TrackInfoPlacement::BottomRight,
        },
    ]
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VisualizerConfigFile {
    #[serde(default)]
    pub active: ActiveVisualizerSelection,
    #[serde(default)]
    pub behavior_presets: Vec<VisualizerBehaviorPreset>,
    #[serde(default)]
    pub look_presets: Vec<VisualizerLookPreset>,
    #[serde(
        skip_serializing,
        skip_deserializing,
        default = "default_visualizer_placement_presets"
    )]
    pub placement_presets: Vec<VisualizerPlacementPreset>,
    #[serde(skip)]
    pub loaded_from_file: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ActiveVisualizerSelection {
    pub behavior_preset: String,
    pub look_preset: String,
    pub placement_preset: String,
    pub visualizer_mode: VisualizerMode,
    pub visualizer_direction: VisualizerDirection,
    #[serde(default)]
    pub visualizer_peak_reach: VisualizerPeakReach,
    #[serde(default)]
    pub visualizer_peak_spread: VisualizerPeakSpread,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VisualizerBehaviorPreset {
    pub name: String,
    pub visualizer_frequency_bias: f32,
    pub visualizer_sensitivity: f32,
    pub visualizer_rise_speed: f32,
    pub visualizer_fall_speed: f32,
    pub visualizer_low_min_max_length: VisualizerLengthRange,
    pub visualizer_mid_min_max_length: VisualizerLengthRange,
    pub visualizer_high_min_max_length: VisualizerLengthRange,
    pub visualizer_peak_min_max_length: VisualizerLengthRange,
    pub visualizer_mid_threshold: f32,
    pub visualizer_high_threshold: f32,
    pub visualizer_peak_threshold: f32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VisualizerLookPreset {
    pub name: String,
    #[serde(alias = "visualizer_bar_width")]
    pub visualizer_bar_thickness: u16,
    pub visualizer_bar_gap: u16,
    pub visualizer_center_offset: i16,
    pub visualizer_border: bool,
    pub visualizer_show_on_pause: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VisualizerPlacementPreset {
    pub name: String,
    pub visualizer_placement: VisualizerPlacement,
    pub progress_bar_placement: ProgressBarPlacement,
    #[serde(default)]
    pub track_info_placement: TrackInfoPlacement,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VisualizerPreset {
    pub behavior_preset: String,
    pub look_preset: String,
    pub placement_preset: String,
    pub visualizer_mode: VisualizerMode,
    pub visualizer_direction: VisualizerDirection,
    #[serde(default)]
    pub visualizer_peak_reach: VisualizerPeakReach,
    #[serde(default)]
    pub visualizer_peak_spread: VisualizerPeakSpread,
    pub visualizer_frequency_bias: f32,
    pub visualizer_sensitivity: f32,
    pub visualizer_rise_speed: f32,
    pub visualizer_fall_speed: f32,
    pub visualizer_bar_thickness: u16,
    pub visualizer_bar_gap: u16,
    pub visualizer_center_offset: i16,
    pub visualizer_low_min_max_length: VisualizerLengthRange,
    pub visualizer_mid_min_max_length: VisualizerLengthRange,
    pub visualizer_high_min_max_length: VisualizerLengthRange,
    pub visualizer_peak_min_max_length: VisualizerLengthRange,
    pub visualizer_mid_threshold: f32,
    pub visualizer_high_threshold: f32,
    pub visualizer_peak_threshold: f32,
    pub visualizer_placement: VisualizerPlacement,
    pub visualizer_border: bool,
    pub visualizer_show_on_pause: bool,
    pub progress_bar_placement: ProgressBarPlacement,
    pub track_info_placement: TrackInfoPlacement,
}

impl VisualizerConfigFile {
    pub fn new(path: &std::path::Path) -> Result<Self> {
        let file_path = path.join(super::VISUALIZER_CONFIG_FILE);
        if !file_path.exists() {
            std::fs::write(&file_path, DEFAULT_VISUALIZER_CONFIG_TOML)?;
        }

        let mut config = Self::default();
        config.parse_config_file(path)?;
        Ok(config)
    }

    pub fn find_behavior_preset(&self, name: &str) -> Option<VisualizerBehaviorPreset> {
        self.behavior_presets
            .iter()
            .find(|preset| preset.name == name)
            .cloned()
    }

    pub fn find_look_preset(&self, name: &str) -> Option<VisualizerLookPreset> {
        self.look_presets
            .iter()
            .find(|preset| preset.name == name)
            .cloned()
    }

    pub fn find_placement_preset(&self, name: &str) -> Option<VisualizerPlacementPreset> {
        self.placement_presets
            .iter()
            .find(|preset| preset.name == name)
            .cloned()
    }

    pub fn active_visualizer_preset(&self) -> VisualizerPreset {
        let behavior = self
            .find_behavior_preset(&self.active.behavior_preset)
            .or_else(|| self.behavior_presets.first().cloned())
            .unwrap_or_default();
        let look = self
            .find_look_preset(&self.active.look_preset)
            .or_else(|| self.look_presets.first().cloned())
            .unwrap_or_default();
        let placement = self
            .find_placement_preset(&self.active.placement_preset)
            .or_else(|| self.placement_presets.first().cloned())
            .unwrap_or_default();

        VisualizerPreset {
            behavior_preset: behavior.name.clone(),
            look_preset: look.name.clone(),
            placement_preset: placement.name.clone(),
            visualizer_mode: self.active.visualizer_mode,
            visualizer_direction: self.active.visualizer_direction,
            visualizer_peak_reach: self.active.visualizer_peak_reach,
            visualizer_peak_spread: self.active.visualizer_peak_spread,
            visualizer_frequency_bias: behavior.visualizer_frequency_bias,
            visualizer_sensitivity: behavior.visualizer_sensitivity,
            visualizer_rise_speed: behavior.visualizer_rise_speed,
            visualizer_fall_speed: behavior.visualizer_fall_speed,
            visualizer_bar_thickness: look.visualizer_bar_thickness,
            visualizer_bar_gap: look.visualizer_bar_gap,
            visualizer_center_offset: look.visualizer_center_offset,
            visualizer_low_min_max_length: behavior.visualizer_low_min_max_length,
            visualizer_mid_min_max_length: behavior.visualizer_mid_min_max_length,
            visualizer_high_min_max_length: behavior.visualizer_high_min_max_length,
            visualizer_peak_min_max_length: behavior.visualizer_peak_min_max_length,
            visualizer_mid_threshold: behavior.visualizer_mid_threshold,
            visualizer_high_threshold: behavior.visualizer_high_threshold,
            visualizer_peak_threshold: behavior.visualizer_peak_threshold,
            visualizer_placement: placement.visualizer_placement,
            visualizer_border: look.visualizer_border,
            visualizer_show_on_pause: look.visualizer_show_on_pause,
            progress_bar_placement: placement.progress_bar_placement,
            track_info_placement: placement.track_info_placement,
        }
    }

    fn parse_config_file(&mut self, path: &std::path::Path) -> Result<()> {
        let file_path = path.join(super::VISUALIZER_CONFIG_FILE);
        match std::fs::read_to_string(&file_path) {
            Err(err) => {
                tracing::warn!(
                    "Failed to open the visualizer config file (path={file_path:?}): {err:#}. Use the default visualizer config instead",
                );
            }
            Ok(content) => {
                let mut config = toml::from_str::<Self>(&content)?;
                config.loaded_from_file = true;
                *self = config;
            }
        }
        Ok(())
    }
}

impl VisualizerPreset {
    pub fn from_app_config(app_config: &AppConfig) -> Self {
        Self {
            behavior_preset: "legacy".to_string(),
            look_preset: "legacy".to_string(),
            placement_preset: "legacy".to_string(),
            visualizer_mode: app_config.visualizer_mode,
            visualizer_direction: app_config.visualizer_direction,
            visualizer_peak_reach: VisualizerPeakReach::Medium,
            visualizer_peak_spread: VisualizerPeakSpread::Regular,
            visualizer_frequency_bias: app_config.visualizer_frequency_bias,
            visualizer_sensitivity: app_config.visualizer_sensitivity,
            visualizer_rise_speed: app_config.visualizer_rise_speed,
            visualizer_fall_speed: app_config.visualizer_fall_speed,
            visualizer_bar_thickness: app_config.visualizer_bar_thickness,
            visualizer_bar_gap: app_config.visualizer_bar_gap,
            visualizer_center_offset: app_config.visualizer_center_offset,
            visualizer_low_min_max_length: app_config.visualizer_low_min_max_length,
            visualizer_mid_min_max_length: app_config.visualizer_mid_min_max_length,
            visualizer_high_min_max_length: app_config.visualizer_high_min_max_length,
            visualizer_peak_min_max_length: app_config.visualizer_peak_min_max_length,
            visualizer_mid_threshold: app_config.visualizer_mid_threshold,
            visualizer_high_threshold: app_config.visualizer_high_threshold,
            visualizer_peak_threshold: app_config.visualizer_peak_threshold,
            visualizer_placement: app_config.visualizer_placement,
            visualizer_border: app_config.visualizer_border,
            visualizer_show_on_pause: app_config.visualizer_show_on_pause,
            progress_bar_placement: app_config.progress_bar_placement,
            track_info_placement: TrackInfoPlacement::TopRight,
        }
    }
}

impl Default for VisualizerConfigFile {
    fn default() -> Self {
        let mut config: Self = toml::from_str(DEFAULT_VISUALIZER_CONFIG_TOML)
            .expect("valid built-in visualizer config");
        config.loaded_from_file = false;
        config
    }
}

impl Default for ActiveVisualizerSelection {
    fn default() -> Self {
        Self {
            behavior_preset: "balanced".to_string(),
            look_preset: "boxed".to_string(),
            placement_preset: "vislzr_b_bar_b_info_tr".to_string(),
            visualizer_mode: VisualizerMode::LeftBass,
            visualizer_direction: VisualizerDirection::Up,
            visualizer_peak_reach: VisualizerPeakReach::Medium,
            visualizer_peak_spread: VisualizerPeakSpread::Regular,
        }
    }
}

impl Default for VisualizerBehaviorPreset {
    fn default() -> Self {
        Self {
            name: "balanced".to_string(),
            visualizer_frequency_bias: 1.0,
            visualizer_sensitivity: 1.0,
            visualizer_rise_speed: 1.0,
            visualizer_fall_speed: 1.0,
            visualizer_low_min_max_length: VisualizerLengthRange::new(0.0, 1.0 / 3.0),
            visualizer_mid_min_max_length: VisualizerLengthRange::new(1.0 / 3.0, 2.0 / 3.0),
            visualizer_high_min_max_length: VisualizerLengthRange::new(2.0 / 3.0, 0.85),
            visualizer_peak_min_max_length: VisualizerLengthRange::new(0.85, 1.0),
            visualizer_mid_threshold: 1.0 / 3.0,
            visualizer_high_threshold: 2.0 / 3.0,
            visualizer_peak_threshold: 0.85,
        }
    }
}

impl Default for VisualizerLookPreset {
    fn default() -> Self {
        Self {
            name: "boxed".to_string(),
            visualizer_bar_thickness: 1,
            visualizer_bar_gap: 0,
            visualizer_center_offset: 0,
            visualizer_border: true,
            visualizer_show_on_pause: true,
        }
    }
}

impl Default for VisualizerPlacementPreset {
    fn default() -> Self {
        Self {
            name: "vislzr_b_bar_b_info_tr".to_string(),
            visualizer_placement: VisualizerPlacement::BottomOfCover,
            progress_bar_placement: ProgressBarPlacement::BottomOfCover,
            track_info_placement: TrackInfoPlacement::TopRight,
        }
    }
}

impl Default for VisualizerPreset {
    fn default() -> Self {
        VisualizerConfigFile::default().active_visualizer_preset()
    }
}
