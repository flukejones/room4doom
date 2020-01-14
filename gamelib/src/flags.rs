/// The flags control some attributes of the line
pub enum LineDefFlags {
    /// Players and monsters cannot cross this line. Note that
    /// if there is no sector on the other side, they can't go through the line
    /// anyway, regardless of the flags
    Blocking = 1,
    /// Monsters cannot cross this line
    BlockMonsters = 1 << 1,
    /// The linedef's two sidedefs can have "-" as a texture,
    /// which in this case means "transparent". If this flag is not set, the
    /// sidedefs can't be transparent. A side effect of this flag is that if
    /// it is set, then gunfire (pistol, shotgun, chaingun) can go through it
    TwoSided = 1 << 2,
    /// The upper texture is pasted onto the wall from
    /// the top down instead of from the bottom up like usual.
    /// The effect is if a wall moves down, it looks like the
    /// texture is stationary and is appended to as the wall moves
    UnpegTop = 1 << 3,
    /// Lower and middle textures are drawn from the
    /// bottom up, instead of from the top down like usual
    /// The effect is if a wall moves up, it looks like the
    /// texture is stationary and is appended to as the wall moves
    UnpegBottom = 1 << 4,
    /// On the automap, this line appears in red like a normal
    /// solid wall that has nothing on the other side. This is useful in
    /// protecting secret doors and such. Note that if the sector on the other
    /// side of this "secret" line has its floor height HIGHER than the sector
    /// on the facing side of the secret line, then the map will show the lines
    /// beyond and thus give up the secret
    Secret = 1 << 5,
    /// For purposes of monsters hearing sounds and thus
    /// becoming alerted. Every time a player fires a weapon, the "sound" of
    /// it travels from sector to sector, alerting all non-deaf monsters in
    /// each new sector. This flag blocks sound traveling out of this sector
    /// through this line to adjacent sector
    BlockSound = 1 << 6,
    /// Not on AutoMap
    DontDraw = 1 << 7,
    /// Already on AutoMap
    Draw = 1 << 8,
}

#[test]
fn check_flags_enum() {
    let flag = 28; // upper and lower unpegged, twosided
    println!("Blocking, two-sided, unpeg top and bottom\n{:#018b}", 29);
    println!(
        "Twosided Masked\n{:#018b}",
        29 & LineDefFlags::TwoSided as u16
    );
    dbg!(29 & LineDefFlags::TwoSided as u16 == LineDefFlags::TwoSided as u16);
    println!("Flag: Blocking\n{:#018b}", LineDefFlags::Blocking as u16);
    println!(
        "Flag: Block Monsters\n{:#018b}",
        LineDefFlags::BlockMonsters as u16
    );
    println!("Flag: Two-sided\n{:#018b}", LineDefFlags::TwoSided as u16);
    println!("Flag: Unpeg upper\n{:#018b}", LineDefFlags::UnpegTop as u16);
    println!(
        "Flag: Unpeg lower\n{:#018b}",
        LineDefFlags::UnpegBottom as u16
    );
    println!("Flag: Secret\n{:#018b}", LineDefFlags::Secret as u16);
    println!(
        "Flag: Block sound\n{:#018b}",
        LineDefFlags::BlockSound as u16
    );
    println!(
        "Flag: Not on AutoMap yet\n{:#018b}",
        LineDefFlags::DontDraw as u16
    );
    println!(
        "Flag: Already on AutoMap\n{:#018b}",
        LineDefFlags::Draw as u16
    );
    let compare = LineDefFlags::TwoSided as u16
        | LineDefFlags::UnpegTop as u16
        | LineDefFlags::UnpegBottom as u16;
    assert_eq!(compare, flag);
}
