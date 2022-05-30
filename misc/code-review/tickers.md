There are multiple tickers in the game:

- Gameplay: `P_Ticker()`
- Stasusbar: `ST_Ticker()`
- The automap display: `AM_Ticker()`
- User HUD messages: `HU_Ticker()` (e.g, picked up X item)
- Screen-wipe/intermission: `WI_Ticker()`
- The end-of-game: `F_Ticker()`
- Demo playback: `D_PageTicker()`

Each of these is called according to the game state set. Additionally to these there
are game actions - these select a particular action tha tthe game must do, for example
new-game, load/save-game, load-level.