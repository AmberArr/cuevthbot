// based on https://github.com/flickr/justified-layout
#![allow(dead_code)]
mod row;
use anyhow::Result;
use row::Row;
pub use row::{LayoutItem, WidowLayoutStyle};

#[derive(Debug, Clone, Copy)]
pub struct Padding {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

impl Default for Padding {
    fn default() -> Self {
        Self {
            top: 10.0,
            right: 10.0,
            bottom: 10.0,
            left: 10.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Spacing {
    pub horizontal: f64,
    pub vertical: f64,
}

impl Default for Spacing {
    fn default() -> Self {
        Self {
            horizontal: 10.0,
            vertical: 10.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LayoutConfig {
    pub container_width: f64,
    pub container_padding: Padding,
    pub box_spacing: Spacing,
    pub target_row_height: Vec<f64>,
    pub target_row_height_tolerance: f64,
    pub edge_case_min_row_height_factor: f64,
    pub edge_case_max_row_height_factor: f64,
    pub max_num_rows: usize,
    pub force_aspect_ratio: Option<f64>,
    pub show_widows: bool,
    pub full_width_breakout_row_cadence: usize,
    pub widow_layout_style: WidowLayoutStyle,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            container_width: 1060.0,
            container_padding: Padding::default(),
            box_spacing: Spacing::default(),
            target_row_height: vec![320.0],
            target_row_height_tolerance: 0.25,
            edge_case_min_row_height_factor: 0.5,
            edge_case_max_row_height_factor: 2.,
            max_num_rows: usize::MAX,
            force_aspect_ratio: None,
            show_widows: true,
            full_width_breakout_row_cadence: 0,
            widow_layout_style: WidowLayoutStyle::Left,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LayoutResult {
    pub container_height: f64,
    pub widow_count: usize,
    pub boxes: Vec<LayoutItem>,
}

struct LayoutState {
    container_height: f64,
    rows: Vec<Row>,
}

impl LayoutState {
    fn create_new_row(&self, config: &LayoutConfig) -> Row {
        let cadence = config.full_width_breakout_row_cadence;
        let is_breakout_row = cadence != 0 && (self.rows.len() + 1) % cadence == 0;
        let target_row_height =
            config.target_row_height[self.rows.len() % config.target_row_height.len()];

        Row {
            top: self.container_height,
            left: config.container_padding.left,
            width: config.container_width
                - config.container_padding.left
                - config.container_padding.right,
            spacing: config.box_spacing.horizontal,
            target_row_height,
            target_row_height_tolerance: config.target_row_height_tolerance,
            edge_case_min_row_height: config.edge_case_min_row_height_factor * target_row_height,
            edge_case_max_row_height: config.edge_case_max_row_height_factor * target_row_height,
            widow_layout_style: config.widow_layout_style,
            is_breakout_row,
            items: Vec::new(),
            height: 0.0,
        }
    }

    fn add_row(&mut self, config: &LayoutConfig, row: Row) -> Vec<LayoutItem> {
        self.container_height += row.height + config.box_spacing.vertical;
        let items = row.items.clone();
        self.rows.push(row);
        items
    }
}

// input contains aspect ratios
pub fn compute(input: &[f64], config: &LayoutConfig) -> Result<LayoutResult> {
    let mut state = LayoutState {
        container_height: config.container_padding.top,
        rows: Vec::new(),
    };
    let mut laid_out_items = Vec::new();

    // Convert input aspect ratios to LayoutItems
    let item_layout_data: Vec<LayoutItem> = input
        .iter()
        .map(|&ar| LayoutItem {
            aspect_ratio: config.force_aspect_ratio.unwrap_or(ar),
            forced_aspect_ratio: config.force_aspect_ratio.is_some(),
            ..Default::default()
        })
        .collect();

    let mut current_row: Option<Row> = None;

    for (i, item_data) in item_layout_data.into_iter().enumerate() {
        if item_data.aspect_ratio.is_nan() {
            anyhow::bail!("Item {} has an invalid aspect ratio", i);
        }

        let mut row = current_row.unwrap_or(state.create_new_row(config));

        let item_added = row.add_item(item_data);

        if row.is_layout_complete() {
            // Row is filled; add it and try to start a new one
            laid_out_items.extend(state.add_row(config, row));
            if state.rows.len() >= config.max_num_rows {
                current_row = None;
                break;
            }
            row = state.create_new_row(config);

            // Item was rejected; add it to its own row
            if let Err(item_data) = item_added {
                let _ = row.add_item(item_data);
                if row.is_layout_complete() {
                    laid_out_items.extend(state.add_row(config, row));
                    if state.rows.len() >= config.max_num_rows {
                        current_row = None;
                        break;
                    }
                    current_row = Some(state.create_new_row(config));
                    continue;
                }
            }
        }
        current_row = Some(row);
    }

    // Handle orphans
    let mut widow_count = 0;
    if let Some(mut row) = current_row {
        if !row.items.is_empty() && config.show_widows {
            if !state.rows.is_empty() {
                let last_row = &state.rows[state.rows.len() - 1];
                let next_to_last_row_height = if last_row.is_breakout_row {
                    last_row.target_row_height
                } else {
                    last_row.height
                };
                row.force_complete(false, Some(next_to_last_row_height));
            } else {
                row.force_complete(false, None);
            }

            widow_count = row.items.len();
            laid_out_items.extend(state.add_row(config, row));
        }
    }

    // Cleanup bottom padding
    state.container_height -= config.box_spacing.vertical;
    state.container_height += config.container_padding.bottom;

    Ok(LayoutResult {
        container_height: state.container_height,
        widow_count,
        boxes: laid_out_items,
    })
}
