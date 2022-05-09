use game_traits::MachinationTrait;

/// Blob of various tickers required during gameplay, this exists mostly to pass things
/// around as some functions can end up with quite a few args
pub struct Machinations<I, S>
where
    I: MachinationTrait,
    S: MachinationTrait,
{
    // statusbar
    pub statusbar: S,
    // update the automap display info
    // AM_Ticker();
    // update the HUD statuses (things like timeout displayed messages)
    // HU_Ticker();
    // Screen wipe and intermission - WI_Ticker calls world_done()
    pub intermission: I,
    // Show the finale screen
    // F_Ticker();
    // Demo run + info show
    // D_PageTicker();
}
