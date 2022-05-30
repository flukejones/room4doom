Below are my notes as written while rewriting the Doom source in Rust.

I'd originally done this using Chocolate Doom source, which has a lot of added complexity in it due to supporting many Doom engined games - I'm now rewriting it using the OG Doom source. These notes need to be adjusted as the Choc Doom source makes *a lot* of fundamental architecture changes to be able to compile several games from the same base of code.

## Why?

Because I can, and it's a very good way to learn the Doom engine and how it did things.

# The call tree

The Doom (C src) call tree in detail:

`main` does nothing but call `D_DoomMain` which then does a large amount of tasks:
- parse CLI args,
- networking setup for network games,
- performs all the subsystem init (and the Linuxy style startup output with `DEH_printf`),
- game settings like key bindings,
- load IWAD,
- game version checks,
- Choc Doom addds some BFG Edition fixes here too,
- Chains to `D_DoomLoop`

There is a heck of a lot of game setup stuff being done here with CLI args.

- `D_DoomLoop`
    - `I_StartFrame` // does nothing?
    - [`TryRunTics`](#TryRunTics)
      - `G_Ticker`
        + `P_Ticker`, if game state is `GS_LEVEL`
          - `P_RunThinkers`
          - `P_UpdateSpecials`
          - `P_RespawnSpecials`
        + `WI_Ticker` if game state is `GS_INTERMISSION`
        + `F_Ticker` if game state is `GS_FINALE`
        + `D_PageTicker` if game state is `GS_DEMOSCREEN`
      - `M_Ticker`
      - `NetUpdate`
        + `D_ProcessEvents`
        + `G_BuildTiccmd`
    - `S_UpdateSounds
    - [`D_Display`](#D_Display)
      + [`I_FinishUpdate`](#I_FinishUpdate) called after [`D_Display`](#D_Display) if no screen wipe

# D_DoomLoop
- `D_DoomLoop`, main loop, never exits. Timing, I/O, ticker, drawers. I_GetTime, I_StartFrame, and I_StartTic. Calls below functions as a pre-start then calls `D_RunFrame` in a loop that never exits.
  + `G_BeginRecording`, record a demo. Stores all ticcmd
  + `I_SetWindowTitle`
  + `I_GraphicsCheckCommandLine`, sets a series of settings from CLI args
  + `I_SetGrabMouseCallback(D_GrabMouseCallback)`, control of mouse if focused (or not)
  + `I_InitGraphics`, sets up frame buffers, SDL, palette, one of the frame buffers is `I_VideoBuffer`, a 1D u8 array.
  + `EnableLoadingDisk`, show the CD or Floppy icon
  + [`TryRunTics`](#TryRunTics) initial run of tics
  + `V_RestoreBuffer` sets `dest_screen = I_VideoBuffer`, then used in video.c
  + `R_ExecuteSetViewSize` make your screen postage stamp sized (not really needed these days)
  + `D_StartGameLoop`, Called after the screen is set but before the game starts running. Is just `lasttime = GetAdjustedTime() / ticdup`
  + loop of **[`D_RunFrame`](#D_RunFrame)**
  
## D_RunFrame

Never exits. This is *the* main game loop and takes full control of your fancy 486DX2-66Mhz to bring it to it's knees.

Does two things, a screen 'wipe' if required which is the randomised columns of pixels falling down. And then the following functions:

- `I_StartFrame`, which does nothing in src. For frame sync ops?
- [`TryRunTics`](#TryRunTics), advance tics
  - `G_Ticker`
    + `P_Ticker`, if game state is `GS_LEVEL`
      - `P_RunThinkers`, main AI/movers/lights updates
      - `P_UpdateSpecials`
      - `P_RespawnSpecials`
    + `WI_Ticker` if game state is `GS_INTERMISSION`
    + `F_Ticker` if game state is `GS_FINALE`
    + `D_PageTicker` if game state is `GS_DEMOSCREEN`
- `S_UpdateSounds` update sounds for the current player of this game view (there are 4 possilbe players, each with a differnet view)
- [`D_Display`](#D_Display), this may return a bool, true if a screen wipe is required
  - [`I_FinishUpdate`](#I_FinishUpdate) called after [`D_Display`](#D_Display) if no screen wipe

### D_Display

### I_FinishUpdate
Mostly about finalising a blit to screen

# TryRunTics

**In progress**

Game tics/time, and network syncing.

If tics are too far behind rendering then the tics will run in a loop until caught up. If there are no players in the game then the loop is skipped.

It also checks per loop if a player has joined. So it looks like co-op play was possible with running a regular game.

`D_ProcessEvents` takes all incoming events, `G_BuildTiccmd` builds a tic command. The tic command is a structure that can be recorded for playback later and is how demos are acheived. 

## RunTic

This will check for if a player has quit the game, and call `PlayerQuitGame`. Then set the netcmds (tic commands) received over network to be used later, run `D_DoAdvanceDemo` if in demo mode, and lastly run [`G_Ticker`](#G_Ticker) which is the meat and potatoes of the game update.

## G_Ticker

Performs an action based on the game action from:

```C
typedef enum
{
    ga_nothing,
    ga_loadlevel,
    ga_newgame,
    ga_loadgame,
    ga_savegame,
    ga_playdemo,
    ga_completed,
    ga_victory,
    ga_worlddone,
    ga_screenshot
} gameaction_t;
```

Each member of the enum reflects the name of the function that will be called on match in the switch block slightly modified e.g, `G_DoLoadLevel()`. `ga_nothing` is used when in-game (I think). `G_Ticker` stops in a while loop calling one of these functions until the function changes the action.

A game action is not the same as a game state, which are (and is called here):

```C
    // do main actions
    switch (gamestate)
    {
    case GS_LEVEL:
        P_Ticker(); // state updater
        ST_Ticker();
        AM_Ticker();
        HU_Ticker();
        break;

    case GS_INTERMISSION:
        WI_Ticker();
        break;

    case GS_FINALE:
        F_Ticker();
        break;

    case GS_DEMOSCREEN:
        D_PageTicker();
        break;
    }
```

[`P_Ticker`](#P_Ticker) does the whole map world update cycle, running thinkers, doors, platforms, lights, spawns etc.

[`ST_Ticker](#ST_Ticker) does all bottom status bar stuff like player face updates, ammo counts, key cards etc. The statusbar is drawn later directly to the main buffer.

[`AM_Ticker`](#AM_Ticker) controls the automap. The automap is a whole self-contianed module. When it comes time to render it does so to the main buffer.

[`HU_Ticker](#HU_Ticker) the messages you see in top left.

## P_Ticker

Does not run if the game is paused, or not a netgame, demo playback, or no active player.

For each active player it calls `P_PlayerThink`. Then calls:

- `P_RunThinkers`
- `P_UpdateSpecials`
- `P_RespawnSpecials`

and lastly advances the level time (the time spent in level), which because `P_Ticker` is called in step with tics will be one unit per tic.

## G_DoNewGame

Sets a number of Game variables to defaults then calls [`G_InitNew`](#G_InitNew) with skill, episode, and map numbers.

Chains to:
- `G_InitNew` which changes game settings depending on skill and map/episode then
- `G_DoLoadLevel` after setup.

## G_DoLoadLevel

Sets starting tic for level time, sky box, a screen wipe if first game, then calls [`P_SetupLevel`](#P_SetupLevel) to set up the level.

Also sets the render view to the Doomguy you are playing as (in the event of netgame).

Reset input buffers..

## P_SetupLevel

Takes an episode, map, 0, and skill as args. This does the meat of the game setup:

- Reset player stats (shown at end of level),
- Tells the Zmalloc to purge (won't be needed with rust version),
- `P_InitThinkers`, sets the linked lists to zero,
- Loads all the data for the level,
- `P_LoadThings`
- Deathmatch setup + `G_DeathMatchSpawnPlayer()`,
- `P_SpawnSpecials`
- `R_PrecacheLevel` which may not ever really be needed with a source rewrite

## P_LoadThings

Loads the lump data to cache then chains to `P_SpawnMapThing`. It does also check if full/shareware/Doom II and set if Doom II things can spawn.

Also checks that the map has the required start points for all players.

- `P_SpawnMapThing`, validates the passed in Thing then calls either `P_SpawnMobj` or `P_SpawnPlayer`.

- `P_SpawnSpecials`
