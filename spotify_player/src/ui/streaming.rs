use crate::{
    config::{self, Theme, VisualizerDirection, VisualizerLengthRange, VisualizerMode},
    state::SharedState,
};
use librespot_playback::{
    audio_backend::{Sink, SinkResult},
    convert::Converter,
    decoder::AudioPacket,
};
use parking_lot::Mutex;
use ratatui::{
    layout::Rect,
    style::Color,
    symbols::{bar, block},
    Frame,
};
use rustfft::{num_complex::Complex, FftPlanner};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

const FFT_SIZE: usize = 1024;
const HOP_SIZE: usize = 128;
pub const NUM_BANDS: usize = 128;

const DECAY_FACTOR: f32 = 0.985;
const DECAY_FACTOR_PEAK: f32 = 0.9985;
const SAMPLE_RATE: f32 = 44_100.0;
const SMOOTHING_BASE_RATE: f32 = 32.0;

pub struct VisBands {
    pub values: [f32; NUM_BANDS],
    pub updated_at: Instant,
    pub peak_envelope: f32,
    pub display_values: [f32; NUM_BANDS],
    pub display_peak_envelope: f32,
    pub rendered_at: Instant,
    pub is_active: bool,
}

#[derive(Clone, Copy)]
struct RenderedBar {
    length: f32,
    intensity: f32,
}

impl VisBands {
    pub fn new() -> Self {
        Self {
            values: [0.0f32; NUM_BANDS],
            updated_at: Instant::now(),
            peak_envelope: 1e-6,
            display_values: [0.0f32; NUM_BANDS],
            display_peak_envelope: 1e-6,
            rendered_at: Instant::now(),
            is_active: false,
        }
    }
}

impl Default for VisBands {
    fn default() -> Self {
        Self::new()
    }
}

pub fn decay_for_elapsed(elapsed: std::time::Duration) -> f32 {
    let elapsed_hops = elapsed.as_secs_f32() * SAMPLE_RATE / HOP_SIZE as f32;
    DECAY_FACTOR.powf(elapsed_hops)
}

pub fn peak_decay_for_elapsed(elapsed: std::time::Duration) -> f32 {
    let elapsed_hops = elapsed.as_secs_f32() * SAMPLE_RATE / HOP_SIZE as f32;
    DECAY_FACTOR_PEAK.powf(elapsed_hops)
}

fn smoothing_factor(speed: f32, elapsed: std::time::Duration) -> f32 {
    let speed = speed.clamp(0.0, 1.0);
    if speed <= 0.0 {
        0.0
    } else if speed >= 1.0 {
        1.0
    } else {
        1.0 - (-speed * elapsed.as_secs_f32() * SMOOTHING_BASE_RATE).exp()
    }
}

pub struct VisualizationSink {
    inner: Box<dyn Sink>,
    sample_buf: VecDeque<f32>,
    bands: Arc<Mutex<VisBands>>,
    fft: Arc<dyn rustfft::Fft<f32>>,
    hann_window: Vec<f32>,
    fft_buf: Vec<Complex<f32>>,
    magnitudes: Vec<f32>,
    sample_rate: f32,
    band_ranges: Vec<(usize, usize)>,
    new_bands: [f32; NUM_BANDS],
    smooth_scratch: [f32; NUM_BANDS],
}

impl VisualizationSink {
    pub fn new(inner: Box<dyn Sink>, bands: Arc<Mutex<VisBands>>, sample_rate: f32) -> Self {
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);
        let hann_window: Vec<f32> = (0..FFT_SIZE)
            .map(|i| {
                0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (FFT_SIZE - 1) as f32).cos())
            })
            .collect();
        let band_ranges = precompute_band_ranges(FFT_SIZE / 2, NUM_BANDS);
        Self {
            inner,
            sample_buf: VecDeque::with_capacity(FFT_SIZE * 2),
            bands,
            fft,
            hann_window,
            fft_buf: vec![Complex::new(0.0, 0.0); FFT_SIZE],
            magnitudes: vec![0.0; FFT_SIZE / 2],
            sample_rate,
            band_ranges,
            new_bands: [0.0f32; NUM_BANDS],
            smooth_scratch: [0.0f32; NUM_BANDS],
        }
    }
}

impl Sink for VisualizationSink {
    fn start(&mut self) -> SinkResult<()> {
        self.inner.start()
    }

    fn stop(&mut self) -> SinkResult<()> {
        let mut g = self.bands.lock();
        g.values.fill(0.0);
        g.peak_envelope = 1e-6;
        g.display_values.fill(0.0);
        g.display_peak_envelope = 1e-6;
        g.updated_at = Instant::now();
        g.rendered_at = Instant::now();
        g.is_active = false;
        drop(g);
        self.sample_buf.clear();
        self.inner.stop()
    }

