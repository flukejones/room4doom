# OPL2/OPL3 FM Synthesis Emulator — Implementation Specification

Reference: DOSBox DBOPL (WAVE_TABLEMUL mode), Chocolate Doom i_oplmusic.c

## 1. Constants

```
OPLRATE          = 14318180.0 / 288.0   // ~49716 Hz
PI               = std::f64::consts::PI

WAVE_BITS        = 10
WAVE_SH          = 32 - WAVE_BITS       // 22
WAVE_MASK        = (1 << WAVE_SH) - 1   // 0x3FFFFF

LFO_SH           = WAVE_SH - 10         // 12
LFO_MAX          = 256 << LFO_SH        // 1048576

ENV_BITS         = 9
ENV_EXTRA        = 0                     // ENV_BITS - 9
ENV_MIN          = 0
ENV_MAX          = 511
ENV_LIMIT        = 384                   // silent threshold
env_silent(x)    = x >= ENV_LIMIT

RATE_SH          = 24
RATE_MASK        = (1 << RATE_SH) - 1
MUL_SH           = 16

SHIFT_KSLBASE    = 16
SHIFT_KEYCODE    = 24

MASK_KSR         = 0x10
MASK_SUSTAIN     = 0x20
MASK_VIBRATO     = 0x40
MASK_TREMOLO     = 0x80

TREMOLO_TABLE_LEN = 52
```

## 2. Static Lookup Tables (Compiled-In)

```rust
const KSL_CREATE: [u8; 16] = [64,32,24,19,16,12,11,10,8,6,5,4,3,2,1,0];
const FREQ_CREATE: [u8; 16] = [1,2,4,6,8,10,12,14,16,18,20,20,24,24,30,30];
const ATTACK_SAMPLES: [u8; 13] = [69,55,46,40,35,29,23,20,19,15,11,10,9];
const ENV_INCREASE: [u8; 13] = [4,5,6,7,8,10,12,14,16,20,24,28,32];
const VIBRATO: [i8; 8] = [1,0,1,30,-127,-128,-127,-98];
const KSL_SHIFT: [u8; 4] = [31,1,2,0];
const WAVE_BASE: [u16; 8] = [0x000,0x200,0x200,0x800,0xa00,0xc00,0x100,0x400];
const WAVE_MASK_TBL: [u16; 8] = [1023,1023,511,511,1023,1023,512,1023];
const WAVE_START: [u16; 8] = [512,0,0,0,0,512,512,256];
```

## 3. Generated Tables (init once)

### 3.1 MulTable — 384 entries, u16
```
for i in 0..384:
    s = i * 8   (as i32, can go negative in exponent)
    MulTable[i] = (0.5 + 2^(-1.0 + (255-s)/256.0) * (1 << MUL_SH)) as u16
```

### 3.2 WaveTable — 8×512 = 4096 entries, i16
```
// Sine (offsets 0x200..0x3FF positive, 0x000..0x1FF negative)
for i in 0..512:
    WaveTable[0x200+i] = (sin((i+0.5) * PI/512.0) * 4084.0) as i16
    WaveTable[0x000+i] = -WaveTable[0x200+i]

// Exponential (offsets 0x700 positive, 0x6FF..down negative)
for i in 0..256:
    WaveTable[0x700+i] = (0.5 + 2^(-1.0 + (255-i*8)/256.0) * 4085.0) as i16
    WaveTable[0x6FF-i] = -WaveTable[0x700+i]

// Silence fills
for i in 0..256:
    WaveTable[0x400+i] = WaveTable[0]
    WaveTable[0x500+i] = WaveTable[0]
    WaveTable[0x900+i] = WaveTable[0]
    WaveTable[0xC00+i] = WaveTable[0]
    WaveTable[0xD00+i] = WaveTable[0]

// Replicated waveforms
    WaveTable[0x800+i] = WaveTable[0x200+i]     // half-sine
    WaveTable[0xA00+i] = WaveTable[0x200+i*2]   // double-speed pos
    WaveTable[0xB00+i] = WaveTable[0x000+i*2]   // double-speed neg
    WaveTable[0xE00+i] = WaveTable[0x200+i*2]   // double-speed pos
    WaveTable[0xF00+i] = WaveTable[0x200+i*2]   // double-speed pos
```

