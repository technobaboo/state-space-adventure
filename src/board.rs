use asteroids::{
	elements::{LineExt, Lines},
	CustomElement, Reify, Transformable,
};
use serde::{Deserialize, Serialize};
use stardust_xr_fusion::drawable::{Line, LinePoint};
use std::collections::{HashMap, HashSet};

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Cell {
	pub row: usize,
	pub col: usize,
}

/// Represents a block on the board.
/// The block occupies a contiguous rectangle defined by top-left cell, width and height.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
	pub label: char,
	pub top_left: Cell,
	pub width: usize,
	pub height: usize,
}

impl Block {
	fn cells(&self) -> HashSet<Cell> {
		let mut set = HashSet::new();
		for r in self.top_left.row..self.top_left.row + self.height {
			for c in self.top_left.col..self.top_left.col + self.width {
				set.insert(Cell { row: r, col: c });
			}
		}
		set
	}

	fn moved(&self, delta_row: isize, delta_col: isize) -> Option<Block> {
		let new_row = (self.top_left.row as isize).checked_add(delta_row)?;
		let new_col = (self.top_left.col as isize).checked_add(delta_col)?;
		if new_row < 0 || new_col < 0 {
			return None;
		}
		Some(Block {
			label: self.label,
			top_left: Cell {
				row: new_row as usize,
				col: new_col as usize,
			},
			width: self.width,
			height: self.height,
		})
	}
}

/// Klotski board with dimensions as const generics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Board<const ROWS: usize, const COLS: usize> {
	blocks: HashMap<char, Block>,
}
impl<const ROWS: usize, const COLS: usize> Board<ROWS, COLS> {
	/// Creates a new board ensuring constraints are satisfied:
	/// - All blocks fit inside the board
	/// - No blocks overlap
	pub fn new(blocks: Vec<Block>) -> Result<Self, String> {
		if ROWS == 0 || COLS == 0 {
			return Err("Board dimensions must be > 0".to_string());
		}

		// Check blocks fit
		for block in &blocks {
			if block.top_left.row + block.height > ROWS || block.top_left.col + block.width > COLS {
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
			blocks: blocks.into_iter().map(|b| (b.label, b)).collect(),
		})
	}

	/// Checks if move is valid for the given block label.
	pub fn can_move(&self, label: char, delta_row: isize, delta_col: isize) -> bool {
		let block = match self.blocks.get(&label) {
			Some(b) => b,
			None => return false,
		};

		if let Some(moved_block) = block.moved(delta_row, delta_col) {
			if moved_block.top_left.row + moved_block.height > ROWS {
				return false;
			}
			if moved_block.top_left.col + moved_block.width > COLS {
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
		delta_row: isize,
		delta_col: isize,
	) -> Result<(), String> {
		if !self.can_move(label, delta_row, delta_col) {
			return Err(format!("Invalid move for block '{label}'"));
		}
		let block = self.blocks.get(&label).cloned().unwrap();
		let moved_block = block.moved(delta_row, delta_col).unwrap();
		self.blocks.insert(label, moved_block);
		Ok(())
	}

	/// Example: Check if the board is solved given a target block and goal cell.
	pub fn is_solved(&self, target_label: char, goal_cell: Cell) -> bool {
		self.blocks
			.get(&target_label)
			.map_or(false, |block| block.cells().contains(&goal_cell))
	}

	fn rectangle_lines(top_left: Cell, width: usize, height: usize, padding: f32) -> Lines {
		let w = (width as f32 * CELL_SIZE) - (padding * 2.0);
		let h = (height as f32 * CELL_SIZE) - (padding * 2.0);
		Lines::new([rectangle(w, h).thickness(0.001)]).pos([
			(top_left.col as f32 * CELL_SIZE) + padding,
			-(top_left.row as f32 * CELL_SIZE) - padding,
			0.0,
		])
	}
}

const CELL_SIZE: f32 = 0.02;

impl<const ROWS: usize, const COLS: usize> Reify for Board<ROWS, COLS> {
	fn reify(&self) -> impl asteroids::Element<Self> {
		Self::rectangle_lines(Cell::default(), COLS, ROWS, 0.0)
			.build()
			.children(
				self.blocks
					.values()
					.map(|b| Self::rectangle_lines(b.top_left, b.width, b.height, 0.0025).build()),
			)
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
