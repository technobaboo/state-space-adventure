use crate::board::Board;
use crate::octree::Octree;
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
use std::collections::{HashMap, VecDeque};

#[derive(Clone, Serialize, Deserialize)]
pub struct NodeData<N> {
	pos: Vec3A,
	#[serde(skip)]
	velocity: Vec3A,

	hash: u64,
	node: N,

	/// The node we discovered this one from, i.e. the move we'd undo to back
	/// out of it. `None` for the root. This is what lets [`StateSpace::explore_dfs`]
	/// backtrack during its depth-first walk without keeping any separate,
	/// independently-persisted frontier — the path back to the root is just
	/// parent pointers stored on nodes that are already part of the graph.
	#[serde(default)]
	parent: Option<NodeIndex<u32>>,

	/// BFS distance from the root, recorded the moment this node is discovered.
	/// Used only to rebuild [`StateSpace::bfs_frontier`] in the right order if it
	/// ever needs to be reconstructed from scratch.
	#[serde(default)]
	depth: u32,

	/// Whether [`StateSpace::explore_bfs`] has expanded this node yet (looked at
	/// all of its neighbors). Defaulting to `false` on old data just means we
	/// redo that (idempotent) work rather than silently skip nodes.
	#[serde(default)]
	bfs_expanded: bool,
}

/// Selects which traversal [`StateSpace::explore`] performs.
#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExploreMode {
	Depth,
	#[default]
	Breadth,
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

	/// Barnes-Hut opening threshold for the octree-approximated repulsion.
	/// Smaller values are more accurate but slower; larger values approximate
	/// more aggressively. 0.5 is a common default.
	#[serde(default = "default_theta")]
	pub theta: f32,

	/// Hard cap on node speed (units/sec). Without this, a frame with a
	/// large delta or a cluster of overlapping nodes (e.g. right after
	/// several new states are discovered at the same spot) can produce a
	/// runaway repulsion/attraction feedback loop that flings nodes out to
	/// infinity instead of settling.
	#[serde(default = "default_max_speed")]
	pub max_speed: f32,

	pub mode: ExploreMode,

	/// Queue for [`Self::explore_bfs`]. Deliberately *not* serialized: rather
	/// than trust a persisted queue to stay in sync with the graph across
	/// saves/reloads (the bug the old depth-first frontier had), we rebuild it
	/// on demand from the `bfs_expanded` flag on each node whenever it runs dry.
	#[serde(skip)]
	bfs_frontier: VecDeque<NodeIndex<u32>>,
}

fn default_theta() -> f32 {
	0.5
}
fn default_max_speed() -> f32 {
	2.0
}

/// Largest delta-time we'll integrate in one step. Clamping this stops a
/// frame hitch (e.g. loading a board, a stall) from being treated as a huge
/// timestep, which would otherwise blow forces and velocities up in a single
/// step.
const MAX_DELTA: f32 = 1.0 / 30.0;
impl StateSpace {
	pub fn new(board: Board) -> Self {
		let hash = board.board_hash();
		let mut states = StableUnGraph::default();
		let idx = states.add_node(NodeData {
			pos: Vec3A::ZERO,
			hash,
			node: board,
			velocity: Vec3A::ZERO,
			parent: None,
			depth: 0,
			bfs_expanded: false,
		});
		let mut map = HashMap::new();
		map.insert(hash, idx);
		StateSpace {
			current: idx,
			map,
			states,
			settle_speed: 10.0,
			cooloff_factor: 0.95,
			scale: 0.01,
			theta: default_theta(),
			max_speed: default_max_speed(),
			mode: ExploreMode::default(),
			bfs_frontier: VecDeque::from([idx]),
		}
	}

	/// Expand the next state in the reachable state space, following
	/// [`Self::mode`]. Returns the board reached this step, or `None` once the
	/// whole connected state space has been mapped.
	pub fn explore(&mut self) -> Option<Board> {
		match self.mode {
			ExploreMode::Depth => self.explore_dfs(),
			ExploreMode::Breadth => self.explore_bfs(),
		}
	}

	/// Depth-first walk of the reachable state space, one step per call.
	///
	/// At [`Self::current`] we look at every board reachable in a single move:
	/// newly-discovered ones get added to the graph and wired up with an edge,
	/// already-known ones just get the edge (so no move is ever dropped, even
	/// the ones that loop back to a state we've already visited). The moment we
	/// find a genuinely new state we step into it and return. If every neighbor
	/// of `current` is already known, we've exhausted this branch, so we back
	/// out along the parent pointer recorded when we first stepped into it and
	/// try again from there.
	///
	/// Unlike a separately-tracked frontier queue, the only state this needs —
	/// `current`, plus each node's `parent` — already lives in the graph itself,
	/// so there's nothing extra that can fall out of sync with it. Returns the
	/// board we just stepped into, or `None` once we've backtracked all the way
	/// to the root with nowhere left to go.
	fn explore_dfs(&mut self) -> Option<Board> {
		loop {
			let board = self.states.node_weight(self.current).unwrap().node.clone();
			let mut stepped_into = None;

			for (block, next) in board.neighbors() {
				let hash = next.board_hash();
				let discovered = self.map.contains_key(&hash);
				let depth = self.states.node_weight(self.current).unwrap().depth;
				let neighbor = self.ensure_node(next, hash, depth + 1);
				self.add_edge(self.current, neighbor, block.display_color());
				if !discovered {
					self.states.node_weight_mut(neighbor).unwrap().parent = Some(self.current);
					stepped_into = Some(neighbor);
					break;
				}
			}

			if let Some(neighbor) = stepped_into {
				self.current = neighbor;
				return Some(self.states.node_weight(neighbor).unwrap().node.clone());
			}

			self.current = self.states.node_weight(self.current).unwrap().parent?;
		}
	}

