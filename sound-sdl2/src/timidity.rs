//! Configuration data for timidity to emulate GUS. Reads either `DMXGUS` or
//! `DMXGUSC` from the Wad data.

use std::{num::ParseIntError, path::PathBuf};

use log::warn;
use wad::WadData;

#[derive(Debug, Copy, Clone)]
pub enum GusMemSize {
    P256,
    P512,
    P768,
    P1024,
    Perfect,
}

#[derive(Debug, Clone)]
struct TimidityMapping {
    base_num: i32,
    link_256k: i32,
    link_512k: i32,
    link_768k: i32,
    link_1024k: i32,
    name: String,
}

impl TimidityMapping {
    fn gen_line(
        &self,
        data: &[TimidityMapping],
        mut patch_path: PathBuf,
        mem_size: GusMemSize,
    ) -> String {
        let mut pnum = self.base_num;
        if self.base_num > 128 {
            pnum -= 128;
        }

        let link_num = match mem_size {
            GusMemSize::P256 => self.link_256k,
            GusMemSize::P512 => self.link_512k,
            GusMemSize::P768 => self.link_768k,
            GusMemSize::P1024 => self.link_1024k,
            GusMemSize::Perfect => self.base_num,
        };

        if !matches!(mem_size, GusMemSize::Perfect) {
            for t in data.iter() {
                if t.base_num == link_num {
                    patch_path.push(t.name.as_str());
                    break;
                }
            }
        } else {
            patch_path.push(self.name.as_str());
        }

        format!("{} {}", pnum, patch_path.to_string_lossy())
    }
}

impl TryInto<TimidityMapping> for String {
    type Error = ParseIntError;

    fn try_into(self) -> Result<TimidityMapping, Self::Error> {
        // Need trimming as the start of the split str may have a space
        let split: Vec<&str> = self.split(',').map(|s| s.trim_start()).collect();

        Ok(TimidityMapping {
            base_num: split[0].parse::<i32>()?,
            link_256k: split[1].parse::<i32>()?,
            link_512k: split[2].parse::<i32>()?,
            link_768k: split[3].parse::<i32>()?,
            link_1024k: split[4].parse::<i32>()?,
            name: split[5].to_string(),
        })
    }
}

fn parse(lump: &[u8]) -> Vec<TimidityMapping> {
    lump.split(|byte| *byte == b'\n')
        .filter_map(|l| {
            let line = String::from_utf8_lossy(l).trim_end().to_string();
            line.try_into().ok()
        })
        .collect()
}

/// Returns `None` if neither `DMXGUS` or `DMXGUSC` exist
pub fn make_timidity_cfg(
    wad: &WadData,
    patch_path: PathBuf,
    mem_size: GusMemSize,
) -> Option<Vec<u8>> {
    let lump = wad.get_lump("DMXGUS").or_else(|| wad.get_lump("DMXGUSC"))?;
    let gus = parse(&lump.data);

    let mut data = Vec::with_capacity(gus.len() * 10);
    for s in "bank 0".as_bytes() {
        data.push(*s);
    }
    data.push(b'\n');

    let mut count = 0;
    let mut complete = true;
    for g in gus.iter() {
        if count == 128 {
            for s in "drumset 0".as_bytes() {
                data.push(*s);
            }
            data.push(b'\n');
            count = 27;
            continue;
        }

        let mut tmp_path = patch_path.clone();
        tmp_path.push(format!("{}.pat", g.name));
        if !tmp_path.exists() {
            warn!("Missing: {:?}", tmp_path);
            complete = false;
        }

        for b in g.gen_line(&gus, patch_path.clone(), mem_size).as_bytes() {
            data.push(*b);
        }
        data.push(b'\n');
        count += 1;
    }
    if !complete {
        return None;
    }
    Some(data)
}

#[cfg(test)]
mod tests {
    use std::{fs::File, io::Write, path::PathBuf};

    use wad::WadData;

    use crate::timidity::{parse, TimidityMapping};

    use super::{make_timidity_cfg, GusMemSize};

    #[test]
    fn read_gus_data() {
        let wad = WadData::new("../doom1.wad".into());
        let gus = wad.get_lump("DMXGUS").unwrap();

        // line endings are `\r\n`
        let lines = parse(&gus.data);

        let tim: TimidityMapping = lines[0].clone();
        assert_eq!(tim.base_num, 0);
        assert_eq!(tim.name, "acpiano");

        let tim: TimidityMapping = lines[50].clone();
        assert_eq!(tim.base_num, 50);
        assert_eq!(tim.name, "synstr1");
        assert_eq!(lines.len(), 190);
    }

    #[test]
    fn read_gus_1024k() {
        let wad = WadData::new("../doom1.wad".into());

        let base = env!("CARGO_MANIFEST_DIR");
        let mut path = PathBuf::new();
        path.push(base);
        path.pop();
        path.push("data/sound/");
        if let Some(cfg) = make_timidity_cfg(&wad, path, GusMemSize::P1024) {
            let mut file = File::create("/tmp/timidity_1024k.cfg").unwrap();
            file.write_all(&cfg).unwrap();
        }
    }

    #[test]
    fn read_gus_perfect() {
        let wad = WadData::new("../doom1.wad".into());

        let base = env!("CARGO_MANIFEST_DIR");
        let mut path = PathBuf::new();
        path.push(base);
        path.pop();
        path.push("data/sound/");
        if let Some(cfg) = make_timidity_cfg(&wad, path, GusMemSize::Perfect) {
            let mut file = File::create("/tmp/timidity.cfg").unwrap();
            file.write_all(&cfg).unwrap();
        }
    }
}
