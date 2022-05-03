use crate::strings::*;
use crate::{InfoGroupType, InfoType};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

pub fn write_info_file(ordering: &[String], info: InfoGroupType, path: PathBuf) {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path.clone())
        .unwrap_or_else(|e| panic!("Couldn't open {:?}, {}", path, e));

    file.write_all(MOBJ_INFO_HEADER_STR.as_bytes()).unwrap();
    file.write_all(MOBJ_INFO_TYPE_STR.as_bytes()).unwrap();
    file.write_all(MOBJ_INFO_ARRAY_STR.as_bytes()).unwrap();
    for key in ordering.iter() {
        let value = info.get(key).unwrap();
        let info = info_to_string(key, value);
        file.write_all(info.as_bytes()).unwrap();
    }
    file.write_all(ARRAY_END_STR.as_bytes()).unwrap();
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
            .map(|n| if n == "0" { "StateNum::S_NULL" } else { n })
            .unwrap_or("StateNum::S_NULL"),
        spawnhealth = info.get("spawnhealth").unwrap_or(&"0".to_string()),
        seestate = info
            .get("seestate")
            .map(|n| if n == "0" { "StateNum::S_NULL" } else { n })
            .unwrap_or("StateNum::S_NULL"),
        seesound = info
            .get("seesound")
            .map(|n| if n == "0" { "SfxEnum::None" } else { n })
            .unwrap_or("SfxEnum::None"),
        reactiontime = info.get("reactiontime").unwrap_or(&"8".to_string()),
        attacksound = info
            .get("attacksound")
            .map(|n| if n == "0" { "SfxEnum::None" } else { n })
            .unwrap_or("SfxEnum::None"),
        painstate = info
            .get("painstate")
            .map(|n| if n == "0" { "StateNum::S_NULL" } else { n })
            .unwrap_or("StateNum::S_NULL"),
        painchance = info.get("painchance").unwrap_or(&"0".to_string()),
        painsound = info
            .get("painsound")
            .map(|n| if n == "0" { "SfxEnum::None" } else { n })
            .unwrap_or("SfxEnum::None"),
        meleestate = info
            .get("meleestate")
            .map(|n| if n == "0" { "StateNum::S_NULL" } else { n })
            .unwrap_or("StateNum::S_NULL"),
        missilestate = info
            .get("missilestate")
            .map(|n| if n == "0" { "StateNum::S_NULL" } else { n })
            .unwrap_or("StateNum::S_NULL"),
        deathstate = info
            .get("deathstate")
            .map(|n| if n == "0" { "StateNum::S_NULL" } else { n })
            .unwrap_or("StateNum::S_NULL"),
        xdeathstate = info
            .get("xdeathstate")
            .map(|n| if n == "0" { "StateNum::S_NULL" } else { n })
            .unwrap_or("StateNum::S_NULL"),
        deathsound = info
            .get("deathsound")
            .map(|n| if n == "0" { "SfxEnum::None" } else { n })
            .unwrap_or("SfxEnum::None"),
        speed = info
            .get("speed")
            .map(|n| if !n.contains(".0") {
                format!("{n}.0")
            } else {
                n.to_string()
            })
            .map(|n| if n == "0" {
                "0.0".to_string()
            } else {
                n.to_string()
            })
            .unwrap_or_else(|| "0.0".to_string()),
        radius = info.get("radius").unwrap_or(&"20.0".to_string()),
        height = info.get("height").unwrap_or(&"16.0".to_string()),
        mass = info.get("mass").unwrap_or(&"100".to_string()),
        damage = info.get("damage").unwrap_or(&"0".to_string()),
        activesound = info
            .get("activesound")
            .map(|n| if n == "0" { "SfxEnum::None" } else { n })
            .unwrap_or("SfxEnum::None"),
        flags = info.get("flags").unwrap_or(&"0".to_string()),
        raisestate = info
            .get("raisestate")
            .map(|n| if n == "0" { "StateNum::S_NULL" } else { n })
            .unwrap_or("StateNum::S_NULL"),
    )
}

#[cfg(test)]
mod tests {
    use crate::parse_info::info_to_string;
    use crate::{parse_data, read_file};
    use std::path::PathBuf;

    #[test]
    fn test_info() {
        let data = read_file(PathBuf::from("multigen.txt.orig"));
        let (order, info) = parse_data(&data);

        let plasma = info.get("MT_PLASMA").unwrap();
        assert_eq!(plasma.get("spawnstate").unwrap(), "StateNum::S_PLASBALL");
        assert_eq!(plasma.get("deathstate").unwrap(), "StateNum::S_PLASEXP");

        let lines = info_to_string("MT_PLASMA", &plasma);

        let mobj = info.get("MT_MISC0").unwrap();
        assert_eq!(mobj.get("doomednum").unwrap(), "2018");
        assert_eq!(mobj.get("spawnstate").unwrap(), "StateNum::S_ARM1");

        let mobj = info.get("MT_MISC12").unwrap();
        assert_eq!(mobj.get("doomednum").unwrap(), "2013");
        assert_eq!(mobj.get("spawnstate").unwrap(), "StateNum::S_SOUL");

        let mobj = info.get("MT_INV").unwrap();
        assert_eq!(mobj.get("doomednum").unwrap(), "2022");
        assert_eq!(mobj.get("spawnstate").unwrap(), "StateNum::S_PINV");

        let mobj = info.get("MT_MISC17").unwrap();
        assert_eq!(mobj.get("doomednum").unwrap(), "2048");
        assert_eq!(mobj.get("spawnstate").unwrap(), "StateNum::S_AMMO");

        let mobj = info.get("MT_MISC26").unwrap();
        assert_eq!(mobj.get("spawnstate").unwrap(), "StateNum::S_CSAW");

        let mobj = info.get("MT_MISC54").unwrap();
        assert_eq!(mobj.get("spawnstate").unwrap(), "StateNum::S_MEAT4");
    }
}