    fn write(&mut self, packet: AudioPacket, converter: &mut Converter) -> SinkResult<()> {
        if let AudioPacket::Samples(ref samples) = packet {
            self.sample_buf.extend(samples.chunks(2).map(|c| {
                if c.len() == 2 {
                    f64::midpoint(c[0], c[1]) as f32
                } else {
                    c[0] as f32
                }
            }));

            while self.sample_buf.len() >= FFT_SIZE {
                {
                    let (front, back) = self.sample_buf.as_slices();
                    if front.len() >= FFT_SIZE {
                        for (dst, (&s, &w)) in self
                            .fft_buf
                            .iter_mut()
                            .zip(front.iter().zip(self.hann_window.iter()))
                        {
                            *dst = Complex::new(s * w, 0.0);
                        }
                    } else {
                        let split = front.len();
                        for (dst, (&s, &w)) in self.fft_buf[..split]
                            .iter_mut()
                            .zip(front.iter().zip(self.hann_window[..split].iter()))
                        {
                            *dst = Complex::new(s * w, 0.0);
                        }
                        let remaining = FFT_SIZE - split;
                        for (dst, (&s, &w)) in self.fft_buf[split..].iter_mut().zip(
                            back[..remaining]
                                .iter()
                                .zip(self.hann_window[split..].iter()),
                        ) {
                            *dst = Complex::new(s * w, 0.0);
                        }
                    }
                }

                self.fft.process(&mut self.fft_buf);

                for (mag, c) in self.magnitudes.iter_mut().zip(self.fft_buf.iter()) {
                    *mag = c.norm();
                }

                fill_log_bands(&self.magnitudes, &self.band_ranges, &mut self.new_bands);
                smooth_bands(&mut self.new_bands, &mut self.smooth_scratch);

                let mut g = self.bands.lock();
                let elapsed_hops =
                    g.updated_at.elapsed().as_secs_f32() * self.sample_rate / HOP_SIZE as f32;
                let decay = DECAY_FACTOR.powf(elapsed_hops);
                let peak_decay = DECAY_FACTOR_PEAK.powf(elapsed_hops);
                let frame_peak = self.new_bands.iter().copied().fold(0.0_f32, f32::max);
                for (stored, fresh) in g.values.iter_mut().zip(self.new_bands.iter()) {
                    *stored = (*stored * decay).max(*fresh);
                }
                g.peak_envelope = (g.peak_envelope * peak_decay).max(frame_peak);
                g.updated_at = Instant::now();
                drop(g);

                self.sample_buf.drain(..HOP_SIZE);
            }
        }

        self.inner.write(packet, converter)
    }
}

fn precompute_band_ranges(num_bins: usize, num_bands: usize) -> Vec<(usize, usize)> {
    let log_min = 1.0_f64;
    let log_max = num_bins as f64;
    let mut used_up_to: usize = 1;
    let mut ranges = Vec::with_capacity(num_bands);
    for band in 0..num_bands {
        if used_up_to >= num_bins {
            ranges.push((num_bins - 1, num_bins));
            continue;
        }
        let t_start = band as f64 / num_bands as f64;
        let t_end = (band + 1) as f64 / num_bands as f64;
        let natural_start = (log_min * (log_max / log_min).powf(t_start)) as usize;
        let natural_end = (log_min * (log_max / log_min).powf(t_end)) as usize;
        let start = natural_start.max(used_up_to).min(num_bins - 1);
        let end = natural_end.max(start + 1).min(num_bins);
        used_up_to = end;
        ranges.push((start, end));
    }
    ranges
}

fn fill_log_bands(magnitudes: &[f32], band_ranges: &[(usize, usize)], out: &mut [f32]) {
    for (band_val, &(start, end)) in out.iter_mut().zip(band_ranges.iter()) {
        let len = (end - start) as f32;
        let sum_sq: f32 = magnitudes[start..end].iter().map(|&v| v * v).sum();
        *band_val = (sum_sq / len).sqrt();
    }
}

fn smooth_bands(bands: &mut [f32], scratch: &mut [f32]) {
    let n = bands.len();
    if n < 3 {
        return;
    }
    scratch[..n].copy_from_slice(&bands[..n]);
    for i in 0..n {
        let prev = scratch[if i > 0 { i - 1 } else { 0 }];
        let next = scratch[if i + 1 < n { i + 1 } else { n - 1 }];
        bands[i] = prev * 0.25 + scratch[i] * 0.5 + next * 0.25;
    }
}

fn bar_color(
    theme: &Theme,
    t: f32,
    mid_threshold: f32,
    high_threshold: f32,
    peak_threshold: f32,
) -> Color {
    let [low, mid, high, peak] = theme.visualizer_colors();
    let t = t.clamp(0.0, 1.0);
    let (mid_threshold, high_threshold, peak_threshold) =
        sanitize_thresholds(mid_threshold, high_threshold, peak_threshold);

    if t <= mid_threshold {
        interpolate_color(low, mid, normalize_segment(t, 0.0, mid_threshold))
    } else if t <= high_threshold {
        interpolate_color(
            mid,
            high,
            normalize_segment(t, mid_threshold, high_threshold),
        )
    } else if t <= peak_threshold {
        interpolate_color(
            high,
            peak,
            normalize_segment(t, high_threshold, peak_threshold),
        )
    } else {
        peak
    }
}

