# Intermission screens

// TODO: there is a lot of hidden state via player status.

The intermissions screens are screens that show the end-of-level stats, end-of-episode finale, or
the end-of-game screen (with demon slide-show for Doom II).

This screen is kicked off when the gamestate changes to `GS_INTERMISSION`, where the main loop then begins
to call `WI_Ticker()`. There are multiple tickers in the game:

- Gameplay: `P_Ticker()`
- Stasusbar: `ST_Ticker()`
- The automap display: `AM_Ticker()`
- User HUD messages: `HU_Ticker()` (e.g, picked up X item)
- Screen-wipe/intermission: `WI_Ticker()`
- The end-of-game: `F_Ticker()`
- Demo playback: `D_PageTicker()`

There are two things tracking game state or affecting it see [./tickers.md](./tickers.md)
for more information:
- `gameaction`
- `gamestate`

# main loop

The main loop `G_Ticker()` runs a variety of things determinewd by game state and game action. Here we focus
on `WI_Ticker()` which is the "wipe screen" ticker. It shows the end-of-level stats, end-episode or end-game.

The wipe-screen stuff honestly `wi_stuff.c` has it's own state in `stateenum_t`:

1. NoState
2. StatCount
3. ShowNextLoc

# Intermission stats

WI_Ticker is called while game state is `GS_INTERMISSION`.

`stateenum_t(NoState, StatCount, ShowNextLoc)`

`WI_updateStats` sets the intermission screen while state is `StatCount`, then when ended sets state to `NoState`

```c
void WI_Ticker(void) {
  // counter for general background animation
  bcnt++;

  if (bcnt == 1) {
    // intermission music
    if (gamemode == commercial)
      S_ChangeMusic(mus_dm2int, true);
    else
      S_ChangeMusic(mus_inter, true);
  }

  WI_checkForAccelerate();

  switch (state) {
  case StatCount:
    if (deathmatch)
      WI_updateDeathmatchStats();
    else if (netgame)
      WI_updateNetgameStats();
    else
      WI_updateStats();
    break;

  case ShowNextLoc:
    WI_updateShowNextLoc();
    break;

  case NoState:
    WI_updateNoState();
    break;
  }
}
```

NoState -> WI_updateNoState -> G_WorldDone -> set ga_worlddone -> G_DoWorldDone


`G_DoCompleted` fills in the wminfo struct and passes to `WI_Start()`.

# Game menus

- `M_Responder()`,
- `M_Drawer()`,

---

As a crate:
- ticker, Doom related. Takes events + `Game` to access methods
- items would need to be generic, probably implement a callback
- drawing trait, takes main state? State would need traits to get stuff

`Game` will also depend on Menu ticker, so to avoid circular deps the Menu can't actually depend on `Game`.
It needs to have a trait set determined for it so things can be separated.

**Traits for Game**:
- G_LoadGame(name); 
- G_SaveGame (slot,savegamestrings[slot]);
- G_DeferedInitNew(choice,epi+1,1); / G_DeferedInitNew(nightmare,epi+1,1);
- Exit

`Game` can then pass self as trait object to Menu ticker
