use std::hash::{DefaultHasher, Hash, Hasher};

use asteroids::{
	elements::{Handle, Spatial, Text},
	ClientState, CustomElement, Migrate, Reify, Transformable,
};
use board::{Block, Board, Cell};
use serde::{Deserialize, Serialize};
use stardust_xr_fusion::{drawable::YAlign, project_local_resources, values::color::rgba_linear};

mod board;

#[tokio::main(flavor = "current_thread")]
async fn main() {
	asteroids::client::run::<State>(&[&project_local_resources!("res")]).await
}

#[derive(Debug, Serialize, Deserialize)] // Defining variables used in client
pub struct State {
	board: Board<5, 4>,
}
impl Default for State {
	fn default() -> Self {
		Self {
			board: Board::new(vec![
				// Tall red block on top-left
				Block {
					label: 'A',
					color: rgba_linear!(0.75, 0.1, 0.1, 1.0),
					top_left: Cell { row: 0, col: 0 },
					width: 1,
					height: 2,
				},
				// Large green target block at top center
				Block {
					label: 'B',
					color: rgba_linear!(0.1, 0.75, 0.1, 1.0),
					top_left: Cell { row: 0, col: 1 },
					width: 2,
					height: 2,
				},
				// Tall purple block on top-right
				Block {
					label: 'C',
					color: rgba_linear!(0.4, 0.1, 0.75, 1.0),
					top_left: Cell { row: 0, col: 3 },
					width: 1,
					height: 2,
				},
				// Tall teal block mid-left
				Block {
					label: 'D',
					color: rgba_linear!(0.1, 0.65, 0.75, 1.0),
					top_left: Cell { row: 2, col: 0 },
					width: 1,
					height: 2,
				},
				// Tall blue block mid-right
				Block {
					label: 'E',
					color: rgba_linear!(0.1, 0.4, 0.75, 1.0),
					top_left: Cell { row: 2, col: 3 },
					width: 1,
					height: 2,
				},
				// Orange horizontal block center
				Block {
					label: 'F',
					color: rgba_linear!(0.75, 0.5, 0.1, 1.0),
					top_left: Cell { row: 2, col: 1 },
					width: 2,
					height: 1,
				},
				// Pink small block bottom-left center
				Block {
					label: 'G',
					color: rgba_linear!(0.85, 0.2, 0.5, 1.0),
					top_left: Cell { row: 3, col: 1 },
					width: 1,
					height: 1,
				},
				// Green small block bottom-right center
				Block {
					label: 'H',
					color: rgba_linear!(0.4, 0.75, 0.1, 1.0),
					top_left: Cell { row: 3, col: 2 },
					width: 1,
					height: 1,
				},
				// Orange small block bottom-left corner
				Block {
					label: 'I',
					color: rgba_linear!(0.75, 0.3, 0.1, 1.0),
					top_left: Cell { row: 4, col: 0 },
					width: 1,
					height: 1,
				},
				// Green small block bottom-right corner
				Block {
					label: 'J',
					color: rgba_linear!(0.3, 0.75, 0.1, 1.0),
					top_left: Cell { row: 4, col: 3 },
					width: 1,
					height: 1,
				},
			])
			.unwrap(),
		}
	}
}

impl Migrate for State {
	type Old = Self;
}
impl ClientState for State {
	const APP_ID: &'static str = "technobaboo.StateSpaceAdventure";

	fn on_frame(&mut self, _info: &stardust_xr_fusion::root::FrameInfo) {
		self.board.random_move();
	}
}
impl Reify for State {
	fn reify(&self) -> impl asteroids::Element<Self> {
		Spatial::default().build().child(
			self.board
				.reify_substate(|state: &mut Self| Some(&mut state.board)),
		)
		// .child(
		// 	Text::new(format!("Hash: {}", {
		// 		let mut hasher = DefaultHasher::new();
		// 		self.board.hash(&mut hasher);
		// 		hasher.finish()
		// 	}))
		// 	.text_align_y(YAlign::Top)
		// 	.pos([0.0, 0.01, 0.0])
		// 	.rot(glam::Quat::from_rotation_y(std::f32::consts::PI))
		// 	.character_height(0.005)
		// 	.build(),
		// )
	}
}
