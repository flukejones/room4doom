# CHAPTER [7]: Sounds and Songs
## [7-1]: D_[xxxxxx]
Songs. What format are they? Apparently the MUS format, which I have absolutely no knowledge of. But it's obvious what each song is for, from their names.

## [7-2]: DP[xxxxxx] and DS[xxxxxx]
These are the sound effects. They come in pairs - DP for pc speaker sounds, DS for sound cards.

The DS sounds are in RAW format: they have a four integer header, then the sound samples (each is 1 byte since they are 8-bit samples).

The headers' four (unsigned) integers are: 3, then 11025 (the sample rate), then the number of samples, then 0. Since the maximum number of samples is 65535, that means a little less than 6 seconds is the longest possible sound effect.
