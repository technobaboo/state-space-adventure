use asteroids::{elements::Spatial, ClientState, CustomElement, Migrate, Reify};
use serde::{Deserialize, Serialize};
use stardust_xr_fusion::project_local_resources;

#[tokio::main(flavor = "current_thread")]
async fn main() {
	asteroids::client::run::<State>(&[&project_local_resources!("res")]).await
}

#[derive(Default, Debug, Serialize, Deserialize)] // Defining variables used in client
pub struct State {}
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
		Spatial::default().build()
	}
}
