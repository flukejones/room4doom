//! A `GameSubsystem` is a self-contained entity.
//!
//! These implement `SubsystemTrait` to initialise state, tick (update self),
//! and draw. The trait's methods provide access to `GameTraits` and the
//! `PixelBuf`.

use gamestate_traits::SubsystemTrait;

/// Blob of various tickers required during gameplay, this exists mostly to pass
/// things around as some functions can end up with quite a few args
pub struct GameSubsystem<I, S, H, F>
where
    I: SubsystemTrait,
    S: SubsystemTrait,
    H: SubsystemTrait,
    F: SubsystemTrait,
{
    /// Shows the players current status, updated every tick
    pub statusbar: S,
    // update the automap display info
    // AM_Ticker();
    // update the HUD statuses (things like timeout displayed messages)
    pub hud_msgs: H,
    /// Screen wipe and intermission - WI_Ticker calls world_done()
    pub intermission: I,
    // Show the finale screen
    pub finale: F,
    // Demo run + info show
    // D_PageTicker();
}
