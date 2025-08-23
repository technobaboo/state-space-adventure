use std::{collections::HashMap, f32::consts::FRAC_PI_2};

use crate::board::Board;
use asteroids::{
	elements::{circle, line_from_points, LineExt, Lines, Spatial},
	CustomElement, Reify,
};
use glam::{vec3a, Mat4, Vec3A};
use petgraph::{
	graph::NodeIndex,
	prelude::StableUnGraph,
	visit::{EdgeRef, IntoEdgeReferences},
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct NodeData<N> {
	pos: Vec3A,
	#[serde(skip)]
	disp: Vec3A,

	hash: u64,
	node: N,
}

#[derive(Serialize, Deserialize)]
pub struct StateSpace<const ROWS: usize, const COLS: usize> {
	current: NodeIndex<u32>,
	map: HashMap<u64, NodeIndex<u32>>,
	states: StableUnGraph<NodeData<Board<ROWS, COLS>>, ()>,
}
impl<const ROWS: usize, const COLS: usize> StateSpace<ROWS, COLS> {
	pub fn new(board: Board<ROWS, COLS>) -> Self {
		let hash = board.board_hash();
		let mut states = StableUnGraph::default();
		let idx = states.add_node(NodeData {
			pos: Vec3A::ZERO,
			hash,
			node: board,
			disp: Vec3A::ZERO,
		});
		let mut map = HashMap::new();
		map.insert(hash, idx);
		StateSpace {
			current: idx,
			map,
			states,
		}
	}
	pub fn add(&mut self, board: &Board<ROWS, COLS>) {
		let hash = board.board_hash();
		if self.map.contains_key(&hash) {
			return;
		}
		let current_pos = self.states.node_weight(self.current).unwrap().pos;
		let idx = self.states.add_node(NodeData {
			pos: current_pos + vec3a(0.01, 0.01, 0.01),
			disp: Vec3A::ZERO,
			hash,
			node: board.clone(),
		});
		self.states.add_edge(self.current, idx, ());
		self.current = idx;
	}
	pub fn state_count(&self) -> usize {
		self.states.node_count()
	}
	pub fn move_count(&self) -> usize {
		self.states.edge_count()
	}

	pub fn force_direct(&mut self, k: f32, damping: f32) {
		if self.states.node_count() == 0 {
			return;
		}

		// Initialize displacement vectors to zero
		let nodes: Vec<_> = self.states.node_indices().collect();
		for node_idx in nodes.iter().cloned() {
			if let Some(node) = self.states.node_weight_mut(node_idx) {
				node.disp = Vec3A::ZERO;
			}
		}

		// Repulsive forces between all pairs of nodes
		for (i, &v) in nodes.iter().enumerate() {
			for &u in nodes.iter().skip(i + 1) {
				let pos_v = self.states.node_weight(v).unwrap().pos;
				let pos_u = self.states.node_weight(u).unwrap().pos;
				let delta = pos_v - pos_u;
				let dist = delta.length().max(0.01);
				let force = k * k / dist;
				let direction = delta / dist;

				self.states.node_weight_mut(v).unwrap().disp += direction * force;
				self.states.node_weight_mut(u).unwrap().disp -= direction * force;
			}
		}

		// Attractive forces along edges
		let edges = self
			.states
			.edge_references()
			.map(|e| (e.source(), e.target()))
			.collect::<Vec<_>>();
		for (source, target) in edges {
			let pos_s = self.states.node_weight(source).unwrap().pos;
			let pos_t = self.states.node_weight(target).unwrap().pos;
			let delta = pos_s - pos_t;
			let dist = delta.length().max(0.01);
			let force = (dist * dist) / k;
			let direction = delta / dist;

			self.states.node_weight_mut(source).unwrap().disp -= direction * force;
			self.states.node_weight_mut(target).unwrap().disp += direction * force;
		}

		// Update node positions with damping and max displacement capped by temperature = k
		let temperature = k;
		for node_idx in nodes {
			let node = self.states.node_weight_mut(node_idx).unwrap();
			let len = node.disp.length();

			let delta_pos = if len > temperature {
				node.disp / len * temperature
			} else {
				node.disp
			};

			node.pos += delta_pos * damping;
		}
	}
}
impl<const ROWS: usize, const COLS: usize> Reify for StateSpace<ROWS, COLS> {
	fn reify(&self) -> impl asteroids::Element<Self> {
		Spatial::default()
			.build()
			.child(
				// edges
				Lines::new(self.states.edge_references().filter_map(|e| {
					Some(
						line_from_points(vec![
							self.states.node_weight(e.source())?.pos,
							self.states.node_weight(e.target())?.pos,
						])
						.thickness(0.001),
					)
				}))
				.build(),
			)
			.child({
				// nodes
				let diamond = circle(4, 0.0, 0.01).thickness(0.001);
				let octahedron = [
					diamond.clone().transform(Mat4::from_rotation_x(FRAC_PI_2)),
					diamond.clone().transform(Mat4::from_rotation_z(FRAC_PI_2)),
					diamond,
				];
				Lines::new(self.states.node_weights().flat_map(|n| {
					octahedron.iter().map(|shape| {
						shape
							.clone()
							.transform(Mat4::from_translation(n.pos.into()))
					})
				}))
				.build()
			})
	}
}
