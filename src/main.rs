use board::Board;
use glam::{Quat, Vec3A};
use mint::{Quaternion, Vector3};
use serde::{Deserialize, Serialize};
use stardust_xr_asteroids::{
	elements::{rgba_linear, Dial, Pen, PenState, Spatial, Text},
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
	states: StateSpace,
	#[serde(skip)]
	frame_warning: FrameWarning,

	#[serde(skip, default = "pen_home")]
	pen_pos: Vector3<f32>,
	#[serde(skip, default = "pen_rot")]
	pen_rot: Quaternion<f32>,
}
fn pen_home() -> Vector3<f32> {
	[-0.015, -0.075, 0.0].into()
}
fn pen_rot() -> Quaternion<f32> {
	Quat::IDENTITY.into()
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
		Self {
			states: StateSpace::new(load_board()),
			frame_warning: FrameWarning::default(),
			pen_pos: pen_home(),
			pen_rot: pen_rot(),
		}
	}
}

impl Migrate for State {
	type Old = Self;
}
impl ClientState for State {
	const APP_ID: &'static str = "technobaboo.StateSpaceAdventure";

	fn on_frame(&mut self, info: &FrameInfo) {
		self.states.explore();
		self.states.force_direct(info);

		self.frame_warning.update(info);
	}
}
impl Reify for State {
	fn reify(&self, context: &Context, tasks: impl Tasker<Self>, _props: ()) -> impl Element<Self> {
		Spatial::default()
			.build()
			.child(
				self.states
					.board()
					.view(|state: &mut Self, i, pos| state.states.drag(i, pos)),
			)
			.child(
				self.states
					.reify_substate(context, tasks, (), |state: &mut Self| {
						Some(&mut state.states)
					}),
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
			.child(
				Pen::new(
					self.pen_pos,
					self.pen_rot,
					|state: &mut Self, pen, pos, rot| {
						(state.pen_pos, state.pen_rot) = match pen {
							PenState::Floating => (pen_home(), pen_rot()),
							_ => (pos, rot),
						};
						if let PenState::StartedDrawing(_) | PenState::Drawing(_) = pen {
							state.states.snap(Vec3A::from(pos));
						}
					},
				)
				.build(),
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