	/// Breadth-first walk of the reachable state space, one step per call.
	///
	/// Expands the next not-yet-expanded node off [`Self::bfs_frontier`]: every
	/// board reachable from it in one move gets an edge (existing or new), and
	/// newly-discovered ones get queued. If the queue ever runs dry without
	/// every node being expanded (e.g. right after a reload, since the queue
	/// itself isn't persisted), it's rebuilt from scratch by scanning for
	/// unexpanded nodes in `depth` order, so the walk can't get stuck on a
	/// stale or empty queue. Returns `None` only once no unexpanded node is
	/// left anywhere in the graph.
	fn explore_bfs(&mut self) -> Option<Board> {
		loop {
			let Some(idx) = self.bfs_frontier.pop_front() else {
				if !self.rebuild_bfs_frontier() {
					return None;
				}
				continue;
			};

			let node = self.states.node_weight(idx).unwrap();
			if node.bfs_expanded {
				continue;
			}
			self.current = idx;
			let board = node.node.clone();
			let depth = node.depth;

			for (block, next) in board.neighbors() {
				let hash = next.board_hash();
				let discovered = self.map.contains_key(&hash);
				let neighbor = self.ensure_node(next, hash, depth + 1);
				self.add_edge(idx, neighbor, block.display_color());
				if !discovered {
					self.bfs_frontier.push_back(neighbor);
				}
			}
			self.states.node_weight_mut(idx).unwrap().bfs_expanded = true;

			return Some(board);
		}
	}

	/// Refills [`Self::bfs_frontier`] from every node not yet marked expanded,
	/// ordered by BFS depth. Returns whether it found anything to queue.
	fn rebuild_bfs_frontier(&mut self) -> bool {
		let mut pending: Vec<_> = self
			.states
			.node_indices()
			.filter(|&i| !self.states.node_weight(i).unwrap().bfs_expanded)
			.collect();
		if pending.is_empty() {
			return false;
		}
		pending.sort_by_key(|&i| self.states.node_weight(i).unwrap().depth);
		self.bfs_frontier = pending.into();
		true
	}

	/// Returns the node for `board`, inserting it (positioned just off the
	/// current node) if it isn't already in the graph.
	fn ensure_node(&mut self, board: Board, hash: u64, depth: u32) -> NodeIndex<u32> {
		if let Some(&idx) = self.map.get(&hash) {
			return idx;
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
			node: board,
			parent: None,
			depth,
			bfs_expanded: false,
		});
		self.map.insert(hash, idx);
		idx
	}

	/// Adds an undirected edge between two distinct states if it doesn't exist yet.
	fn add_edge(&mut self, a: NodeIndex<u32>, b: NodeIndex<u32>, color: Color) {
		if a != b && !self.states.contains_edge(a, b) {
			self.states.add_edge(a, b, EdgeData(color));
		}
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

		let delta = frame_info.delta.min(MAX_DELTA);
		let nodes: Vec<_> = self.states.node_indices().collect();

		// Build a Barnes-Hut octree over the current node positions so each
		// node can approximate repulsion against distant clusters as a single
		// aggregate body, instead of iterating over every other node (O(n²)).
		let positions: Vec<_> = nodes
			.iter()
			.map(|&v| self.states.node_weight(v).unwrap().pos)
			.collect();
		let octree = Octree::build(&positions);

		// Calculate forces and update velocities in parallel
		let velocity_updates: Vec<_> = nodes
			.par_iter()
			.map(|&v| {
				let pos_v = self.states.node_weight(v).unwrap().pos;
				let mut velocity = self.states.node_weight(v).unwrap().velocity;

				// Approximate repulsion forces against all other nodes via the octree.
				let repulsion = octree.repulsion(pos_v, self.scale, self.theta);

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
				velocity += (attraction + repulsion) * delta * self.settle_speed;
				velocity *= self.cooloff_factor;

				// Hard speed cap: stops a runaway repulsion/attraction
				// feedback loop from flinging nodes out to infinity.
				let speed = velocity.length();
				if speed > self.max_speed {
					velocity *= self.max_speed / speed;
				}

				(v, velocity)
			})
			.collect();

		// Apply the calculated velocities
		for (node_idx, velocity) in velocity_updates {
			self.states.node_weight_mut(node_idx).unwrap().velocity = velocity;
		}

		// Apply velocities to positions. `settle_speed` already shaped how
		// strongly forces feed into velocity above; applying it again here
		// would double its effect (effectively squaring it) and overdrive
		// the integration into a stiff, oscillating system.
		for node_idx in nodes {
			let node = self.states.node_weight_mut(node_idx).unwrap();
			node.pos += node.velocity * delta;
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
									self.states.node_weight(e.source())?.pos,
									self.states.node_weight(e.target())?.pos,
								])
								.thickness(0.001)
								.color(e.weight().0),
							)
						}))
						.build()
					}),
			)
			.children({
				let step_amount = (self.state_count() / 10).max(1);
				// nodes
				self.states
					.node_references()
					.step_by(step_amount)
					.map(|(idx, d)| {
						Handle::new(d.pos, move |state: &mut Self, pos| {
							let Some(weight) = state.states.node_weight_mut(idx) else {
								return;
							};
							weight.pos = pos.into();
						})
						.build()
					})
			})
	}
}
