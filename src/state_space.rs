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
use rand::{rng, Rng};
use serde::{Deserialize, Serialize};
use stardust_xr_fusion::root::FrameInfo;
use std::{collections::HashMap, f32::consts::FRAC_PI_2};

#[derive(Serialize, Deserialize)]
pub struct NodeData<N> {
	pos: Vec3A,
	#[serde(skip)]
	velocity: Vec3A,

	hash: u64,
	node: N,
}

#[derive(Serialize, Deserialize)]
pub struct StateSpace<const ROWS: usize, const COLS: usize> {
	current: NodeIndex<u32>,
	map: HashMap<u64, NodeIndex<u32>>,
	states: StableUnGraph<NodeData<Board<ROWS, COLS>>, ()>,

	pub settle_speed: f32,

	/// Cooling factor (between 0 and 1) reduces the temperature over iterations.
	/// This simulates "annealing," gradually limiting node movement to reach equilibrium.
	pub cooloff_factor: f32,

	/// Scale factor controlling the overall size / spacing of the layout.
	/// Higher scale values spread nodes farther apart.
	pub scale: f32,
}
impl<const ROWS: usize, const COLS: usize> StateSpace<ROWS, COLS> {
	pub fn new(board: Board<ROWS, COLS>) -> Self {
		let hash = board.board_hash();
		let mut states = StableUnGraph::default();
		let idx = states.add_node(NodeData {
			pos: Vec3A::ZERO,
			hash,
			node: board,
			velocity: Vec3A::ZERO,
		});
		let mut map = HashMap::new();
		map.insert(hash, idx);
		StateSpace {
			current: idx,
			map,
			states,
			settle_speed: 20.0,
			cooloff_factor: 0.95,
			scale: 0.01,
		}
	}
	pub fn add(&mut self, board: &Board<ROWS, COLS>) {
		let hash = board.board_hash();
		if let Some(&idx) = self.map.get(&hash) {
			println!("linking to existing node");
			self.link_to_node(idx);
			return;
		}

		let mut rng = rng();
		let current_pos = self.states.node_weight(self.current).unwrap().pos;
		let idx = self.states.add_node(NodeData {
			pos: current_pos
				+ (vec3a(
					rng.random_range(-1.0..1.0),
					rng.random_range(-1.0..1.0),
					rng.random_range(-1.0..1.0),
				)
				.normalize() * 0.01),
			velocity: Vec3A::ZERO,
			hash,
			node: board.clone(),
		});
		self.link_to_node(idx);
	}
	fn link_to_node(&mut self, node: NodeIndex) {
		if self.current != node && !self.states.contains_edge(self.current, node) {
			self.states.add_edge(self.current, node, ());
		}
		self.current = node;
	}
	pub fn state_count(&self) -> usize {
		self.states.node_count()
	}
	pub fn move_count(&self) -> usize {
		self.states.edge_count()
	}

	/// Applies the Fruchterman-Reingold (1991) force-directed graph layout algorithm
	/// to arrange nodes in 3D space based on graph topology.
	///
	/// This implementation uses velocity persistence and temperature cooling to achieve
	/// stable convergence over multiple iterations.
	pub fn force_direct(&mut self, frame_info: &FrameInfo) {
		if self.states.node_count() == 0 {
			return;
		}

		let nodes: Vec<_> = self.states.node_indices().collect();

		// Calculate forces and update velocities
		for &v in nodes.iter() {
			let pos_v = self.states.node_weight(v).unwrap().pos;
			let mut velocity = self.states.node_weight(v).unwrap().velocity;

			// Calculate repulsion forces (all other nodes)
			let repulsion = nodes
				.iter()
				.filter(|&&other| other != v)
				.map(|&other| {
					let pos_o = self.states.node_weight(other).unwrap().pos;
					let delta = pos_v - pos_o; // Direction from other to this node
					let dist = delta.length().max(0.01);
					let direction = delta / dist; // Normalize

					// FR repulsion: -scale² / distance
					direction * (self.scale * self.scale / dist)
				})
				.fold(Vec3A::ZERO, |acc, v| acc + v);

			// Calculate attraction forces (neighbors)
			let attraction = self
				.states
				.neighbors_undirected(v)
				.filter(|&neighbor| neighbor != v)
				.map(|neighbor| {
					let pos_n = self.states.node_weight(neighbor).unwrap().pos;
					let delta = pos_n - pos_v; // Direction from this node to neighbor
					let dist = delta.length().max(0.01);
					let direction = delta / dist; // Normalize

					// FR attraction: distance² / scale
					direction * (dist * dist / self.scale)
				})
				.fold(Vec3A::ZERO, |acc, v| acc + v);

			// Update velocity with forces and apply damping
			velocity += (attraction + repulsion) * frame_info.delta * self.settle_speed;
			velocity *= self.cooloff_factor;

			// Store the updated velocity
			self.states.node_weight_mut(v).unwrap().velocity = velocity;
		}

		// Apply velocities to positions
		for node_idx in nodes {
			let node = self.states.node_weight_mut(node_idx).unwrap();
			node.pos += node.velocity * frame_info.delta * self.settle_speed;
		}
	}
}
impl<const ROWS: usize, const COLS: usize> Reify for StateSpace<ROWS, COLS> {
	fn reify(&self) -> impl asteroids::Element<Self> {
		Spatial::default()
			.build()
			.child({
				// nodes
				let diamond = circle(4, 0.0, 0.005).thickness(0.001);
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
	}
}