pub fn render_audio_visualization(
    frame: &mut Frame,
    state: &SharedState,
    theme: &Theme,
    rect: Rect,
    preset: &config::VisualizerPreset,
) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }

    clear_visualization_area(frame, rect, theme);

    let Some(vis_lock) = state.vis_bands.as_ref() else {
        return;
    };
    let mut guard = vis_lock.lock();
    if !guard.is_active {
        return;
    }

    let direction = preset.visualizer_direction.clone();
    let mode = preset.visualizer_mode.clone();
    let bar_thickness = preset.visualizer_bar_thickness.max(1);
    let bar_gap = preset.visualizer_bar_gap;
    let center_offset = preset.visualizer_center_offset;
    let frequency_bias = preset.visualizer_frequency_bias.max(0.1);
    let sensitivity = preset.visualizer_sensitivity.max(0.0);
    let rise_speed = preset.visualizer_rise_speed.max(0.0);
    let fall_speed = preset.visualizer_fall_speed.max(0.0);
    let low_length_range = preset.visualizer_low_min_max_length;
    let mid_length_range = preset.visualizer_mid_min_max_length;
    let high_length_range = preset.visualizer_high_min_max_length;
    let peak_length_range = preset.visualizer_peak_min_max_length;
    let (mid_threshold, high_threshold, peak_threshold) = sanitize_thresholds(
        preset.visualizer_mid_threshold,
        preset.visualizer_high_threshold,
        preset.visualizer_peak_threshold,
    );
    let display_decay = decay_for_elapsed(guard.updated_at.elapsed());
    let peak_target =
        (guard.peak_envelope * peak_decay_for_elapsed(guard.updated_at.elapsed())).max(1e-6);
    let smoothing_elapsed = guard.rendered_at.elapsed();
    let rise_factor = smoothing_factor(rise_speed, smoothing_elapsed);
    let fall_factor = smoothing_factor(fall_speed, smoothing_elapsed);

    for idx in 0..NUM_BANDS {
        let target = guard.values[idx] * display_decay;
        let factor = if target >= guard.display_values[idx] {
            rise_factor
        } else {
            fall_factor
        };
        guard.display_values[idx] += (target - guard.display_values[idx]) * factor;
    }

    let peak_factor = if peak_target >= guard.display_peak_envelope {
        rise_factor
    } else {
        fall_factor
    };
    guard.display_peak_envelope += (peak_target - guard.display_peak_envelope) * peak_factor;
    guard.rendered_at = Instant::now();

    let values = guard.display_values;
    let peak_norm = guard.display_peak_envelope.max(1e-6);
    drop(guard);

    let bar_axis_len = match direction {
        VisualizerDirection::Up
        | VisualizerDirection::Down
        | VisualizerDirection::UpAndDown
        | VisualizerDirection::CenterUpAndDown => rect.width,
        VisualizerDirection::Left
        | VisualizerDirection::Right
        | VisualizerDirection::LeftAndRight
        | VisualizerDirection::CenterLeftAndRight => rect.height,
    };
    let num_bars = visible_bar_count(bar_axis_len, bar_thickness, bar_gap);
    if num_bars == 0 {
        return;
    }

    let thresholds = (mid_threshold, high_threshold, peak_threshold);
    let length_ranges = (
        low_length_range,
        mid_length_range,
        high_length_range,
        peak_length_range,
    );

    match direction {
        VisualizerDirection::Up
        | VisualizerDirection::Down
        | VisualizerDirection::Left
        | VisualizerDirection::Right => {
            let horizontal = matches!(
                direction,
                VisualizerDirection::Left | VisualizerDirection::Right
            );
            let bars = build_rendered_bars(
                &values,
                num_bars,
                mode,
                frequency_bias,
                center_offset,
                display_decay,
                peak_norm,
                sensitivity,
                thresholds,
                length_ranges,
                horizontal,
                VerticalDualLength::None,
            );
            render_single_direction(
                frame,
                direction,
                rect,
                theme,
                &bars,
                bar_thickness,
                bar_gap,
                thresholds,
            );
        }
        VisualizerDirection::LeftAndRight => {
            let (left_mode, right_mode) = dual_side_modes(mode);
            let left_bars = build_rendered_bars(
                &values,
                num_bars,
                left_mode,
                frequency_bias,
                center_offset,
                display_decay,
                peak_norm,
                sensitivity,
                thresholds,
                length_ranges,
                true,
                VerticalDualLength::Cap,
            );
            let right_bars = build_rendered_bars(
                &values,
                num_bars,
                right_mode,
                frequency_bias,
                center_offset,
                display_decay,
                peak_norm,
                sensitivity,
                thresholds,
                length_ranges,
                true,
                VerticalDualLength::Cap,
            );
            // Edge-anchored: same reach as singular Left/Right across the full width.
            render_horizontal_right(
                frame,
                rect,
                theme,
                &right_bars,
                bar_thickness,
                bar_gap,
                mid_threshold,
                high_threshold,
                peak_threshold,
            );
            render_horizontal_left(
                frame,
                rect,
                theme,
                &left_bars,
                bar_thickness,
                bar_gap,
                mid_threshold,
                high_threshold,
                peak_threshold,
            );
        }
        VisualizerDirection::CenterLeftAndRight => {
            let (left_mode, right_mode) = dual_side_modes(mode);
            let left_bars = build_rendered_bars(
                &values,
                num_bars,
                left_mode,
                frequency_bias,
                center_offset,
                display_decay,
                peak_norm,
                sensitivity,
                thresholds,
                length_ranges,
                true,
                VerticalDualLength::None,
            );
            let right_bars = build_rendered_bars(
                &values,
                num_bars,
                right_mode,
                frequency_bias,
                center_offset,
                display_decay,
                peak_norm,
                sensitivity,
                thresholds,
                length_ranges,
                true,
                VerticalDualLength::None,
            );
            let (left_rect, right_rect) = split_horizontal_center(rect);
            if left_rect.width > 0 {
                render_horizontal_left(
                    frame,
                    left_rect,
                    theme,
                    &left_bars,
                    bar_thickness,
                    bar_gap,
                    mid_threshold,
                    high_threshold,
                    peak_threshold,
                );
            }
            if right_rect.width > 0 {
                render_horizontal_right(
                    frame,
                    right_rect,
                    theme,
                    &right_bars,
                    bar_thickness,
                    bar_gap,
                    mid_threshold,
                    high_threshold,
                    peak_threshold,
                );
            }
        }
        VisualizerDirection::UpAndDown => {
            let (up_mode, down_mode) = dual_side_modes(mode);
            let up_bars = build_rendered_bars(
                &values,
                num_bars,
                up_mode,
                frequency_bias,
                center_offset,
                display_decay,
                peak_norm,
                sensitivity,
                thresholds,
                length_ranges,
                false,
                VerticalDualLength::Cap,
            );
            let down_bars = build_rendered_bars(
                &values,
                num_bars,
                down_mode,
                frequency_bias,
                center_offset,
                display_decay,
                peak_norm,
                sensitivity,
                thresholds,
                length_ranges,
                false,
                VerticalDualLength::Cap,
            );
            // Edge-anchored: Down first, then Up on top.
            render_vertical_down(
                frame,
                rect,
                theme,
                &down_bars,
                bar_thickness,
                bar_gap,
                mid_threshold,
                high_threshold,
                peak_threshold,
            );
            render_vertical_up(
                frame,
                rect,
                theme,
                &up_bars,
                bar_thickness,
                bar_gap,
                mid_threshold,
                high_threshold,
                peak_threshold,
            );
        }
        VisualizerDirection::CenterUpAndDown => {
            let (up_mode, down_mode) = dual_side_modes(mode);
            let up_bars = build_rendered_bars(
                &values,
                num_bars,
                up_mode,
                frequency_bias,
                center_offset,
                display_decay,
                peak_norm,
                sensitivity,
                thresholds,
                length_ranges,
                false,
                VerticalDualLength::Boost,
            );
            let down_bars = build_rendered_bars(
                &values,
                num_bars,
                down_mode,
                frequency_bias,
                center_offset,
                display_decay,
                peak_norm,
                sensitivity,
                thresholds,
                length_ranges,
                false,
                VerticalDualLength::Boost,
            );
            let (up_rect, down_rect) = split_vertical_center(rect);
            if up_rect.height > 0 {
                render_vertical_up(
                    frame,
                    up_rect,
                    theme,
                    &up_bars,
                    bar_thickness,
                    bar_gap,
                    mid_threshold,
                    high_threshold,
                    peak_threshold,
                );
            }
            if down_rect.height > 0 {
                render_vertical_down(
                    frame,
                    down_rect,
                    theme,
                    &down_bars,
                    bar_thickness,
                    bar_gap,
                    mid_threshold,
                    high_threshold,
                    peak_threshold,
                );
            }
        }
    }
}

