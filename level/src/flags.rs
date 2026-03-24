use bitflags::bitflags;

bitflags! {
    /// The flags control some attributes of the line
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct LineDefFlags: u32 {
        /// Players and monsters cannot cross this line. Note that
        /// if there is no sector on the other side, they can't go through the line
        /// anyway, regardless of the flags
        const Blocking = 1;
        /// Monsters cannot cross this line
        const BlockMonsters = 1 << 1;
        /// The linedef's two sidedefs can have "-" as a texture,
        /// which in this case means "transparent". If this flag is not set, the
        /// sidedefs can't be transparent. A side effect of this flag is that if
        /// it is set, then gunfire (pistol, shotgun, chaingun) can go through it
        const TwoSided = 1 << 2;
        /// The upper texture is pasted onto the wall from
        /// the top down instead of from the bottom up like usual.
        /// The effect is if a wall moves down, it looks like the
        /// texture is stationary and is appended to as the wall moves
        const UnpegTop = 1 << 3;
        /// Lower and middle textures are drawn from the
        /// bottom up, instead of from the top down like usual
        /// The effect is if a wall moves up, it looks like the
        /// texture is stationary and is appended to as the wall moves
        const UnpegBottom = 1 << 4;
        /// On the automap, this line appears in red like a normal
        /// solid wall that has nothing on the other side. This is useful in
        /// protecting secret doors and such. Note that if the sector on the other
        /// side of this "secret" line has its floor height HIGHER than the sector
        /// on the facing side of the secret line, then the level will show the
        /// lines beyond and thus give up the secret
        const Secret = 1 << 5;
        /// For purposes of monsters hearing sounds and thus
        /// becoming alerted. Every time a player fires a weapon, the "sound" of
        /// it travels from sector to sector, alerting all non-deaf monsters in
        /// each new sector. This flag blocks sound traveling out of this sector
        /// through this line to adjacent sector
        const BlockSound = 1 << 6;
        /// Not on AutoMap
        const UnMapped = 1 << 7;
        /// Already on AutoMap
        const Mapped = 1 << 8;
        /// BOOM: allows a single use-press to activate multiple lines
        const PassUse = 1 << 9;
    }
}

#[test]
fn check_flags_enum() {
    let flag = LineDefFlags::TwoSided | LineDefFlags::UnpegTop | LineDefFlags::UnpegBottom;
    assert_eq!(flag.bits(), 28);
    assert!(flag.contains(LineDefFlags::TwoSided));
    assert!(flag.contains(LineDefFlags::UnpegTop));
    assert!(flag.contains(LineDefFlags::UnpegBottom));
    assert!(!flag.contains(LineDefFlags::Blocking));

    let from_raw = LineDefFlags::from_bits_truncate(29);
    assert!(from_raw.contains(LineDefFlags::Blocking));
    assert!(from_raw.contains(LineDefFlags::TwoSided));
    assert!(from_raw.contains(LineDefFlags::UnpegTop));
    assert!(from_raw.contains(LineDefFlags::UnpegBottom));
}
