use crate::InfoType;

#[rustfmt::skip]
pub fn state_to_string(name: &str, info: &InfoType) -> String {
    format!(
        r#"
    State {{ // {name}
        sprite: SpriteNum::{sprite},
        frame: {frame},
        tics: {tics},
        action: {action},
        next_state: {next_state},
        misc1: {misc1},
        misc2: {misc2},
    }},"#,
        sprite = info.get("sprite").expect("State requires sprite name"),
        frame = info.get("frame").expect("State requires frame"),
        tics = info.get("tics").expect("State requires tics"),
        action = info.get("action").expect("State requires action"),
        next_state = info
            .get("next_state")
            .expect("State requires next_state name"),
        misc1 = info.get("misc1").unwrap_or(&"0".to_string()),
        misc2 = info.get("misc2").unwrap_or(&"0".to_string()),
    )
}

pub fn info_to_string(name: &str, info: &InfoType) -> String {
    format!(
        r#"
    // {name}
    MapObjInfo {{
        doomednum: {doomednum},
        spawnstate: {spawnstate},
        spawnhealth: {spawnhealth},
        seestate: {seestate},
        seesound: {seesound},
        reactiontime: {reactiontime},
        attacksound: {attacksound},
        painstate: {painstate},
        painchance: {painchance},
        painsound: {painsound},
        meleestate: {meleestate},
        missilestate: {missilestate},
        deathstate: {deathstate},
        xdeathstate: {xdeathstate},
        deathsound: {deathsound},
        speed: {speed},
        radius: {radius},
        height: {height},
        mass: {mass},
        damage: {damage},
        activesound: {activesound},
        flags: {flags},
        raisestate: {raisestate},
    }},"#,
        doomednum = info.get("doomednum").unwrap_or(&"-1".to_string()),
        spawnstate = info
            .get("spawnstate")
            .map(|n| if n == "0" { "StateNum::None" } else { n })
            .unwrap_or("StateNum::None"),
        spawnhealth = info.get("spawnhealth").unwrap_or(&"0".to_string()),
        seestate = info
            .get("seestate")
            .map(|n| if n == "0" { "StateNum::None" } else { n })
            .unwrap_or("StateNum::None"),
        seesound = info
            .get("seesound")
            .map(|n| if n == "0" { "SfxName::None" } else { n })
            .unwrap_or("SfxName::None"),
        reactiontime = info.get("reactiontime").unwrap_or(&"8".to_string()),
        attacksound = info
            .get("attacksound")
            .map(|n| if n == "0" { "SfxName::None" } else { n })
            .unwrap_or("SfxName::None"),
        painstate = info
            .get("painstate")
            .map(|n| if n == "0" { "StateNum::None" } else { n })
            .unwrap_or("StateNum::None"),
        painchance = info.get("painchance").unwrap_or(&"0".to_string()),
        painsound = info
            .get("painsound")
            .map(|n| if n == "0" { "SfxName::None" } else { n })
            .unwrap_or("SfxName::None"),
        meleestate = info
            .get("meleestate")
            .map(|n| if n == "0" { "StateNum::None" } else { n })
            .unwrap_or("StateNum::None"),
        missilestate = info
            .get("missilestate")
            .map(|n| if n == "0" { "StateNum::None" } else { n })
            .unwrap_or("StateNum::None"),
        deathstate = info
            .get("deathstate")
            .map(|n| if n == "0" { "StateNum::None" } else { n })
            .unwrap_or("StateNum::None"),
        xdeathstate = info
            .get("xdeathstate")
            .map(|n| if n == "0" { "StateNum::None" } else { n })
            .unwrap_or("StateNum::None"),
        deathsound = info
            .get("deathsound")
            .map(|n| if n == "0" { "SfxName::None" } else { n })
            .unwrap_or("SfxName::None"),
        speed = info
            .get("speed")
            .map(|n| if !n.contains(".0") {
                format!("{n}.0")
            } else {
                n.to_string()
            })
            .map(|n| if n == "0" { "0.0".to_string() } else { n })
            .unwrap_or_else(|| "0.0".to_string()),
        radius = info.get("radius").unwrap_or(&"20.0".to_string()),
        height = info.get("height").unwrap_or(&"16.0".to_string()),
        mass = info.get("mass").unwrap_or(&"100".to_string()),
        damage = info.get("damage").unwrap_or(&"0".to_string()),
        activesound = info
            .get("activesound")
            .map(|n| if n == "0" { "SfxName::None" } else { n })
            .unwrap_or("SfxName::None"),
        flags = info.get("flags").unwrap_or(&"0".to_string()),
        raisestate = info
            .get("raisestate")
            .map(|n| if n == "0" { "StateNum::None" } else { n })
            .unwrap_or("StateNum::None"),
    )
}

#[cfg(test)]
mod tests {
    use crate::{parse_data, read_file};
    use std::path::PathBuf;