fn split_horizontal_center(rect: Rect) -> (Rect, Rect) {
    let mid = rect.width / 2;
    let left = Rect {
        x: rect.x,
        y: rect.y,
        width: mid,
        height: rect.height,
    };
    let right = Rect {
        x: rect.x.saturating_add(mid),
        y: rect.y,
        width: rect.width.saturating_sub(mid),
        height: rect.height,
    };
    (left, right)
}

fn split_vertical_center(rect: Rect) -> (Rect, Rect) {
    // Up sits in the top half (grows up from the midline); Down in the bottom half.
    // On odd heights, give the extra row to Up so the shared hinge sits on true center
    // (otherwise the unit reads a row too high).
    let up_height = (rect.height + 1) / 2;
    let down_height = rect.height / 2;
    let up = Rect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: up_height,
    };
    let down = Rect {
        x: rect.x,
        y: rect.y.saturating_add(up_height),
        width: rect.width,
        height: down_height,
    };
    (up, down)
}

fn dual_side_modes(mode: VisualizerMode) -> (VisualizerMode, VisualizerMode) {
    match mode {
        VisualizerMode::FlippedLeftAndRight => {
            (VisualizerMode::LeftBass, VisualizerMode::RightBass)
        }
        VisualizerMode::FlippedRightAndLeft => {
            (VisualizerMode::RightBass, VisualizerMode::LeftBass)
        }
        other => (other, other),
    }
}

#[derive(Clone, Copy)]
enum VerticalDualLength {
    None,
    Boost,
    Cap,
}

fn build_rendered_bars(
    values: &[f32; NUM_BANDS],
    num_bars: usize,
    mode: VisualizerMode,
    frequency_bias: f32,
    center_offset: i16,
    display_decay: f32,
    peak_norm: f32,
    sensitivity: f32,
    thresholds: (f32, f32, f32),
    length_ranges: (
        VisualizerLengthRange,
        VisualizerLengthRange,
        VisualizerLengthRange,
        VisualizerLengthRange,
    ),
    horizontal: bool,
    vertical_dual_length: VerticalDualLength,
) -> Vec<RenderedBar> {
    let (mid_threshold, high_threshold, peak_threshold) = thresholds;
    let (low_length_range, mid_length_range, high_length_range, peak_length_range) = length_ranges;
    let mode = if mode.is_dual_only() {
        VisualizerMode::LeftBass
    } else {
        mode
    };

    let mut bars: Vec<RenderedBar> =
        shape_visualizer_values(values, num_bars, mode, frequency_bias, center_offset)
            .into_iter()
            .map(|value| {
                let intensity = (((value * display_decay) / peak_norm) * sensitivity)
                    .clamp(0.0, 1.0)
                    .powf(0.5);
                let mut length = normalized_length_for_intensity(
                    intensity,
                    mid_threshold,
                    high_threshold,
                    peak_threshold,
                    low_length_range,
                    mid_length_range,
                    high_length_range,
                    peak_length_range,
                );
                if horizontal {
                    length = soften_horizontal_bar_length(length);
                    if matches!(vertical_dual_length, VerticalDualLength::Cap) {
                        length = cap_edge_meet_bar_length(length);
                    }
                } else {
                    match vertical_dual_length {
                        VerticalDualLength::None => {}
                        VerticalDualLength::Boost => {
                            length = boost_vertical_dual_bar_length(length);
                        }
                        VerticalDualLength::Cap => {
                            length = cap_edge_meet_bar_length(length);
                        }
                    }
                }

                RenderedBar { length, intensity }
            })
            .collect();

    if horizontal {
        soften_horizontal_bar_slope(&mut bars);
    }

    bars
}

