use std::f64::consts::{PI, TAU};
use std::fs;
use std::path::{Path, PathBuf};

const FINEANGLES: u32 = 8192;

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    let is_hd = std::env::var("CARGO_FEATURE_FIXED64HD").is_ok();

    if is_hd {
        gen_hd_tables(&out);
    }

    println!("cargo:rerun-if-changed=build.rs");
}

/// Generate 32.32 fixed-point trig tables (i64) for fixed64hd.
#[expect(
    clippy::cast_possible_truncation,
    reason = "rounded trig samples intentionally saturate to the fixed-point i64 range"
)]
fn gen_hd_tables(out: &Path) {
    let fracunit = 4_294_967_296.0;
    let path = out.join("hd_trig_tables.rs");

    let mut finesine = Vec::with_capacity(10240);
    for i in 0..FINEANGLES {
        let angle = (f64::from(i) + 0.5) * TAU / f64::from(FINEANGLES);
        finesine.push((fracunit * angle.sin()).round() as i64);
    }
    for i in 0..2048 {
        finesine.push(finesine[i]);
    }

    let mut finetangent = Vec::with_capacity(4096);
    for i in 0..4096u32 {
        let angle = (f64::from(i) - 2048.0 + 0.5) * PI / 4096.0;
        finetangent.push((fracunit * angle.tan()).round() as i64);
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
