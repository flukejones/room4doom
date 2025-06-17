use glam::Vec2;
use math::{f2v, fconst, fixed, fvec2, v2f};

fn main() {
    let a = fixed!(3.14159);
    let b = fixed!(2.0);

    println!("Using fixed! macro: {} + {} = {}", a, b, a + b);

    let vec1 = fvec2!(3.0, 4.0);
    let vec2 = fvec2!(1.5);

    println!("Using fvec2! macro: {} and {}", vec1, vec2);

    let pi = fconst!(PI);
    let e = fconst!(E);
    let half_pi = fconst!(FRAC_PI_2);

    println!("Constants: PI={}, E={}, PI/2={}", pi, e, half_pi);

    let glam_vec = Vec2::new(1.0, 2.0);
    let fixed_vec = f2v!(glam_vec);
    let back_to_glam = v2f!(fixed_vec);

    println!(
        "Conversion: {:?} -> {} -> {:?}",
        glam_vec, fixed_vec, back_to_glam
    );

    let rotated = vec1.rotate(fconst!(FRAC_PI_4));
    println!("vec1 rotated by PI/4: {}", rotated);

    let distance = vec1.distance(fvec2!(0.0, 0.0));
    println!("Distance from origin: {}", distance);
}