fn render_single_direction(
    frame: &mut Frame,
    direction: VisualizerDirection,
    rect: Rect,
    theme: &Theme,
    bars: &[RenderedBar],
    bar_thickness: u16,
    bar_gap: u16,
    thresholds: (f32, f32, f32),
) {
    let (mid_threshold, high_threshold, peak_threshold) = thresholds;
    match direction {
        VisualizerDirection::Up => render_vertical_up(
            frame,
            rect,
            theme,
            bars,
            bar_thickness,
            bar_gap,
            mid_threshold,
            high_threshold,
            peak_threshold,
        ),
        VisualizerDirection::Down => render_vertical_down(
            frame,
            rect,
            theme,
            bars,
            bar_thickness,
            bar_gap,
            mid_threshold,
            high_threshold,
            peak_threshold,
        ),
        VisualizerDirection::Left => render_horizontal_left(
            frame,
            rect,
            theme,
            bars,
            bar_thickness,
            bar_gap,
            mid_threshold,
            high_threshold,
            peak_threshold,
        ),
        VisualizerDirection::Right => render_horizontal_right(
            frame,
            rect,
            theme,
            bars,
            bar_thickness,
            bar_gap,
            mid_threshold,
            high_threshold,
            peak_threshold,
        ),
        VisualizerDirection::LeftAndRight
        | VisualizerDirection::CenterLeftAndRight
        | VisualizerDirection::UpAndDown
        | VisualizerDirection::CenterUpAndDown => {}
    }
}

fn shape_visualizer_values(
    values: &[f32; NUM_BANDS],
    num_bars: usize,
    mode: VisualizerMode,
    frequency_bias: f32,
    center_offset: i16,
) -> Vec<f32> {
    let base = sample_visualizer_values(values, num_bars, frequency_bias);

    match mode {
        VisualizerMode::LeftBass
        | VisualizerMode::FlippedLeftAndRight
        | VisualizerMode::FlippedRightAndLeft => base,
        VisualizerMode::RightBass => reverse_values(base),
        VisualizerMode::CenterBass => center_out_map(base, center_offset),
        VisualizerMode::CenterMirrored => center_mirrored_map(&base, center_offset),
        VisualizerMode::MultiPeakLcr => multi_peak_lcr_map(&base, center_offset),
        VisualizerMode::MultiPeakLr => multi_peak_lr_map(&base),
    }
}

fn sample_visualizer_values(values: &[f32; NUM_BANDS], num_bars: usize, emphasis: f32) -> Vec<f32> {
    if num_bars <= 1 {
        return vec![values[0]];
    }

    (0..num_bars)
        .map(|i| {
            let t = i as f32 / (num_bars - 1) as f32;
            sample_at(values, t.powf(emphasis))
        })
        .collect()
}

fn center_out_map(values: Vec<f32>, center_offset: i16) -> Vec<f32> {
    let mut output = vec![0.0; values.len()];
    let center = shifted_center(values.len(), center_offset);
    let mut index_order = (0..values.len()).collect::<Vec<_>>();
    index_order.sort_by(|left, right| {
        let left_dist = center_distance(*left, center);
        let right_dist = center_distance(*right, center);
        left_dist
            .partial_cmp(&right_dist)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.cmp(right))
    });

    for (value, target_idx) in values.into_iter().zip(index_order.into_iter()) {
        if target_idx < output.len() {
            output[target_idx] = value;
        }
    }

    output
}

fn reverse_values(mut values: Vec<f32>) -> Vec<f32> {
    values.reverse();
    values
}

fn center_mirrored_map(values: &[f32], center_offset: i16) -> Vec<f32> {
    let len = values.len();
    let center = shifted_center(len, center_offset);
    let max_index = values.len().saturating_sub(1) as f32;

    (0..len)
        .map(|i| {
            let dist = center_distance(i, center).min(max_index);
            sample_vec_at(values, dist)
        })
        .collect()
}

fn multi_peak_lr_map(values: &[f32]) -> Vec<f32> {
    let len = values.len();
    (0..len)
        .map(|i| {
            let idx = i.min(len.saturating_sub(1).saturating_sub(i));
            values[idx.min(values.len().saturating_sub(1))]
        })
        .collect()
}

fn multi_peak_lcr_map(values: &[f32], center_offset: i16) -> Vec<f32> {
    let len = values.len();
    let center = shifted_center(len, center_offset);
    let max_index = values.len().saturating_sub(1) as f32;

    (0..len)
        .map(|i| {
            let edge_dist = i.min(len.saturating_sub(1).saturating_sub(i)) as f32;
            let center_dist = center_distance(i, center);
            sample_vec_at(values, edge_dist.min(center_dist).min(max_index))
        })
        .collect()
}

fn shifted_center(len: usize, center_offset: i16) -> f32 {
    if len <= 1 {
        return 0.0;
    }

    // True midpoint across the visible bars (odd → one center bar, even → shared by two).
    let base_center = (len.saturating_sub(1) as f32) / 2.0;
    (base_center + center_offset as f32).clamp(0.0, len.saturating_sub(1) as f32)
}

fn center_distance(index: usize, center: f32) -> f32 {
    (index as f32 - center).abs()
}

fn sample_vec_at(values: &[f32], idx: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }

    let last = values.len().saturating_sub(1) as f32;
    let pos = idx.clamp(0.0, last);
    let lower = pos.floor() as usize;
    let upper = pos.ceil() as usize;

    if lower == upper {
        values[lower]
    } else {
        let mix = pos - lower as f32;
        values[lower] * (1.0 - mix) + values[upper] * mix
    }
}

fn sample_at(values: &[f32; NUM_BANDS], t: f32) -> f32 {
    let last = (values.len() - 1) as f32;
    let pos = t.clamp(0.0, 1.0) * last;
    let lower = pos.floor() as usize;
    let upper = pos.ceil() as usize;

    if lower == upper {
        values[lower]
    } else {
        let mix = pos - lower as f32;
        values[lower] * (1.0 - mix) + values[upper] * mix
    }
}

