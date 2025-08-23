use asteroids::{elements::Handle, ClientState, CustomElement, Migrate, Reify};
use board::{Block, Board, Cell};
use serde::{Deserialize, Serialize};
use stardust_xr_fusion::project_local_resources;

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
				// Tall 1x2 blocks on the left and right sides, and top right
				Block {
					label: 'A',
					top_left: Cell { row: 0, col: 0 },
					width: 1,
					height: 2,
				}, // red tall left top
				Block {
					label: 'B',
					top_left: Cell { row: 0, col: 1 },
					width: 2,
					height: 2,
				}, // green large 2x2 top center
				Block {
					label: 'C',
					top_left: Cell { row: 0, col: 3 },
					width: 1,
					height: 2,
				}, // purple tall top right
				Block {
					label: 'D',
					top_left: Cell { row: 2, col: 0 },
					width: 1,
					height: 2,
				}, // teal tall mid left
				Block {
					label: 'E',
					top_left: Cell { row: 2, col: 3 },
					width: 1,
					height: 2,
				}, // blue tall mid right
				// Horizontal 2x1 block center (orange)
				Block {
					label: 'F',
					top_left: Cell { row: 2, col: 1 },
					width: 2,
					height: 1,
				},
				// Small 1x1 pink and green blocks
				Block {
					label: 'G',
					top_left: Cell { row: 3, col: 1 },
					width: 1,
					height: 1,
				},
				Block {
					label: 'H',
					top_left: Cell { row: 3, col: 2 },
					width: 1,
					height: 1,
				},
				// Small 1x1 blocks bottom corners (orange left, green right)
				Block {
					label: 'I',
					top_left: Cell { row: 4, col: 0 },
					width: 1,
					height: 1,
				},
				Block {
					label: 'J',
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
}
impl Reify for State {
	// Example: Cube with grab ring on the bottom (allows user to move it)
	// and a dial on the top that allows the user to change the size of the box
	fn reify(&self) -> impl asteroids::Element<Self> {
		Handle::new([0.0; 3], |state: &mut Self, pos| {})
			.build()
			.child(
				self.board
					.reify_substate(|state: &mut Self| Some(&mut state.board)),
			)
	}
}
