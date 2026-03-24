# Sound

Starting off first with the functions as the names provide context.

- `S_Init`, initialises sound on game start
- `S_Start`, sets up sound at level start
- `S_StartSound` (basically a wrapper of `S_StartSoundAtVolume`)
- `S_StopSound`
- `S_UpdateSounds`, called every frame to ensure correct sounds are playing for state
- `S_StopChannel`
- `S_SetSfxVolume`

*Music specific*

- `S_SetMusicVolume`
- `S_StartMusic`
- `S_ChangeMusic`
- `S_StopMusic`
- `S_PauseSound`, misnamed I think as it only pauses music
- `S_ResumeSound`, as above

The file `sounds.c` contains hardcoded data about the sounds and music.

```
// sfxinfo_t (is sfxinfo_struct)
// name,     singularity, priority, link, pitch, volume, data, (usefulness and lumpnum missing?)
{  "pistol", false,       64,       0,    -1,    -1,     0,    },
{ "chgun",   false,       64,       &S_sfx[sfx_pistol], 150, 0, 0 },

// musicinfo_t
// name,  lumpnum, (data and handle missing?)
{ "e1m1", 0        },
```

## OS Init part

**Linux note**: sound was using an ioctl in the original port, and there is no music playback. To get full sound SDL2 will be required (or something similar).

## Playing Sound

`S_StartSoundAtVolume` is used to play a sound. The parameters are *origin*, *sfx id*, and *volume*.

The Sfx ID is used to fetch a sound data from the cached data, of type `sfxinfo_t`. This struct contains
a number of things:

```c
struct sfxinfo_struct
{
    // up to 6-character name
    char*	name;
    // Sfx singularity (only one at a time)
    int		singularity;
    // Sfx priority
    int		priority;
    // referenced sound if a link
    sfxinfo_t*	link;
    // pitch if a link
    int		pitch;
    // volume if a link
    int		volume;
    // sound data
    void*	data;
    // this is checked every second to see if sound
    // can be thrown out (if 0, then decrement, if -1,
    // then throw out, if > 0, then it is in use)
    int		usefulness;
    // lump number of sfx
    int		lumpnum;		
};
```

there can be multiple of a sound using a single sound file but with a different pitch.

## Channels

In OG Doom the sounds are played one-per-channel.


---

i_sound.c

I_RegisterSong
I_PlaySong
S_StartSong

IsMid
ConvertMus -> mus2mid
Mix_LoadMUS

relationship: mobj-info -> state -> sound enum as index -> sfx priority and data array


Music:

.... ugh.

https://moddingwiki.shikadi.net/wiki/MUS_Format
https://moddingwiki.shikadi.net/wiki/MID_Format

The MUS/MID combined with `GENMIDI` is what gave Doom it's music. The `GENMIDI` is used to program
the OPL chip on Sound Blaster cards

https://doomwiki.org/wiki/GENMIDI
https://github.com/freedoom/freedoom/blob/master/lumps/genmidi/README.adoc

Chocolate Doom does full-on OPL emulation

GUS can use SDL2 + timidity + the patch files. Will need to parse the `DMXGUS` lump
https://www.chocolate-doom.org/wiki/index.php/GUS

`S_Start` controls music vs level start and selection

For the most part music selection is done via enum-name cast to int. Where:

```c
typedef enum
{
    mus_None,
    mus_e1m1,
    mus_e1m2,
    mus_e1m3,
    mus_e1m4,
    mus_e1m5,
    mus_e1m6,
    mus_e1m7,
    mus_e1m8,
    mus_e1m9,
    mus_e2m1,
    mus_e2m2,
    mus_e2m3,
    mus_e2m4,
    mus_e2m5,
    mus_e2m6,
    mus_e2m7,
    mus_e2m8,
    mus_e2m9,
    mus_e3m1,
    mus_e3m2,
    mus_e3m3,
    mus_e3m4,
    mus_e3m5,
    mus_e3m6,
    mus_e3m7,
    mus_e3m8,
    mus_e3m9,
    }
```

is the index in to `S_sfx [ sfxinfo_struct ]`, which `sfxinfo_struct->name` contains the lump name minus `d_` appended to it.

`musicinfo_t S_music[] =` is in the order of the above enum.
