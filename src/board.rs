use glam::{vec3, Mat4};
use mint::Vector2;
use serde::{Deserialize, Serialize};
use stardust_xr_asteroids::{
	elements::{LineExt, Lines},
	Context, CustomElement, Element, Reify, Tasker, Transformable,
};
use stardust_xr_fusion::{
	drawable::{Line, LinePoint},
	types::{rgba_linear, Color},
};
use std::{
	collections::HashSet,
	hash::{DefaultHasher, Hash, Hasher},
	path::Path,
};

pub type Cell = Vector2<usize>;

/// Represents a block on the board.
/// The block occupies a contiguous rectangle defined by top-left cell, width and height.
/// `color` is a linear `[r, g, b, a]`; it's converted to a display [`Color`] on demand.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
	pub color: [f32; 4],
	pub top_left: Cell,
	pub width: usize,
	pub height: usize,
}
impl Block {
	/// The block's color as a renderable [`Color`].
	pub fn display_color(&self) -> Color {
		let [r, g, b, a] = self.color;
		rgba_linear!(r, g, b, a)
	}

	fn cells(&self) -> HashSet<Cell> {
		let mut set = HashSet::new();
		for r in self.top_left.y..self.top_left.y + self.height {
			for c in self.top_left.x..self.top_left.x + self.width {
				set.insert(Cell { y: r, x: c });
			}
		}
		set
	}

	fn moved(&self, delta_x: isize, delta_y: isize) -> Option<Block> {
		let new_x = (self.top_left.x as isize).checked_add(delta_x)?;
		let new_y = (self.top_left.y as isize).checked_add(delta_y)?;
		if new_x < 0 || new_y < 0 {
			return None;
		}
		Some(Block {
			color: self.color,
			top_left: Cell {
				x: new_x as usize,
				y: new_y as usize,
			},
			width: self.width,
			height: self.height,
		})
	}
}

// Equality and hashing ignore color: two blocks of the same shape and position
// are the same board state regardless of how they're painted.
impl PartialEq for Block {
	fn eq(&self, other: &Self) -> bool {
		self.top_left == other.top_left && self.width == other.width && self.height == other.height
	}
}
impl Eq for Block {}
impl Hash for Block {
	fn hash<H: Hasher>(&self, state: &mut H) {
		self.top_left.hash(state);
		self.width.hash(state);
		self.height.hash(state);
	}
}

/// Klotski board. This is also the on-disk `.ron` format, serialized directly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Board {
	width: usize,
	height: usize,
	pinned: bool,
	blocks: Vec<Block>,
}
impl Hash for Board {
	fn hash<H: Hasher>(&self, state: &mut H) {
		// Hash in a canonical (position-sorted) order so equivalent states match
		// regardless of the order blocks happen to be stored in.
		let mut blocks = self.blocks.iter().collect::<Vec<_>>();
		blocks.sort_by_key(|b| (b.top_left.x, b.top_left.y, b.width, b.height));
		for block in blocks {
			block.hash(state);
		}
	}
}
impl Board {
	/// Checks the board's invariants: positive dimensions, every block inside
	/// the bounds, and no two blocks overlapping.
	fn validate(&self) -> Result<(), String> {
		if self.width == 0 || self.height == 0 {
			return Err("Board dimensions must be > 0".to_string());
		}

		// Check blocks fit
		for (i, block) in self.blocks.iter().enumerate() {
			if block.top_left.x + block.width > self.width
				|| block.top_left.y + block.height > self.height
			{
				return Err(format!("Block {i} out of board bounds"));
			}
		}

		// Check overlaps
		let mut occupied = HashSet::new();
		for block in &self.blocks {
			for cell in block.cells() {
				if !occupied.insert(cell) {
					return Err(format!("Overlapping blocks at {cell:?}"));
				}
			}
		}

		Ok(())
	}

	/// Builds a [`Board`] from a RON string and validates it.
	pub fn from_ron_str(s: &str) -> Result<Board, String> {
		let board: Board = ron::from_str(s).map_err(|e| e.to_string())?;
		board.validate()?;
		Ok(board)
	}

	/// Loads a board from a `.ron` file on disk.
	pub fn from_ron_file(path: impl AsRef<Path>) -> Result<Board, String> {
		let path = path.as_ref();
		let s = std::fs::read_to_string(path)
			.map_err(|e| format!("failed to read board file {}: {e}", path.display()))?;
		Self::from_ron_str(&s)
	}