fn render_vertical_up(
    frame: &mut Frame,
    rect: Rect,
    theme: &Theme,
    bars: &[RenderedBar],
    bar_thickness: u16,
    bar_gap: u16,
    mid_threshold: f32,
    high_threshold: f32,
    peak_threshold: f32,
) {
    let bg = background_color(theme);
    let max_ticks = u32::from(rect.height) * 8;
    let mut cursor_x = centered_axis_start(rect.x, rect.width, bars.len(), bar_thickness, bar_gap);

    for bar in bars.iter().copied() {
        if cursor_x >= rect.right() {
            break;
        }

        let ticks = normalized_ticks(bar.length, max_ticks);
        let color = bar_color(
            theme,
            bar.intensity,
            mid_threshold,
            high_threshold,
            peak_threshold,
        );
        let mut remaining = ticks;

        for row_offset in (0..rect.height).rev() {
            let fill = remaining.min(8);
            if fill > 0 {
                let symbol = bottom_bar_symbol(fill);
                paint_column_cell(
                    frame,
                    cursor_x,
                    row_offset + rect.y,
                    bar_thickness,
                    symbol,
                    color,
                    bg,
                );
            }
            remaining = remaining.saturating_sub(8);
        }

        cursor_x = cursor_x.saturating_add(bar_thickness + bar_gap);
    }
}

fn render_vertical_down(
    frame: &mut Frame,
    rect: Rect,
    theme: &Theme,
    bars: &[RenderedBar],
    bar_thickness: u16,
    bar_gap: u16,
    mid_threshold: f32,
    high_threshold: f32,
    peak_threshold: f32,
) {
    let bg = background_color(theme);
    let max_ticks = u32::from(rect.height) * 8;
    let mut cursor_x = centered_axis_start(rect.x, rect.width, bars.len(), bar_thickness, bar_gap);

    for bar in bars.iter().copied() {
        if cursor_x >= rect.right() {
            break;
        }

        let ticks = normalized_ticks(bar.length, max_ticks);
        let color = bar_color(
            theme,
            bar.intensity,
            mid_threshold,
            high_threshold,
            peak_threshold,
        );
        let full_rows = ticks / 8;
        let partial_ticks = ticks % 8;

        for row_offset in 0..rect.height {
            let y = rect.y + row_offset;
            let symbol = if row_offset < full_rows {
                Some((bar::FULL, color, bg))
            } else if row_offset == full_rows && partial_ticks > 0 {
                Some(top_bar_cell(partial_ticks, color, bg))
            } else {
                None
            };

            if let Some((glyph, fg, bg)) = symbol {
                paint_column_cell(frame, cursor_x, y, bar_thickness, glyph, fg, bg);
            }
        }

        cursor_x = cursor_x.saturating_add(bar_thickness + bar_gap);
    }
}

fn render_horizontal_right(
    frame: &mut Frame,
    rect: Rect,
    theme: &Theme,
    bars: &[RenderedBar],
    bar_thickness: u16,
    bar_gap: u16,
    mid_threshold: f32,
    high_threshold: f32,
    peak_threshold: f32,
) {
    let bg = background_color(theme);
    let max_ticks = u32::from(rect.width) * 8;
    let mut cursor_y = centered_axis_start(rect.y, rect.height, bars.len(), bar_thickness, bar_gap);

    for bar in bars.iter().copied() {
        if cursor_y >= rect.bottom() {
            break;
        }

        let ticks = normalized_ticks(bar.length, max_ticks);
        let color = bar_color(
            theme,
            bar.intensity,
            mid_threshold,
            high_threshold,
            peak_threshold,
        );
        let mut remaining = ticks;

        for col_offset in 0..rect.width {
            let fill = remaining.min(8);
            if fill > 0 {
                let symbol = left_bar_symbol(fill);
                paint_row_cell(
                    frame,
                    rect.x + col_offset,
                    cursor_y,
                    bar_thickness,
                    symbol,
                    color,
                    bg,
                );
            }
            remaining = remaining.saturating_sub(8);
        }

        cursor_y = cursor_y.saturating_add(bar_thickness + bar_gap);
    }
}

fn render_horizontal_left(
    frame: &mut Frame,
    rect: Rect,
    theme: &Theme,
    bars: &[RenderedBar],
    bar_thickness: u16,
    bar_gap: u16,
    mid_threshold: f32,
    high_threshold: f32,
    peak_threshold: f32,
) {
    let bg = background_color(theme);
    let max_ticks = u32::from(rect.width) * 8;
    let mut cursor_y = centered_axis_start(rect.y, rect.height, bars.len(), bar_thickness, bar_gap);

    for bar in bars.iter().copied() {
        if cursor_y >= rect.bottom() {
            break;
        }

        let ticks = normalized_ticks(bar.length, max_ticks);
        let color = bar_color(
            theme,
            bar.intensity,
            mid_threshold,
            high_threshold,
            peak_threshold,
        );
        let full_cols = ticks / 8;
        let partial_ticks = ticks % 8;

        for col_offset in 0..rect.width {
            let x = rect.right().saturating_sub(1 + col_offset);
            let symbol = if col_offset < full_cols {
                Some((block::FULL, color, bg))
            } else if col_offset == full_cols && partial_ticks > 0 {
                left_tip_cell(partial_ticks, color, bg)
            } else {
                None
            };

            if let Some((glyph, fg, cell_bg)) = symbol {
                paint_row_cell(frame, x, cursor_y, bar_thickness, glyph, fg, cell_bg);
            }
        }

        cursor_y = cursor_y.saturating_add(bar_thickness + bar_gap);
    }
}