### 3.3 KslTable — 8×16 = 128 entries, u8
```
for oct in 0..8:
    base = oct * 8
    for i in 0..16:
        val = max(0, base - KSL_CREATE[i])
        KslTable[oct*16+i] = (val * 4) as u8
```

### 3.4 TremoloTable — 52 entries, u8, triangle wave
```
for i in 0..26:
    TremoloTable[i] = i as u8
    TremoloTable[51-i] = i as u8
```

## 4. EnvelopeSelect — rate index/shift decomposition

```rust
fn envelope_select(val: u8) -> (usize, u32) {  // (index, shift)
    if val < 52 {        // rates 0-12
        ((val & 3) as usize, 12 - (val >> 2) as u32)
    } else if val < 60 { // rates 13-14
        ((val - 48) as usize, 0)
    } else {              // rate 15+
        (12, 0)
    }
}
```

## 5. Chip::setup(rate: u32) — Rate Table Initialization

```
scale = OPLRATE / rate

noise_add = round(scale * (1 << LFO_SH))
lfo_add   = round(scale * (1 << LFO_SH))

// Frequency multiplier table
freq_scale = round(scale * (1 << (WAVE_SH - 1 - 10)))   // = scale * 2048
for i in 0..16:
    freq_mul[i] = freq_scale * FREQ_CREATE[i] as u32

// Linear rates (decay/release)
for i in 0..76:
    (idx, sh) = envelope_select(i)
    linear_rates[i] = (scale * (ENV_INCREASE[idx] << (RATE_SH - sh - 3)) as f64) as u32

// Attack rates (iterative best-fit, indices 0..61)
for i in 0..62:
    (idx, sh) = envelope_select(i)
    original = ((ATTACK_SAMPLES[idx] << sh) as f64 / scale) as i32
    guess = (scale * (ENV_INCREASE[idx] << (RATE_SH - sh - 3)) as f64) as u32
    best = guess; best_diff = 1<<30

    for _ in 0..16:
        simulate attack: volume starts at ENV_MAX, each step:
            count += guess; change = count >> RATE_SH; count &= RATE_MASK
            if change: volume += (!volume * change) >> 3
        diff = original - samples_taken
        if |diff| < best_diff: best = guess; best_diff = |diff|
        adjust guess proportionally

    attack_rates[i] = best

// Instant attack for rates 62..75
for i in 62..76:
    attack_rates[i] = 8 << RATE_SH
```

## 6. Data Structures

### 6.1 Operator
```rust
struct Operator {
    // Waveform
    wave_base: usize,       // index into WaveTable
    wave_mask: u32,
    wave_start: u32,        // initial wave_index on keyon (shifted by WAVE_SH)
    wave_index: u32,        // phase accumulator
    wave_add: u32,          // base freq increment (no vibrato)
    wave_current: u32,      // wave_add + vibrato, set each block

    // Channel data (shared with channel)
    chan_data: u32,          // bits[0:9]=freq, [10:17]=block, [16:23]=ksl_base, [24:31]=key_code

    // Frequency
    freq_mul: u32,          // from chip freq_mul table
    vibrato: u32,           // scaled vibrato delta
    vib_strength: u8,       // freq >> 7

    // Envelope
    state: EnvState,        // Off, Release, Sustain, Decay, Attack
    sustain_level: i32,     // level where decay→sustain
    total_level: i32,       // static attenuation (TL + KSL)
    current_level: u32,     // total_level + tremolo (per-block)
    volume: i32,            // current envelope (0=loud, 511=silent)
    attack_add: u32,
    decay_add: u32,
    release_add: u32,
    rate_index: u32,        // fractional rate accumulator
    rate_zero: u8,          // bitmask: bit N set = state N has zero rate

    // Keyon
    key_on: u8,             // bitmask of keyon sources

    // Cached registers
    reg20: u8, reg40: u8, reg60: u8, reg80: u8, reg_e0: u8,

    // Modulation masks
    tremolo_mask: u8,       // 0xFF if tremolo enabled, 0x00 otherwise
    ksr: u8,                // cached KSR value
}
```

### 6.2 Channel
```rust
struct Channel {
    op: [Operator; 2],
    chan_data: u32,
    old: [i32; 2],          // feedback history
    feedback: u8,           // shift amount (2..8, or 31=disabled)
    reg_b0: u8,
    reg_c0: u8,
    four_mask: u8,
    mask_left: i8,          // -1 or 0
    mask_right: i8,         // -1 or 0
    synth_mode: SynthMode,  // which synthesis to use
}
```

