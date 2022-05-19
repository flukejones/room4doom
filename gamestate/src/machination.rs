//! A `Machination` is defined as something that can be a self-contained entity.
//! These implement the `MachinationTrait` which allows them to initialise their
//! state, tick (update self), and draw. The methods on this trait provide access
//! to `GameTraits` methods, and the `PixelBuf`.

use gamestate_traits::MachinationTrait;

/// Blob of various tickers required during gameplay, this exists mostly to pass things
/// around as some functions can end up with quite a few args
pub struct Machinations<I, S, H, F>
where
    I: MachinationTrait,
    S: MachinationTrait,
    H: MachinationTrait,
    F: MachinationTrait,
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