fn visible_bar_count(axis_len: u16, bar_thickness: u16, bar_gap: u16) -> usize {
    if axis_len == 0 {
        return 0;
    }

    let bar_thickness = bar_thickness.max(1);
    usize::from((axis_len.saturating_add(bar_gap) / (bar_thickness + bar_gap)).max(1))
}

fn centered_axis_start(
    axis_origin: u16,
    axis_len: u16,
    num_bars: usize,
    bar_thickness: u16,
    bar_gap: u16,
) -> u16 {
    let used = used_axis_span(num_bars, bar_thickness, bar_gap);
    axis_origin.saturating_add(axis_len.saturating_sub(used) / 2)
}

fn used_axis_span(num_bars: usize, bar_thickness: u16, bar_gap: u16) -> u16 {
    if num_bars == 0 {
        return 0;
    }

    let bar_thickness = u32::from(bar_thickness.max(1));
    let bar_gap = u32::from(bar_gap);
    let bars = num_bars as u32;
    (bar_thickness * bars + bar_gap * bars.saturating_sub(1)).min(u32::from(u16::MAX)) as u16
}

fn normalized_ticks(norm: f32, max_ticks: u32) -> u16 {
    (norm.clamp(0.0, 1.0) * max_ticks as f32).round() as u16
}

// Left/Right use few bars (one per row), so full preset peak lengths look oversized.
// Compress only the upper length range; Up/Down never call this.
fn soften_horizontal_bar_length(length: f32) -> f32 {
    let length = length.clamp(0.0, 1.0);
    const KEEP: f32 = 0.45;
    const COMPRESS: f32 = 0.55;
    if length <= KEEP {
        length
    } else {
        KEEP + (length - KEEP) * COMPRESS
    }
}

// Dual vertical center modes share half-height; ease mid lengths up so bars use more of the available travel.
fn boost_vertical_dual_bar_length(length: f32) -> f32 {
    length.clamp(0.0, 1.0).powf(0.72)
}

// Edge duals that grow toward each other; keep quieter bars, cap peaks so they don't cross.
fn cap_edge_meet_bar_length(length: f32) -> f32 {
    let length = length.clamp(0.0, 1.0);
    const KEEP: f32 = 0.22;
    const MAX: f32 = 0.47;
    if length <= KEEP {
        length
    } else {
        KEEP + (length - KEEP) / (1.0 - KEEP) * (MAX - KEEP)
    }
}

// Blend neighboring Left/Right lengths so peaks fall off as a slope, not a cliff.
fn soften_horizontal_bar_slope(bars: &mut [RenderedBar]) {
    if bars.len() < 2 {
        return;
    }

    for _ in 0..2 {
        let lengths: Vec<f32> = bars.iter().map(|bar| bar.length).collect();
        for (i, bar) in bars.iter_mut().enumerate() {
            let mut sum = lengths[i] * 2.0;
            let mut weight = 2.0;
            if i > 0 {
                sum += lengths[i - 1];
                weight += 1.0;
            }
            if i + 1 < lengths.len() {
                sum += lengths[i + 1];
                weight += 1.0;
            }
            bar.length = sum / weight;
        }
    }
}

fn sanitize_thresholds(mid: f32, high: f32, peak: f32) -> (f32, f32, f32) {
    let mid = mid.clamp(0.0, 0.98);
    let high = high.max(mid + 0.01).clamp(mid + 0.01, 0.99);
    let peak = peak.max(high + 0.01).clamp(high + 0.01, 1.0);
    (mid, high, peak)
}

fn normalized_length_for_intensity(
    intensity: f32,
    mid_threshold: f32,
    high_threshold: f32,
    peak_threshold: f32,
    low_length_range: VisualizerLengthRange,
    mid_length_range: VisualizerLengthRange,
    high_length_range: VisualizerLengthRange,
    peak_length_range: VisualizerLengthRange,
) -> f32 {
    let intensity = intensity.clamp(0.0, 1.0);
    let (range, start, end) = if intensity <= mid_threshold {
        (sanitize_length_range(low_length_range), 0.0, mid_threshold)
    } else if intensity <= high_threshold {
        (
            sanitize_length_range(mid_length_range),
            mid_threshold,
            high_threshold,
        )
    } else if intensity <= peak_threshold {
        (
            sanitize_length_range(high_length_range),
            high_threshold,
            peak_threshold,
        )
    } else {
        (
            sanitize_length_range(peak_length_range),
            peak_threshold,
            1.0,
        )
    };

    interpolate_length_range(range, normalize_segment(intensity, start, end))
}

fn sanitize_length_range(range: VisualizerLengthRange) -> VisualizerLengthRange {
    let min = range.0.clamp(0.0, 1.0);
    let max = range.1.clamp(0.0, 1.0);

    if min <= max {
        VisualizerLengthRange(min, max)
    } else {
        VisualizerLengthRange(max, min)
    }
}

fn interpolate_length_range(range: VisualizerLengthRange, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    range.0 + (range.1 - range.0) * t
}

fn normalize_segment(value: f32, start: f32, end: f32) -> f32 {
    if end <= start {
        1.0
    } else {
        ((value - start) / (end - start)).clamp(0.0, 1.0)
    }
}

fn bottom_bar_symbol(ticks: u16) -> &'static str {
    match ticks {
        0 => " ",
        1 => bar::ONE_EIGHTH,
        2 => bar::ONE_QUARTER,
        3 => bar::THREE_EIGHTHS,
        4 => bar::HALF,
        5 => bar::FIVE_EIGHTHS,
        6 => bar::THREE_QUARTERS,
        7 => bar::SEVEN_EIGHTHS,
        _ => bar::FULL,
    }
}