### 6.3 Chip
```rust
struct Chip {
    chan: [Channel; 18],
    freq_mul: [u32; 16],
    linear_rates: [u32; 76],
    attack_rates: [u32; 76],

    lfo_counter: u32,
    lfo_add: u32,
    noise_counter: u32,
    noise_add: u32,
    noise_value: u32,

    vibrato_index: u8,
    tremolo_index: u8,
    vibrato_sign: i8,       // -1 or 0
    vibrato_shift: u8,
    vibrato_strength: u8,   // 0=deep, 1=shallow
    tremolo_value: u8,
    tremolo_strength: u8,   // 0=deep, 2=shallow

    wave_form_mask: u8,     // 0x7 or 0x0
    opl3_active: bool,
    reg08: u8,
    reg_bd: u8,
    reg104: u8,
}
```

### 6.4 EnvState
```rust
enum EnvState { Off=0, Release=1, Sustain=2, Decay=3, Attack=4 }
```

### 6.5 SynthMode
```rust
enum SynthMode { Sm2FM, Sm2AM, Sm2Percussion, Sm3FM, Sm3AM, Sm3FMFM, Sm3AMFM, Sm3FMAM, Sm3AMAM, Sm3Percussion }
```

## 7. Operator Register Handlers

### Write20 (reg 0x20+offset): tremolo/vibrato/sustain/KSR/freq_mul
```
tremolo_mask = if bit7: 0xFF else 0x00
If KSR bit changed: update_rates()
If sustain set OR release_add==0: rate_zero |= (1<<SUSTAIN)
  else: rate_zero &= !(1<<SUSTAIN)
If freq_mul or vibrato bits changed:
    freq_mul = chip.freq_mul[val & 0xF]
    update_frequency()
```

### Write40 (reg 0x40+offset): KSL / total level
```
total_level = (val & 0x3F) << (ENV_BITS - 7)   // = tl << 2
total_level += (ksl_base << ENV_EXTRA) >> KSL_SHIFT[val >> 6]
```

### Write60 (reg 0x60+offset): attack rate / decay rate
```
If low nibble changed: update_decay()
If high nibble changed: update_attack()
```

### Write80 (reg 0x80+offset): sustain level / release rate
```
sustain = val >> 4
sustain |= (sustain + 1) & 0x10   // maps 0xF→0x1F
sustain_level = sustain << 4       // (ENV_BITS - 5)
If release rate changed: update_release()
```

### WriteE0 (reg 0xE0+offset): waveform select
```
wave = val & (if opl3: 0x7 else: 0x3 & wave_form_mask else: 0)
wave_base = WAVE_BASE[wave]
wave_start = WAVE_START[wave] << WAVE_SH
wave_mask = WAVE_MASK_TBL[wave]
```

## 8. Rate Update Functions

### update_attack
```
rate = reg60 >> 4
if rate: attack_add = attack_rates[(rate<<2) + ksr]; clear rate_zero ATTACK bit
else: attack_add = 0; set rate_zero ATTACK bit
```

### update_decay
```
rate = reg60 & 0xF
if rate: decay_add = linear_rates[(rate<<2) + ksr]; clear rate_zero DECAY bit
else: decay_add = 0; set rate_zero DECAY bit
```

### update_release
```
rate = reg80 & 0xF
if rate: release_add = linear_rates[(rate<<2) + ksr]
    clear RELEASE bit; if !(reg20 & MASK_SUSTAIN): clear SUSTAIN bit
else: release_add = 0
    set RELEASE bit; if !(reg20 & MASK_SUSTAIN): set SUSTAIN bit
```

### update_rates
```
new_ksr = (chan_data >> SHIFT_KEYCODE) & 0xFF
if !(reg20 & MASK_KSR): new_ksr >>= 2
if ksr == new_ksr: return
ksr = new_ksr
update_attack(); update_decay(); update_release()
```

## 9. Operator Frequency Update
```
freq = chan_data & 0x3FF
block = (chan_data >> 10) & 0xFF
wave_add = (freq << block) * freq_mul

if vibrato enabled:
    vib_strength = (freq >> 7) as u8
    vibrato = (vib_strength << block) * freq_mul
else:
    vib_strength = 0; vibrato = 0
```

