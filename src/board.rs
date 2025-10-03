use glam::{vec3, Mat4};
use rand::{rng, seq::IteratorRandom};
use serde::{Deserialize, Serialize};
use stardust_xr_asteroids::{
	elements::{LineExt, Lines},
	CustomElement, Reify, Transformable,
};
use stardust_xr_fusion::{
	drawable::{Line, LinePoint},
	values::{color::rgba_linear, Color, Vector2},
};
use std::{
	collections::{HashMap, HashSet},
	hash::{DefaultHasher, Hash, Hasher},
};

pub type Cell = Vector2<usize>;

/// Represents a block on the board.
/// The block occupies a contiguous rectangle defined by top-left cell, width and height.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
	pub label: char,
	pub color: Color,
	pub top_left: Cell,
	pub width: usize,
	pub height: usize,
}
impl Block {
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
			label: self.label,
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

impl PartialEq for Block {
	fn eq(&self, other: &Self) -> bool {
		self.label == other.label
			&& self.top_left == other.top_left
			&& self.width == other.width
			&& self.height == other.height
	}
}
impl Eq for Block {}
impl Hash for Block {
	fn hash<H: Hasher>(&self, state: &mut H) {
		self.label.hash(state);
		self.top_left.hash(state);
		self.width.hash(state);
		self.height.hash(state);
	}
}

/// Klotski board with dimensions as const generics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Board {
	width: usize,
	height: usize,
	blocks: HashMap<char, Block>,
	pinned: bool,
}
impl Hash for Board {
	fn hash<H: Hasher>(&self, state: &mut H) {
		let mut blocks = self.blocks.values().collect::<Vec<_>>();
		blocks.sort_by_key(|block| block.label);
		for block in blocks {
			block.hash(state);
		}
	}
}
impl Board {
	/// Creates a new board ensuring constraints are satisfied:
	/// - All blocks fit inside the board
	/// - No blocks overlap
	pub fn new(
		width: usize,
		height: usize,
		blocks: Vec<Block>,
		pinned: bool,
	) -> Result<Self, String> {
		if width == 0 || height == 0 {
			return Err("Board dimensions must be > 0".to_string());
		}

		// Check blocks fit
		for block in &blocks {
			if block.top_left.x + block.width > width || block.top_left.y + block.height > height {
				return Err(format!("Block '{}' out of board bounds", block.label));
			}
		}

		// Check overlaps
		let mut occupied = HashSet::new();
		for block in &blocks {
			for cell in block.cells() {
				if !occupied.insert(cell) {
					return Err(format!("Overlapping blocks at {cell:?}"));
				}
			}
		}

		Ok(Board {
			width,
			height,
			blocks: blocks.into_iter().map(|b| (b.label, b)).collect(),
			pinned,
		})
	}

	/// Checks if move is valid for the given block label.
	pub fn can_move(&self, label: char, delta_x: isize, delta_y: isize) -> bool {
		if delta_x.abs() > 0 && delta_y.abs() > 0 {
			return false;
		}
		if delta_x.abs() > 2 || delta_y.abs() > 2 {
			return false;
		}

		let Some(block) = self.blocks.get(&label) else {
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
			for (other_label, other_block) in &self.blocks {
				if *other_label == label {
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

	/// Move the block if valid.
	pub fn move_block(
		&mut self,
		label: char,
		delta_x: isize,
		delta_y: isize,
	) -> Result<(), String> {
		if !self.can_move(label, delta_x, delta_y) {
			return Err(format!("Invalid move for block '{label}'"));
		}
		let block = self.blocks.get(&label).cloned().unwrap();
		let moved_block = block.moved(delta_x, delta_y).unwrap();
		self.blocks.insert(label, moved_block);
		Ok(())
	}

	/// Example: Check if the board is solved given a target block and goal cell.
	pub fn is_solved(&self, target_label: char, goal_cell: Cell) -> bool {
		self.blocks
			.get(&target_label)
			.map_or(false, |block| block.cells().contains(&goal_cell))
	}

	pub fn random_move(&mut self) -> Option<Block> {
		let mut rng = rng();

		// Directions to try for moves: (delta_x, delta_y)
		let directions: &[(isize, isize)] = &[(0, -1), (0, 1), (-1, 0), (1, 0)];

		// Generate all possible valid moves as (block_label, delta_x, delta_y)
		let mut moves = Vec::new();
		for &label in self.blocks.keys() {
			for &(dx, dy) in directions {
				if self.can_move(label, dx, dy) {
					moves.push((label, dx, dy));
				}
			}
		}

		// Pick one at random and perform it
		if let Some(&(label, dx, dy)) = moves.iter().choose(&mut rng) {
			// We unwrap here because we already validated with can_move
			self.move_block(label, dx, dy).unwrap();
			self.blocks.get(&label).cloned()
		} else {
			None
		}
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
	fn reify(&self) -> impl stardust_xr_asteroids::Element<Self> {
		Self::rectangle_lines(
			[0; 2].into(),
			self.width,
			self.height,
			-PADDING,
			rgba_linear!(1.0, 1.0, 1.0, 1.0),
		)
		.build()
		.children(self.blocks.values().map(|b| {
			Self::rectangle_lines(b.top_left, b.width, b.height, PADDING, b.color).build()
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