fn left_bar_symbol(ticks: u16) -> &'static str {
    match ticks {
        0 => " ",
        1 => block::ONE_EIGHTH,
        2 => block::ONE_QUARTER,
        3 => block::THREE_EIGHTHS,
        4 => block::HALF,
        5 => block::FIVE_EIGHTHS,
        6 => block::THREE_QUARTERS,
        7 => block::SEVEN_EIGHTHS,
        _ => block::FULL,
    }
}

fn left_tip_cell(
    fill_ticks: u16,
    color: Color,
    background: Color,
) -> Option<(&'static str, Color, Color)> {
    // Color-swap tips break on Color::Reset (transparent themes). Use a normal
    // right-half block past halfway, otherwise skip the fraction.
    if matches!(background, Color::Reset) {
        if fill_ticks >= 4 {
            Some(("\u{2590}", color, background)) // ▐
        } else {
            None
        }
    } else {
        Some(right_bar_cell(fill_ticks, color, background))
    }
}

fn top_bar_cell(fill_ticks: u16, color: Color, background: Color) -> (&'static str, Color, Color) {
    match fill_ticks {
        1 => (bar::SEVEN_EIGHTHS, background, color),
        2 => (bar::THREE_QUARTERS, background, color),
        3 => (bar::FIVE_EIGHTHS, background, color),
        4 => (bar::HALF, background, color),
        5 => (bar::THREE_EIGHTHS, background, color),
        6 => (bar::ONE_QUARTER, background, color),
        7 => (bar::ONE_EIGHTH, background, color),
        _ => (bar::FULL, color, background),
    }
}

fn right_bar_cell(
    fill_ticks: u16,
    color: Color,
    background: Color,
) -> (&'static str, Color, Color) {
    match fill_ticks {
        1 => (block::SEVEN_EIGHTHS, background, color),
        2 => (block::THREE_QUARTERS, background, color),
        3 => (block::FIVE_EIGHTHS, background, color),
        4 => (block::HALF, background, color),
        5 => (block::THREE_EIGHTHS, background, color),
        6 => (block::ONE_QUARTER, background, color),
        7 => (block::ONE_EIGHTH, background, color),
        _ => (block::FULL, color, background),
    }
}

fn paint_column_cell(
    frame: &mut Frame,
    x: u16,
    y: u16,
    width: u16,
    symbol: &str,
    fg: Color,
    bg: Color,
) {
    for x_offset in 0..width {
        if let Some(cell) = frame.buffer_mut().cell_mut((x + x_offset, y)) {
            cell.set_symbol(symbol).set_fg(fg).set_bg(bg);
        }
    }
}

fn paint_row_cell(
    frame: &mut Frame,
    x: u16,
    y: u16,
    height: u16,
    symbol: &str,
    fg: Color,
    bg: Color,
) {
    for y_offset in 0..height {
        if let Some(cell) = frame.buffer_mut().cell_mut((x, y + y_offset)) {
            cell.set_symbol(symbol).set_fg(fg).set_bg(bg);
        }
    }
}

fn clear_visualization_area(frame: &mut Frame, rect: Rect, theme: &Theme) {
    let app_style = theme.app();
    for x in rect.left()..rect.right() {
        for y in rect.top()..rect.bottom() {
            if let Some(cell) = frame.buffer_mut().cell_mut((x, y)) {
                cell.set_symbol(" ").set_style(app_style);
            }
        }
    }
}

fn background_color(theme: &Theme) -> Color {
    theme.app().bg.unwrap_or(Color::Reset)
}

fn interpolate_color(start: Color, end: Color, t: f32) -> Color {
    let ((sr, sg, sb), (er, eg, eb)) = (color_to_rgb(start), color_to_rgb(end));
    let t = t.clamp(0.0, 1.0);
    let lerp =
        |a: u8, b: u8| -> u8 { (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8 };

    Color::Rgb(lerp(sr, er), lerp(sg, eg), lerp(sb, eb))
}

fn color_to_rgb(color: Color) -> (u8, u8, u8) {
    match color {
        Color::Black => (0, 0, 0),
        Color::Red => (205, 49, 49),
        Color::Green => (13, 188, 121),
        Color::Yellow => (229, 229, 16),
        Color::Blue => (36, 114, 200),
        Color::Magenta => (188, 63, 188),
        Color::Cyan => (17, 168, 205),
        Color::Gray => (204, 204, 204),
        Color::DarkGray => (118, 118, 118),
        Color::LightRed => (241, 76, 76),
        Color::LightGreen => (35, 209, 139),
        Color::LightYellow => (245, 245, 67),
        Color::LightBlue => (59, 142, 234),
        Color::LightMagenta => (214, 112, 214),
        Color::LightCyan => (41, 184, 219),
        Color::White => (242, 242, 242),
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Indexed(i) => indexed_color_to_rgb(i),
        Color::Reset => (255, 255, 255),
    }
}

fn indexed_color_to_rgb(index: u8) -> (u8, u8, u8) {
    match index {
        0 => (0, 0, 0),
        1 => (128, 0, 0),
        2 => (0, 128, 0),
        3 => (128, 128, 0),
        4 => (0, 0, 128),
        5 => (128, 0, 128),
        6 => (0, 128, 128),
        7 => (192, 192, 192),
        8 => (128, 128, 128),
        9 => (255, 0, 0),
        10 => (0, 255, 0),
        11 => (255, 255, 0),
        12 => (0, 0, 255),
        13 => (255, 0, 255),
        14 => (0, 255, 255),
        15 => (255, 255, 255),
        16..=231 => {
            let idx = index - 16;
            let r = idx / 36;
            let g = (idx % 36) / 6;
            let b = idx % 6;
            let conv = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            (conv(r), conv(g), conv(b))
        }
        232..=255 => {
            let gray = 8 + (index - 232) * 10;
            (gray, gray, gray)
        }
    }
}
