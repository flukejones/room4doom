use game_traits::MachinationTrait;

/// Blob of various tickers required during gameplay, this exists mostly to pass things
/// around as some functions can end up with quite a few args
pub struct Machinations<I>
where
    I: MachinationTrait,
{
    // statusbar
    // ST_Ticker();
    // update the automap display info
    // AM_Ticker();
    // update the HUD statuses (things like timeout displayed messages)
    // HU_Ticker();
    // Screen wipe and intermission - WI_Ticker calls world_done()
    pub wipe: I,
    // Show the finale screen
    // F_Ticker();
    // Demo run + info show
    // D_PageTicker();
}