    #[test]
    fn test_info() {
        let data = read_file(PathBuf::from("multigen.txt"));
        let data = parse_data(&data);

        let plasma = data.mobj_info.get("MT_PLASMA").unwrap();
        assert_eq!(plasma.get("spawnstate").unwrap(), "StateNum::PLASBALL");
        assert_eq!(plasma.get("deathstate").unwrap(), "StateNum::PLASEXP");

        let mobj = data.mobj_info.get("MT_MISC0").unwrap();
        assert_eq!(mobj.get("doomednum").unwrap(), "2018");
        assert_eq!(mobj.get("spawnstate").unwrap(), "StateNum::ARM1");

        let mobj = data.mobj_info.get("MT_MISC12").unwrap();
        assert_eq!(mobj.get("doomednum").unwrap(), "2013");
        assert_eq!(mobj.get("spawnstate").unwrap(), "StateNum::SOUL");

        let mobj = data.mobj_info.get("MT_INV").unwrap();
        assert_eq!(mobj.get("doomednum").unwrap(), "2022");
        assert_eq!(mobj.get("spawnstate").unwrap(), "StateNum::PINV");

        let mobj = data.mobj_info.get("MT_MISC17").unwrap();
        assert_eq!(mobj.get("doomednum").unwrap(), "2048");
        assert_eq!(mobj.get("spawnstate").unwrap(), "StateNum::AMMO");

        let mobj = data.mobj_info.get("MT_MISC26").unwrap();
        assert_eq!(mobj.get("spawnstate").unwrap(), "StateNum::CSAW");

        let mobj = data.mobj_info.get("MT_MISC54").unwrap();
        assert_eq!(mobj.get("spawnstate").unwrap(), "StateNum::MEAT4");

        let mobj = data.mobj_info.get("MT_BABY").unwrap();
        assert_eq!(mobj.get("spawnstate").unwrap(), "StateNum::BSPI_STND");
        assert_eq!(mobj.get("seestate").unwrap(), "StateNum::BSPI_SIGHT");
        assert_eq!(mobj.get("painstate").unwrap(), "StateNum::BSPI_PAIN");
        assert_eq!(mobj.get("painchance").unwrap(), "128");
        assert_eq!(mobj.get("radius").unwrap(), "64.0");
        assert_eq!(mobj.get("height").unwrap(), "64.0");
        assert_eq!(mobj.get("mass").unwrap(), "600");
        assert_eq!(mobj.get("raisestate").unwrap(), "StateNum::BSPI_RAISE1");
    }

    #[test]
    fn test_states() {
        let data = read_file(PathBuf::from("multigen.txt"));
        let data = parse_data(&data);

        let state = data.states.get("S_BOSS_PAIN").unwrap();
        assert_eq!(state.get("frame").unwrap(), "7");
        assert_eq!(state.get("sprite").unwrap(), "BOSS");
        assert_eq!(state.get("next_state").unwrap(), "StateNum::BOSS_PAIN2");

        let state = data.states.get("S_BOSS_STND").unwrap();
        assert_eq!(state.get("sprite").unwrap(), "BOSS");
        assert_eq!(state.get("frame").unwrap(), "0");
        assert_eq!(state.get("next_state").unwrap(), "StateNum::BOSS_STND2");

        let state = data.states.get("S_BOS2_STND").unwrap();
        assert_eq!(state.get("sprite").unwrap(), "BOS2");
        assert_eq!(state.get("frame").unwrap(), "0");
        assert_eq!(state.get("next_state").unwrap(), "StateNum::BOS2_STND2");

        let state = data.states.get("S_BOS2_STND2").unwrap();
        assert_eq!(state.get("sprite").unwrap(), "BOS2");
        assert_eq!(state.get("frame").unwrap(), "1");
        assert_eq!(state.get("next_state").unwrap(), "StateNum::BOS2_STND");

        let state = data.states.get("S_TROO_RUN4").unwrap();
        assert_eq!(state.get("sprite").unwrap(), "TROO");
        assert_eq!(state.get("frame").unwrap(), "1");
        assert_eq!(state.get("tics").unwrap(), "3");
        assert_eq!(state.get("action").unwrap(), "ActFn::A(a_chase)");
        assert_eq!(state.get("next_state").unwrap(), "StateNum::TROO_RUN5");

        let state = data.states.get("S_BRBALL1").unwrap();
        assert_eq!(state.get("sprite").unwrap(), "BAL7");
        assert_eq!(state.get("frame").unwrap(), "32768");
        assert_eq!(state.get("next_state").unwrap(), "StateNum::BRBALL2");

        let state = data.states.get("S_BRBALL2").unwrap();
        assert_eq!(state.get("sprite").unwrap(), "BAL7");
        assert_eq!(state.get("frame").unwrap(), "32769");
    }
}