## 10. Channel Register Handlers

### WriteA0: frequency low byte
```
four_op = reg104 & opl3_active & four_mask
if four_op > 0x80: return (secondary 4-op)
change = (chan_data ^ val) & 0xFF
if change: chan_data ^= change; update_frequency()
```

### WriteB0: key on/off, block, freq high
```
change = (chan_data ^ (val<<8)) & 0x1F00
if change: chan_data ^= change; update_frequency()
if bit 5 changed:
    if keyon: key_on both ops (mask 0x1)
    else: key_off both ops (mask 0x1)
```

### WriteC0: feedback/connection
```
feedback = (val>>1) & 7
if feedback: feedback = 9 - feedback   // shift 2..8
else: feedback = 31                     // disabled

OPL2 mode:
    if val & 1: synth = Sm2AM
    else: synth = Sm2FM

OPL3 stereo:
    mask_left = if val & 0x10: -1 else: 0
    mask_right = if val & 0x20: -1 else: 0
```

## 11. SetChanData
```
change = chan_data ^ data
chan_data = data
op[0].chan_data = data
op[1].chan_data = data
update_frequency(op[0]); update_frequency(op[1])
if change in ksl_base bits: update_attenuation(both ops)
if change in key_code bits: update_rates(both ops)
```

## 12. Channel::update_frequency
```
data = chan_data & 0xFFFF
ksl_base = KSL_TABLE[data >> 6]
key_code = (data & 0x1C00) >> 9
if reg08 & 0x40: key_code |= (data & 0x100) >> 8
else: key_code |= (data & 0x200) >> 9
data |= (key_code << SHIFT_KEYCODE) | (ksl_base << SHIFT_KSLBASE)
set_chan_data(data)
```

## 13. Envelope State Machine (TemplateVolume)

### Attack
```
change = rate_forward(attack_add)
if change == 0: return vol
vol += (!vol * change) >> 3   // exponential curve toward 0
if vol < 0: vol=0; state=Decay; rate_index=0
```

### Decay
```
vol += rate_forward(decay_add)
if vol >= sustain_level:
    if vol >= ENV_MAX: vol=ENV_MAX; state=Off
    else: state=Sustain; rate_index=0
```

### Sustain
```
if reg20 & MASK_SUSTAIN: return vol  // hold
// else fall through to release behavior:
vol += rate_forward(release_add)
if vol >= ENV_MAX: state=Off
```

### Release
```
vol += rate_forward(release_add)
if vol >= ENV_MAX: state=Off
```

## 14. Sample Generation

### Prepare (once per block per active operator)
```
current_level = total_level + (tremolo_value & tremolo_mask)
wave_current = wave_add
if vib_strength >> vibrato_shift:
    add = vibrato >> vibrato_shift
    add = (add ^ vibrato_sign) - vibrato_sign   // negate if sign=-1
    wave_current += add
```

### ForwardVolume
```
return current_level + vol_handler()   // vol_handler calls template_volume
```

### ForwardWave
```
wave_index += wave_current
return wave_index >> WAVE_SH
```

### GetWave (WAVE_TABLEMUL)
```
sample = WAVE_TABLE[wave_base + (index & wave_mask)]
mul = MUL_TABLE[vol]   // vol range 0..383
return (sample as i32 * mul as i32) >> MUL_SH
```

### GetSample
```
vol = forward_volume()
if env_silent(vol): wave_index += wave_current; return 0
index = forward_wave()
index += modulation   // phase modulation from feedback/carrier
return get_wave(index, vol)
```

## 15. Block Synthesis (OPL2 mode: Sm2FM, Sm2AM)

### Per sample:
```
// Feedback
mod = (old[0] + old[1]) as u32 >> feedback
old[0] = old[1]
old[1] = op[0].get_sample(mod as i32)
out0 = old[0]

// FM: carrier modulated by modulator
Sm2FM: sample = op[1].get_sample(out0)

// AM: additive
Sm2AM: sample = out0 + op[1].get_sample(0)

// Output
output[i] += sample
```

### Silent check (skip generation)
```
Sm2FM: skip if op[1] is silent
Sm2AM: skip if op[0] AND op[1] both silent
On skip: old[0]=0; old[1]=0
```

