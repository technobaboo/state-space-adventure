use board::{Block, Board};
use serde::{Deserialize, Serialize};
use stardust_xr_asteroids::{
	elements::{Dial, Spatial, Text},
	ClientState, CustomElement, FrameWarning, Migrate, Reify, Transformable,
};
use stardust_xr_fusion::{
	drawable::YAlign, project_local_resources, root::FrameInfo, values::color::rgba_linear,
};
use state_space::StateSpace;

mod board;
mod state_space;

#[tokio::main(flavor = "current_thread")]
async fn main() {
	stardust_xr_asteroids::client::run::<State>(&[&project_local_resources!("res")]).await
}

#[derive(Serialize, Deserialize)] // Defining variables used in client
pub struct State {
	board: Board,
	states: StateSpace,
	#[serde(skip)]
	frame_warning: FrameWarning,
}
impl Default for State {
	fn default() -> Self {
		let board = Board::new(
			7,
			7,
			vec![
				Block {
					label: 'M', // magenta
					color: rgba_linear!(0.9, 0.1, 0.6, 1.0),
					top_left: [1, 0].into(),
					width: 3,
					height: 1,
				},
				Block {
					label: 'P', // purple
					color: rgba_linear!(0.5, 0.1, 0.9, 1.0),
					top_left: [6, 0].into(),
					width: 1,
					height: 2,
				},
				Block {
					label: 'R', // red
					color: rgba_linear!(0.9, 0.2, 0.2, 1.0),
					top_left: [2, 1].into(),
					width: 1,
					height: 3,
				},
				Block {
					label: 'G', // green
					color: rgba_linear!(0.1, 0.9, 0.1, 1.0),
					top_left: [0, 2].into(),
					width: 2,
					height: 1,
				},
				Block {
					label: 'O', // orange
					color: rgba_linear!(0.9, 0.6, 0.1, 1.0),
					top_left: [0, 3].into(),
					width: 2,
					height: 1,
				},
				Block {
					label: 'T', // teal
					color: rgba_linear!(0.1, 0.8, 0.8, 1.0),
					top_left: [6, 2].into(),
					width: 1,
					height: 3,
				},
				Block {
					label: 'B', // blue
					color: rgba_linear!(0.1, 0.3, 0.9, 1.0),
					top_left: [2, 6].into(),
					width: 3,
					height: 1,
				},
			],
			true,
		)
		.unwrap();

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
	fn reify(&self) -> impl stardust_xr_asteroids::Element<Self> {
		Spatial::default()
			.build()
			.child(
				self.board
					.reify_substate(|state: &mut Self| Some(&mut state.board)),
			)
			.child(
				Spatial::default().pos([0.0, 0.05, 0.0]).build().child(
					self.states
						.reify_substate(|state: &mut Self| Some(&mut state.states)),
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
