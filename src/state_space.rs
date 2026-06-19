use crate::board::{Block, Board};
use glam::{vec3a, Vec3A};
use itertools::Itertools;
use petgraph::{
	graph::NodeIndex,
	prelude::StableUnGraph,
	visit::{EdgeRef, IntoEdgeReferences, IntoNodeReferences},
};
use rand::{rng, Rng};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use stardust_xr_asteroids::{
	elements::{line_from_points, shape, Handle, LineExt, Lines, Spatial},
	Context, CustomElement, Element, Reify, Tasker, Transformable,
};
use stardust_xr_fusion::{
	client::FrameInfo,
	fields::Shape,
	types::{rgba_linear, Color},
};
use std::collections::HashMap;

#[derive(Clone, Serialize, Deserialize)]
pub struct NodeData<N> {
	pos: Vec3A,
	#[serde(skip)]
	velocity: Vec3A,

	hash: u64,
	node: N,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct EdgeData(Color);

#[derive(Clone, Serialize, Deserialize)]
pub struct StateSpace {
	current: NodeIndex<u32>,
	map: HashMap<u64, NodeIndex<u32>>,
	states: StableUnGraph<NodeData<Board>, EdgeData>,

	pub settle_speed: f32,

	/// Cooling factor (between 0 and 1) reduces the temperature over iterations.
	/// This simulates "annealing," gradually limiting node movement to reach equilibrium.
	pub cooloff_factor: f32,

	/// Scale factor controlling the overall size / spacing of the layout.
	/// Higher scale values spread nodes farther apart.
	pub scale: f32,
}
impl StateSpace {
	pub fn new(board: Board) -> Self {
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
			settle_speed: 5.0,
			cooloff_factor: 0.95,
			scale: 0.025,
		}
	}
	pub fn add(&mut self, board: &Board, block: &Block) {
		let hash = board.board_hash();
		if let Some(&idx) = self.map.get(&hash) {
			self.link_to_node(idx, EdgeData(block.color));
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
		self.map.insert(hash, idx);
		self.link_to_node(idx, EdgeData(block.color));
	}
	fn link_to_node(&mut self, node: NodeIndex, data: EdgeData) {
		if self.current != node && !self.states.contains_edge(self.current, node) {
			self.states.add_edge(self.current, node, data);
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

		// Calculate forces and update velocities in parallel
		let velocity_updates: Vec<_> = nodes
			.par_iter()
			.map(|&v| {
				let pos_v = self.states.node_weight(v).unwrap().pos;
				let mut velocity = self.states.node_weight(v).unwrap().velocity;

				// Calculate repulsion forces (all other nodes)
				let repulsion = nodes
					.par_iter()
					.filter(|&&other| other != v)
					.map(|&other| {
						let pos_o = self.states.node_weight(other).unwrap().pos;
						let delta = pos_v - pos_o; // Direction from other to this node
						let dist = delta.length().max(0.01);
						let direction = delta / dist; // Normalize

						// FR repulsion: -scale² / distance
						direction * (self.scale * self.scale / dist)
					})
					.reduce(|| Vec3A::ZERO, |acc, v| acc + v);

				// Calculate attraction forces (neighbors)
				let neighbors: Vec<_> = self.states.neighbors_undirected(v).collect();
				let attraction = neighbors
					.par_iter()
					.filter(|&&neighbor| neighbor != v)
					.map(|&neighbor| {
						let pos_n = self.states.node_weight(neighbor).unwrap().pos;
						let delta = pos_n - pos_v; // Direction from this node to neighbor
						let dist = delta.length().max(0.01);
						let direction = delta / dist; // Normalize

						// FR attraction: distance² / scale
						direction * (dist * dist / self.scale)
					})
					.reduce(|| Vec3A::ZERO, |acc, v| acc + v);

				// Update velocity with forces and apply damping
				velocity += (attraction + repulsion) * frame_info.delta * self.settle_speed;
				velocity *= self.cooloff_factor;

				(v, velocity)
			})
			.collect();

		// Apply the calculated velocities
		for (node_idx, velocity) in velocity_updates {
			self.states.node_weight_mut(node_idx).unwrap().velocity = velocity;
		}

		// Apply velocities to positions
		for node_idx in nodes {
			let node = self.states.node_weight_mut(node_idx).unwrap();
			node.pos += node.velocity * frame_info.delta * self.settle_speed;
		}
	}
}
impl Reify for StateSpace {
	fn reify(&self, _context: &Context, _tasks: impl Tasker<Self>) -> impl Element<Self> {
		Spatial::default()
			.build()
			.child(
				Lines::new(
					shape(Shape::Sphere { radius: 0.001 })
						.into_iter()
						.map(|l| l.thickness(0.01).color(rgba_linear!(1.0, 0.0, 1.0, 1.0))),
				)
				.pos(self.states.node_weight(self.current).unwrap().pos)
				.build(),
			)
			.children(
				// edges
				self.states
					.edge_references()
					.chunks(100)
					.into_iter()
					.map(|chunk| {
						Lines::new(chunk.filter_map(|e| {
							Some(
								line_from_points(vec![
									self.states
										.node_weight(e.source())?
										.pos
										.clamp_length_max(10.0),
									self.states
										.node_weight(e.target())?
										.pos
										.clamp_length_max(10.0),
								])
								.thickness(0.001)
								.color(e.weight().0),
							)
						}))
						.build()
					}),
			)
			.children(
				// nodes
				(self.state_count() < 25)
					.then(|| {
						self.states
							.node_references()
							.map(|(idx, d)| {
								Handle::new(d.pos, move |state: &mut Self, pos| {
									state.states.node_weight_mut(idx).unwrap().pos = pos.into();
								})
								.build()
							})
							.collect::<Vec<_>>()
					})
					.into_iter()
					.flatten(),
			)
	}
}
