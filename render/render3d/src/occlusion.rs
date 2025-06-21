#[cfg(feature = "hprof")]
use coarse_prof::profile;

use crate::polygon::Polygon2D;

/// Efficient occlusion buffer that batches span updates per polygon
#[derive(Clone, Debug)]
pub struct OcclusionBuffer {
    /// For each X column, track multiple occluded spans to handle portals
    spans: Vec<Vec<(f32, f32)>>, // List of (top_y, bottom_y) spans per column
    /// Temporary storage for polygon spans before merging
    polygon_spans: Vec<Vec<(f32, f32)>>,
    /// Screen dimensions
    width: usize,
}

impl OcclusionBuffer {
    pub fn new(width: usize, _height: usize) -> Self {
        Self {
            spans: vec![Vec::with_capacity(8); width],
            polygon_spans: vec![Vec::new(); width],
            width,
        }
    }

    /// Reset the buffer for reuse without reallocation
    pub fn reset(&mut self) {
        for spans in self.spans.iter_mut() {
            spans.clear();
        }
    }

    /// Check if a point is occluded
    pub fn is_point_occluded(&self, x: usize, y: f32) -> bool {
        if x >= self.spans.len() {
            return false;
        }

        // Check if point is within any occluded span
        for (top, bottom) in &self.spans[x] {
            if y >= *top && y <= *bottom {
                return true;
            }
        }
        false
    }

    /// Begin collecting spans for a new polygon
    pub fn begin_polygon(&mut self) {
        // Clear temporary polygon spans
        for spans in self.polygon_spans.iter_mut() {
            spans.clear();
        }
    }

    /// Add a span for the current polygon (no merging yet)
    #[inline]
    pub fn add_polygon_span(&mut self, x: usize, top: f32, bottom: f32) {
        if x < self.polygon_spans.len() && top < bottom {
            self.polygon_spans[x].push((top, bottom));
        }
    }

    /// Process a polygon's occlusion data all at once
    pub fn update_polygon_occlusion(&mut self, polygon: &Polygon2D) {
        #[cfg(feature = "hprof")]
        profile!("update_polygon_occlusion");

        self.begin_polygon();

        // First, collect all spans for this polygon
        if let Some((min, max)) = polygon.bounds() {
            let x_start = min.x.max(0.0) as i32;
            let x_end = (max.x.min(self.width as f32 - 1.0) as i32).min(self.width as i32 - 1);

            if x_end < x_start {
                return;
            }

            let vertices = &polygon.vertices;
            let vertex_count = vertices.len();

            // Collect spans for each column
            for x in x_start..=x_end {
                if x >= 0 && (x as usize) < self.width {
                    let x_float = x as f32;
                    let mut y_min = f32::MAX;
                    let mut y_max = f32::MIN;

                    // Find Y range at this X by checking edge intersections
                    for i in 0..vertex_count {
                        let v1 = vertices[i];
                        let v2 = vertices[(i + 1) % vertex_count];

                        // Check if edge crosses this X column
                        if (v1.x <= x_float && v2.x >= x_float)
                            || (v2.x <= x_float && v1.x >= x_float)
                        {
                            let t = if (v2.x - v1.x).abs() > 0.001 {
                                (x_float - v1.x) / (v2.x - v1.x)
                            } else {
                                0.5
                            };
                            let y = v1.y + (v2.y - v1.y) * t.clamp(0.0, 1.0);
                            y_min = y_min.min(y);
                            y_max = y_max.max(y);
                        }
                    }

                    // Add span if valid
                    if y_min <= y_max && y_min != f32::MAX && y_max != f32::MIN {
                        self.add_polygon_span(x as usize, y_min, y_max);
                    }
                }
            }
        }

        // Now merge all polygon spans into the main buffer
        self.merge_polygon_spans();
    }

    /// Merge all polygon spans into the main occlusion buffer
    fn merge_polygon_spans(&mut self) {
        #[cfg(feature = "hprof")]
        profile!("merge_polygon_spans");

        for x in 0..self.width {
            if self.polygon_spans[x].is_empty() {
                continue;
            }

            let column_spans = &mut self.spans[x];
            let new_spans = &self.polygon_spans[x];

            // If column was empty, just copy the new spans
            if column_spans.is_empty() {
                column_spans.extend_from_slice(new_spans);
                if column_spans.len() > 1 {
                    column_spans.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                }
                continue;
            }

            // Merge new spans with existing ones
            // First, add all new spans
            column_spans.extend_from_slice(new_spans);

            // Sort by top coordinate
            column_spans.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

            // Merge overlapping spans in place
            if column_spans.len() > 1 {
                let mut write_idx = 0;

                for read_idx in 1..column_spans.len() {
                    if column_spans[read_idx].0 <= column_spans[write_idx].1 + 0.01 {
                        // Overlapping or adjacent - merge
                        column_spans[write_idx].1 =
                            column_spans[write_idx].1.max(column_spans[read_idx].1);
                    } else {
                        // No overlap - keep both
                        write_idx += 1;
                        if write_idx != read_idx {
                            column_spans[write_idx] = column_spans[read_idx];
                        }
                    }
                }

                // Truncate to remove merged spans
                column_spans.truncate(write_idx + 1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::polygon::Polygon2D;

    use super::*;
    use glam::Vec2;

    #[test]
    fn test_occlusion_basic() {
        let mut buffer = OcclusionBuffer::new(100, 100);

        // Test empty state
        assert!(!buffer.is_point_occluded(0, 50.0));

        // Create a simple polygon
        let polygon = Polygon2D {
            vertices: vec![
                Vec2::new(10.0, 10.0),
                Vec2::new(20.0, 10.0),
                Vec2::new(20.0, 20.0),
                Vec2::new(10.0, 20.0),
            ],
            color: [255, 255, 255, 255],
        };

        // Update occlusion
        buffer.update_polygon_occlusion(&polygon);

        // Check occlusion
        assert!(!buffer.is_point_occluded(5, 15.0)); // Outside polygon
        assert!(buffer.is_point_occluded(15, 15.0)); // Inside polygon
        assert!(!buffer.is_point_occluded(25, 15.0)); // Outside polygon
    }
}
