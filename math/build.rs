use std::f64::consts::{PI, TAU};
use std::fs;
use std::path::PathBuf;

const FINEANGLES: usize = 8192;

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    let is_hd = std::env::var("CARGO_FEATURE_FIXED64HD").is_ok();

    if is_hd {
        gen_hd_tables(&out);
    }

    println!("cargo:rerun-if-changed=build.rs");
}

/// Generate 32.32 fixed-point trig tables (i64) for fixed64hd.
fn gen_hd_tables(out: &PathBuf) {
    let fracunit: i64 = 1 << 32;
    let path = out.join("hd_trig_tables.rs");

    let mut finesine = Vec::with_capacity(10240);
    for i in 0..FINEANGLES {
        let angle = (i as f64 + 0.5) * TAU / FINEANGLES as f64;
        finesine.push((fracunit as f64 * angle.sin()).round() as i64);
    }
    for i in 0..2048 {
        finesine.push(finesine[i]);
    }

    let mut finetangent = Vec::with_capacity(4096);
    for i in 0..4096usize {
        let angle = (i as f64 - 2048.0 + 0.5) * PI / 4096.0;
        let v = (fracunit as f64 * angle.tan()).round();
        let clamped = if v > i64::MAX as f64 {
            i64::MAX
        } else if v < i64::MIN as f64 {
            i64::MIN
        } else {
            v as i64
        };
        finetangent.push(clamped);
    }

    let sine_entries: Vec<String> = finesine.iter().map(|v| format!("{v}_i64")).collect();
    let tan_entries: Vec<String> = finetangent.iter().map(|v| format!("{v}_i64")).collect();

    let content = format!(
        "pub const FINESINE_HD: [i64; 10240] = [{}];\n\npub const FINETANGENT_HD: [i64; 4096] = [{}];\n",
        sine_entries.join(","),
        tan_entries.join(",")
    );

    fs::write(&path, content).unwrap();
}