## 16. ForwardLFO
```
vibrato_sign = VIBRATO[vibrato_index >> 2] >> 7
vibrato_shift = (VIBRATO[vibrato_index >> 2] & 7) + vibrato_strength
tremolo_value = TREMOLO_TABLE[tremolo_index] >> tremolo_strength

todo = LFO_MAX - lfo_counter
count = ceil(todo / lfo_add)
if count > samples:
    count = samples
    lfo_counter += count * lfo_add
else:
    lfo_counter += count * lfo_add
    lfo_counter &= LFO_MAX - 1
    vibrato_index = (vibrato_index + 1) & 31
    tremolo_index = (tremolo_index + 1) % TREMOLO_TABLE_LEN
return count
```

## 17. GenerateBlock2 (OPL2 mono)
```
while total > 0:
    samples = forward_lfo(total)
    clear output[0..samples]
    for each of 9 channels:
        synth_handler(channel, chip, samples, output)
    total -= samples
    output advance by samples
```

**IMPORTANT**: When calling from a per-sample MIDI loop (total=1), forward_lfo
can return 0 if lfo_counter == LFO_MAX. Guard: `count = max(1, count)`.

## 18. Register Write Dispatch

```
match (reg >> 4) & 0xF:
    0x0: handle 0x01 (waveform enable), 0x08 (notesel)
    0x1: ignored
    0x2,0x3: operator Write20, index = ((reg>>3)&0x20) | (reg&0x1F)
    0x4,0x5: operator Write40
    0x6,0x7: operator Write60
    0x8,0x9: operator Write80
    0xA: channel WriteA0, index = ((reg>>4)&0x10) | (reg&0xF)
    0xB: if reg==0xBD: WriteBD, else channel WriteB0
    0xC: channel WriteC0
    0xD: ignored
    0xE,0xF: operator WriteE0
```

## 19. Operator Index Mapping (register offset → channel/op)

For operator registers (0x20-0x35, 0x40-0x55, etc.):
```
index = ((reg >> 3) & 0x20) | (reg & 0x1F)

group = index / 8
slot = index % 8
if slot >= 6 || group % 4 == 3: invalid
ch_num = group * 3 + slot % 3
op_num = slot / 3   // 0=modulator, 1=carrier
if ch_num >= 12: ch_num += 4   // second bank gap
```

For channel registers (0xA0-0xA8, 0xB0-0xB8, 0xC0-0xC8):
```
index = ((reg >> 4) & 0x10) | (reg & 0xF)
ch = index (0..8 for first bank, 16..24 for second)
```

## 20. KeyOn / KeyOff

### KeyOn(mask)
```
if key_on == 0:         // first keyon
    wave_index = wave_start
    rate_index = 0
    state = Attack
key_on |= mask
```

### KeyOff(mask)
```
key_on &= !mask
if key_on == 0 && state != Off:
    state = Release
```

## 21. SDL2 Integration

The OPL2 emulator generates i32 mono samples. The SDL2 audio callback
requests i16 samples. The integration layer:

1. Opens SDL2 audio with desired spec (44100 Hz, i16, mono or stereo)
2. In the callback, for each sample:
   a. Advance MIDI timing, process events when tick boundary crossed
   b. Call chip.generate_block_2(1, &mut [i32; 1])
   c. Scale by volume: scaled = (sample * volume) / 128
   d. Clamp to i16 range
3. Write clamped value to SDL2 output buffer

The generate_block_2(1) per-sample approach works correctly if forward_lfo
is guarded against returning 0. Alternative: batch generate into a ring buffer
and interleave MIDI events, but per-sample is simpler and correct.

## 22. Defaults

### Operator defaults
```
state = Off, volume = ENV_MAX, sustain_level = ENV_MAX
total_level = ENV_MAX, current_level = ENV_MAX
rate_zero = 1 << Off, all rates = 0, all regs = 0
wave_base = WAVE_BASE[0], wave_mask = WAVE_MASK_TBL[0]
wave_start = WAVE_START[0] << WAVE_SH
```

### Channel defaults
```
synth_mode = Sm2FM, feedback = 31, mask_left = -1, mask_right = -1
old = [0, 0], all regs = 0
```

### Chip defaults
```
noise_value = 1, all other counters/indices = 0
vibrato_strength = 1 (shallow), tremolo_strength = 2 (shallow)
wave_form_mask = 0, opl3_active = false
```
