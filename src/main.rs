use board::Board;
use serde::{Deserialize, Serialize};
use stardust_xr_asteroids::{
	elements::{rgba_linear, Dial, Spatial, Text},
	project_local_resources, ClientState, Context, CustomElement, Element, FrameWarning, Migrate,
	Reify, Tasker, Transformable,
};
use stardust_xr_fusion::{client::FrameInfo, drawable::YAlign};
use state_space::StateSpace;

mod board;
mod octree;
mod state_space;

#[tokio::main(flavor = "current_thread")]
async fn main() {
	stardust_xr_asteroids::client::run::<State>(&[&project_local_resources!("res")])
		.await
		.unwrap();
}

#[derive(Serialize, Deserialize)] // Defining variables used in client
pub struct State {
	board: Board,
	states: StateSpace,
	#[serde(skip)]
	frame_warning: FrameWarning,
}
/// Path used when no board file is passed as an argument.
const DEFAULT_BOARD_PATH: &str = "boards/default.ron";

/// Loads the board from the first CLI argument if given, otherwise from
/// [`DEFAULT_BOARD_PATH`].
fn load_board() -> Board {
	let path = std::env::args()
		.nth(1)
		.unwrap_or_else(|| DEFAULT_BOARD_PATH.to_string());
	Board::from_ron_file(&path).unwrap_or_else(|e| panic!("failed to load board: {e}"))
}

impl Default for State {
	fn default() -> Self {
		let board = load_board();

		Self {
			states: StateSpace::new(board.clone()),
			board,
			frame_warning: FrameWarning::default(),
		}
	}
}

impl Migrate for State {
	type Old = Self;
}
impl ClientState for State {
	const APP_ID: &'static str = "technobaboo.StateSpaceAdventure";

	fn on_frame(&mut self, info: &FrameInfo) {
		if let Some(moved_block) = self.board.random_move() {
			self.states.add(&self.board, &moved_block);
		}
		self.states.force_direct(info);

		self.frame_warning.update(info);
	}
}
impl Reify for State {
	fn reify(&self, context: &Context, tasks: impl Tasker<Self>) -> impl Element<Self> {
		Spatial::default()
			.build()
			.child(
				self.board
					.reify_substate(context, tasks.clone(), |state: &mut Self| {
						Some(&mut state.board)
					}),
			)
			.child(
				Spatial::default()
					.pos([0.0, 0.05, 0.0])
					.build()
					.child(
						self.states
							.reify_substate(context, tasks, |state: &mut Self| {
								Some(&mut state.states)
							}),
					),
			)
			.child(
				Text::new(format!(
					"States: {}\nMoves: {}",
					self.states.state_count(),
					self.states.move_count()
				))
				.align_y(YAlign::Top)
				.pos([0.0, 0.02, 0.0])
				.character_height(0.005)
				.build(),
			)
			.child(
				Dial::create(self.states.settle_speed, |state: &mut Self, value| {
					state.states.settle_speed = value;
				})
				.thickness(0.01)
				.pos([0.05, 0.02, 0.0])
				.build()
				.child(
					Text::new(format!("{}", self.states.settle_speed))
						.character_height(0.005)
						.build(),
				),
			)
			.maybe_child(self.frame_warning.danger().then(|| {
				let (delta, real_delta) = self.frame_warning.times();
				Text::new(format!(
					"frametime suuucks: {:.2}ms server vs {:.2}ms client",
					delta * 1000.0,
					real_delta * 1000.0
				))
				.align_y(YAlign::Top)
				.color(rgba_linear!(0.75, 0.1, 0.1, 1.0))
				.pos([-0.02, 0.0, 0.0])
				.rot(glam::Quat::from_rotation_z(std::f32::consts::FRAC_PI_2))
				.character_height(0.005)
				.build()
			}))
	}
}
