//! A Barnes-Hut octree used to approximate the repulsion forces in the
//! force-directed layout.
//!
//! The naive Fruchterman-Reingold repulsion step is O(n²): every node has to
//! consider every other node. Barnes-Hut groups distant nodes into a single
//! aggregate body (their center of mass) whenever the group is "far enough"
//! away, as judged by the opening criterion `cell_width / distance < theta`.
//! This brings the repulsion step down to roughly O(n log n).

use glam::Vec3A;

/// Subdivision is abandoned past this depth. This guards against
/// (near-)coincident points causing unbounded recursion. Past this depth the
/// bodies in a cell are simply treated as a single merged aggregate.
const MAX_DEPTH: u32 = 24;

/// Distance below which we consider a body to be "ourselves" and skip it, so a
/// node does not repel against its own position.
const SELF_EPSILON: f32 = 1e-6;

struct OctNode {
	/// Geometric center of this cell's cube.
	center: Vec3A,
	/// Half the side length of this cell's cube.
	half: f32,
	/// Sum of the positions of every body under this cell. Divided by `count`
	/// this gives the center of mass.
	com_sum: Vec3A,
	/// Number of bodies under this cell (the aggregate "mass").
	count: u32,
	/// Whether this cell has been subdivided into children.
	internal: bool,
	/// Position of the single body held by this cell while it is still an
	/// un-subdivided leaf.
	body: Option<Vec3A>,
	/// Indices into [`Octree::nodes`] of the 8 octant children, or -1 if absent.
	children: [i32; 8],
}

impl OctNode {
	fn new(center: Vec3A, half: f32) -> Self {
		OctNode {
			center,
			half,
			com_sum: Vec3A::ZERO,
			count: 0,
			internal: false,
			body: None,
			children: [-1; 8],
		}
	}
}

pub struct Octree {
	nodes: Vec<OctNode>,
}

impl Octree {
	/// Builds an octree containing every position in `positions`.
	pub fn build(positions: &[Vec3A]) -> Self {
		let mut tree = Octree { nodes: Vec::new() };

		if positions.is_empty() {
			return tree;
		}

		// Compute a cube that bounds every point.
		let mut min = positions[0];
		let mut max = positions[0];
		for &p in &positions[1..] {
			min = min.min(p);
			max = max.max(p);
		}
		let center = (min + max) * 0.5;
		// Pad so every point is strictly inside, and avoid a zero-size root
		// when all points coincide.
		let half = ((max - min).max_element() * 0.5).max(1e-3) + 1e-3;

		tree.nodes.push(OctNode::new(center, half));
		for &p in positions {
			tree.insert(0, p, 0);
		}
		tree
	}

	/// Returns the index of the octant child of `idx` containing `pos`,
	/// creating the eight children if they do not yet exist.
	fn child_for(&mut self, idx: usize, pos: Vec3A) -> usize {
		let center = self.nodes[idx].center;
		let oct = (usize::from(pos.x >= center.x))
			| (usize::from(pos.y >= center.y) << 1)
			| (usize::from(pos.z >= center.z) << 2);

		if self.nodes[idx].children[oct] < 0 {
			let half = self.nodes[idx].half * 0.5;
			let offset = Vec3A::new(
				if oct & 1 != 0 { half } else { -half },
				if oct & 2 != 0 { half } else { -half },
				if oct & 4 != 0 { half } else { -half },
			);
			let child = OctNode::new(center + offset, half);
			let child_idx = self.nodes.len();
			self.nodes.push(child);
			self.nodes[idx].children[oct] = child_idx as i32;
		}
		self.nodes[idx].children[oct] as usize
	}

	fn insert(&mut self, idx: usize, pos: Vec3A, depth: u32) {
		self.nodes[idx].com_sum += pos;
		let prev_count = self.nodes[idx].count;
		self.nodes[idx].count += 1;

		// Already subdivided: descend into the right octant.
		if self.nodes[idx].internal {
			let child = self.child_for(idx, pos);
			self.insert(child, pos, depth + 1);
			return;
		}

		// Empty leaf: this body now occupies it.
		if prev_count == 0 {
			self.nodes[idx].body = Some(pos);
			return;
		}

		// Occupied leaf that we are not allowed to subdivide any further: leave
		// it as a merged aggregate (the new body's position is already folded
		// into `com_sum`).
		if depth >= MAX_DEPTH {
			return;
		}

		// Occupied leaf: subdivide and push both the existing and the new body
		// down into the children.
		let existing = self.nodes[idx].body.take().unwrap();
		self.nodes[idx].internal = true;

		let child = self.child_for(idx, existing);
		self.insert(child, existing, depth + 1);
		let child = self.child_for(idx, pos);
		self.insert(child, pos, depth + 1);
	}

	/// Approximates the Fruchterman-Reingold repulsion force acting on a node at
	/// `pos`.
	///
	/// `scale` matches the layout's scale factor and `theta` is the Barnes-Hut
	/// opening threshold (smaller = more accurate and slower, larger = faster
	/// and coarser; 0.5 is a common default).
	pub fn repulsion(&self, pos: Vec3A, scale: f32, theta: f32) -> Vec3A {
		if self.nodes.is_empty() {
			return Vec3A::ZERO;
		}

		let mut acc = Vec3A::ZERO;
		let mut stack = [0usize; 8 * (MAX_DEPTH as usize + 2)];
		let mut top = 1usize;
		stack[0] = 0;

		while top > 0 {
			top -= 1;
			let node = &self.nodes[stack[top]];
			if node.count == 0 {
				continue;
			}

			let com = node.com_sum / node.count as f32;
			let delta = pos - com;
			let dist = delta.length();

			let aggregate = if node.internal {
				// Treat the whole cell as one body when it is far enough away.
				dist > SELF_EPSILON && node.half * 2.0 / dist < theta
			} else {
				// A leaf is always a single (point-like) aggregate, unless it
				// is effectively our own position.
				if dist < SELF_EPSILON {
					continue;
				}
				true
			};

			if aggregate {
				let d = dist.max(0.01);
				// FR repulsion (scale² / distance) scaled by the cell's mass.
				acc += (delta / d) * (scale * scale / d) * node.count as f32;
			} else {
				for &c in &node.children {
					if c >= 0 {
						stack[top] = c as usize;
						top += 1;
					}
				}
			}
		}
		acc
	}
}
