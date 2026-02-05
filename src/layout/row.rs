#![allow(dead_code)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum WidowLayoutStyle {
    Justify,
    Center,
    Left,
}

#[derive(Debug, Clone, Copy)]
pub struct LayoutItem {
    pub aspect_ratio: f64,
    pub top: f64,
    pub left: f64,
    pub width: f64,
    pub height: f64,
    pub forced_aspect_ratio: bool,
}

impl Default for LayoutItem {
    fn default() -> Self {
        Self {
            aspect_ratio: 1.0,
            top: 0.0,
            left: 0.0,
            width: 0.0,
            height: 0.0,
            forced_aspect_ratio: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Row {
    pub top: f64,
    pub left: f64,
    pub width: f64,
    pub spacing: f64,
    pub target_row_height: f64,
    pub target_row_height_tolerance: f64,
    pub edge_case_min_row_height: f64,
    pub edge_case_max_row_height: f64,
    pub widow_layout_style: WidowLayoutStyle,
    pub is_breakout_row: bool,
    pub items: Vec<LayoutItem>,
    pub height: f64,
}

impl Row {
    pub fn min_aspect_ratio(&self) -> f64 {
        self.width / self.target_row_height * (1.0 - self.target_row_height_tolerance)
    }

    pub fn max_aspect_ratio(&self) -> f64 {
        self.width / self.target_row_height * (1.0 + self.target_row_height_tolerance)
    }

    pub fn add_item(&mut self, item_data: LayoutItem) -> Result<(), LayoutItem> {
        let mut new_items = self.items.clone();
        new_items.push(item_data.clone());

        let row_width_without_spacing = self.width - self.spacing * (new_items.len() - 1) as f64;
        let new_aspect_ratio: f64 = new_items.iter().map(|x| x.aspect_ratio).sum();
        let target_aspect_ratio = row_width_without_spacing / self.target_row_height;

        if self.is_breakout_row {
            if self.items.is_empty() && item_data.aspect_ratio >= 1.0 {
                self.items.push(item_data);
                self.complete_layout(
                    row_width_without_spacing / item_data.aspect_ratio,
                    Some(WidowLayoutStyle::Justify),
                );
                return Ok(());
            }
        }

        if new_aspect_ratio < self.min_aspect_ratio() {
            self.items.push(item_data);
            return Ok(());
        }
        if new_aspect_ratio > self.max_aspect_ratio() {
            if self.items.is_empty() {
                self.items.push(item_data);
                self.complete_layout(
                    row_width_without_spacing / new_aspect_ratio,
                    Some(WidowLayoutStyle::Justify),
                );
                return Ok(());
            }

            let previous_row_width_without_spacing =
                self.width - self.spacing * (self.items.len() - 1) as f64;
            let previous_aspect_ratio: f64 = self.items.iter().map(|x| x.aspect_ratio).sum();
            let previous_target_aspect_ratio =
                previous_row_width_without_spacing / self.target_row_height;

            if (new_aspect_ratio - target_aspect_ratio).abs()
                > (previous_aspect_ratio - previous_target_aspect_ratio).abs()
            {
                self.complete_layout(
                    previous_row_width_without_spacing / previous_aspect_ratio,
                    Some(WidowLayoutStyle::Justify),
                );
                return Err(item_data);
            } else {
                self.items.push(item_data.clone());
                self.complete_layout(
                    row_width_without_spacing / new_aspect_ratio,
                    Some(WidowLayoutStyle::Justify),
                );
                return Ok(());
            }
        } else {
            self.items.push(item_data.clone());
            self.complete_layout(
                row_width_without_spacing / new_aspect_ratio,
                Some(WidowLayoutStyle::Justify),
            );
            return Ok(());
        }
    }

    pub fn is_layout_complete(&self) -> bool {
        self.height > 0.0
    }

    pub fn complete_layout(
        &mut self,
        new_height: f64,
        widow_layout_style: Option<WidowLayoutStyle>,
    ) {
        let widow_layout_style = widow_layout_style.unwrap_or(WidowLayoutStyle::Left);
        let row_width_without_spacing = self.width - self.spacing * (self.items.len() - 1) as f64;

        let clamped_height =
            new_height.clamp(self.edge_case_min_row_height, self.edge_case_max_row_height);
        let clamped_to_native_ratio = if new_height != clamped_height {
            (row_width_without_spacing / clamped_height) / (row_width_without_spacing / new_height)
        } else {
            1.0
        };
        self.height = clamped_height;

        let mut item_width_sum = self.left;
        for item in &mut self.items {
            item.top = self.top;
            item.width = item.aspect_ratio * self.height * clamped_to_native_ratio;
            item.height = self.height;
            item.left = item_width_sum;
            item_width_sum += item.width + self.spacing;
        }

        match widow_layout_style {
            WidowLayoutStyle::Justify => {
                item_width_sum -= self.spacing + self.left;
                let error_width_per_item = (item_width_sum - self.width) / self.items.len() as f64;
                let rounded_cumulative_errors: Vec<f64> = (0..self.items.len())
                    .map(|i| (error_width_per_item * (i + 1) as f64).round())
                    .collect();

                if self.items.len() == 1 {
                    self.items[0].width -= error_width_per_item.round();
                } else {
                    for (i, item) in self.items.iter_mut().enumerate() {
                        if i > 0 {
                            item.left -= rounded_cumulative_errors[i - 1];
                            item.width -=
                                rounded_cumulative_errors[i] - rounded_cumulative_errors[i - 1];
                        } else {
                            item.width -= rounded_cumulative_errors[i];
                        }
                    }
                }
            }
            WidowLayoutStyle::Center => {
                let center_offset = (self.width - item_width_sum) / 2.0;
                for item in &mut self.items {
                    item.left += center_offset + self.spacing;
                }
            }
            _ => {}
        }
    }

    pub fn force_complete(&mut self, _fit_to_width: bool, row_height: Option<f64>) {
        if let Some(height) = row_height {
            self.complete_layout(height, Some(self.widow_layout_style));
        } else {
            self.complete_layout(self.target_row_height, Some(self.widow_layout_style));
        }
    }

    pub fn get_items(&self) -> &[LayoutItem] {
        &self.items
    }
}
