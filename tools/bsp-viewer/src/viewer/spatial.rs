//! Static broad phase for hover queries; exact geometry tests stay in the viewer.
use glam::Vec2;

struct Entry {
    min: Vec2,
    max: Vec2,
    index: usize,
}

struct Node {
    min: Vec2,
    max: Vec2,
    range: std::ops::Range<usize>,
    children: Option<[usize; 2]>,
}

pub(super) struct SpatialIndex {
    entries: Vec<Entry>,
    nodes: Vec<Node>,
}

impl SpatialIndex {
    pub fn new(bounds: impl Iterator<Item = (Vec2, Vec2)>) -> Self {
        let entries: Vec<_> = bounds
            .enumerate()
            .map(|(index, (min, max))| Entry {
                min,
                max,
                index,
            })
            .collect();
        let mut index = Self {
            nodes: Vec::with_capacity(entries.len()),
            entries,
        };
        if !index.entries.is_empty() {
            index.build(0..index.entries.len());
        }
        index
    }

    fn build(&mut self, range: std::ops::Range<usize>) -> usize {
        let mut min = Vec2::splat(f32::INFINITY);
        let mut max = Vec2::splat(f32::NEG_INFINITY);
        for entry in &self.entries[range.clone()] {
            min = min.min(entry.min);
            max = max.max(entry.max);
        }
        let slot = self.nodes.len();
        self.nodes.push(Node {
            min,
            max,
            range: range.clone(),
            children: None,
        });
        if range.len() > 8 {
            let axis = usize::from(max.y - min.y > max.x - min.x);
            let half = range.len() / 2;
            self.entries[range.clone()].select_nth_unstable_by(half, |a, b| {
                (a.min[axis] + a.max[axis]).total_cmp(&(b.min[axis] + b.max[axis]))
            });
            let mid = range.start + half;
            let left = self.build(range.start..mid);
            let right = self.build(mid..range.end);
            self.nodes[slot].children = Some([left, right]);
        }
        slot
    }

    pub fn query(&self, point: Vec2, radius: f32, output: &mut Vec<usize>) {
        output.clear();
        if !point.is_finite() || !radius.is_finite() || radius < 0.0 || self.nodes.is_empty() {
            return;
        }
        self.visit(
            0,
            point - Vec2::splat(radius),
            point + Vec2::splat(radius),
            output,
        );
        // Preserve original map order, including equal-distance and shared-edge ties.
        output.sort_unstable();
    }

    fn visit(&self, slot: usize, min: Vec2, max: Vec2, output: &mut Vec<usize>) {
        let node = &self.nodes[slot];
        if !node.min.cmple(max).all() || !node.max.cmpge(min).all() {
            return;
        }
        if let Some([left, right]) = node.children {
            self.visit(left, min, max, output);
            self.visit(right, min, max, output);
        } else {
            output.extend(
                self.entries[node.range.clone()]
                    .iter()
                    .filter(|entry| entry.min.cmple(max).all() && entry.max.cmpge(min).all())
                    .map(|entry| entry.index),
            );
        }
    }
}
