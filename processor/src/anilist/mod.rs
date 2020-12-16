use crate::state::State;

pub mod client;
pub mod schedule;
pub mod shows;

async fn wait_for_grace_period(state: &State<'_>) {
    // If the process repeatedly crashes, we don't want to put pressure on the anilist
    // servers even if a reload is due. Don't poll anilist during the first minute of
    // the process lifetime.
    tokio::time::delay_until(
        state.startup_time + state.config.anilist.startup_grace_period,
    )
    .await;
}