	/// Checks if the move is valid for the block at the given index.
	pub fn can_move(&self, index: usize, delta_x: isize, delta_y: isize) -> bool {
		if delta_x.abs() > 0 && delta_y.abs() > 0 {
			return false;
		}
		if delta_x.abs() > 2 || delta_y.abs() > 2 {
			return false;
		}

		let Some(block) = self.blocks.get(index) else {
			return false;
		};

		// if pinned, don't let anything move on a side with length greater than 1
		if self.pinned
			&& ((block.height > 1 && delta_x.abs() > 0) || (block.width > 1 && delta_y.abs() > 0))
		{
			return false;
		}

		if let Some(moved_block) = block.moved(delta_x, delta_y) {
			if moved_block.top_left.x + moved_block.width > self.width {
				return false;
			}
			if moved_block.top_left.y + moved_block.height > self.height {
				return false;
			}
			// Check overlaps
			let moved_cells = moved_block.cells();
			for (other_index, other_block) in self.blocks.iter().enumerate() {
				if other_index == index {
					continue;
				}
				if !moved_cells.is_disjoint(&other_block.cells()) {
					return false;
				}
			}
			true
		} else {
			false
		}
	}

	/// Move the block at the given index if valid.
	pub fn move_block(&mut self, index: usize, delta_x: isize, delta_y: isize) -> Result<(), String> {
		if !self.can_move(index, delta_x, delta_y) {
			return Err(format!("Invalid move for block {index}"));
		}
		let moved_block = self.blocks[index].moved(delta_x, delta_y).unwrap();
		self.blocks[index] = moved_block;
		Ok(())
	}

	/// Example: Check if the board is solved given a target block and goal cell.
	pub fn is_solved(&self, target_index: usize, goal_cell: Cell) -> bool {
		self.blocks
			.get(target_index)
			.map_or(false, |block| block.cells().contains(&goal_cell))
	}

	/// Every board reachable from this one in a single valid (unit) move, each
	/// paired with the block (in its new position) that moved to get there.
	///
	/// This enumerates the local neighborhood of the current state without
	/// committing to any move, so the caller can decide where to go based on
	/// which neighbors it has already seen.
	pub fn neighbors(&self) -> Vec<(Block, Board)> {
		// Directions to try for moves: (delta_x, delta_y)
		const DIRECTIONS: [(isize, isize); 4] = [(0, -1), (0, 1), (-1, 0), (1, 0)];

		let mut out = Vec::new();
		for index in 0..self.blocks.len() {
			for (dx, dy) in DIRECTIONS {
				if self.can_move(index, dx, dy) {
					let mut next = self.clone();
					// We unwrap here because we already validated with can_move
					next.move_block(index, dx, dy).unwrap();
					out.push((next.blocks[index].clone(), next));
				}
			}
		}
		out
	}

	pub fn board_hash(&self) -> u64 {
		let mut hasher = DefaultHasher::new();
		self.hash(&mut hasher);
		hasher.finish()
	}

	fn rectangle_lines(
		top_left: Cell,
		width: usize,
		height: usize,
		padding: f32,
		color: Color,
	) -> Lines {
		let w = (width as f32 * CELL_SIZE) - (padding * 2.0);
		let h = (height as f32 * CELL_SIZE) - (padding * 2.0);
		Lines::new([rectangle(w, h)
			.thickness(0.001)
			.color(color)
			.transform(Mat4::from_translation(vec3(padding, -padding, 0.0)))])
		.pos([
			(top_left.x as f32 * CELL_SIZE),
			-(top_left.y as f32 * CELL_SIZE),
			0.0,
		])
	}
}

const CELL_SIZE: f32 = 0.02;
const PADDING: f32 = 0.0025;

impl Reify for Board {
	fn reify(&self, _context: &Context, _tasks: impl Tasker<Self>) -> impl Element<Self> {
		Self::rectangle_lines(
			[0; 2].into(),
			self.width,
			self.height,
			-PADDING,
			rgba_linear!(1.0, 1.0, 1.0, 1.0),
		)
		.build()
		.children(self.blocks.iter().map(|b| {
			Self::rectangle_lines(b.top_left, b.width, b.height, PADDING, b.display_color()).build()
		}))
	}
}

pub fn rectangle(width: f32, height: f32) -> Line {
	Line {
		points: vec![
			LinePoint {
				point: [0.0, 0.0, 0.0].into(),
				..Default::default()
			},
			LinePoint {
				point: [width, 0.0, 0.0].into(),
				..Default::default()
			},
			LinePoint {
				point: [width, -height, 0.0].into(),
				..Default::default()
			},
			LinePoint {
				point: [0.0, -height, 0.0].into(),
				..Default::default()
			},
		],
		cyclic: true,
	}
}
