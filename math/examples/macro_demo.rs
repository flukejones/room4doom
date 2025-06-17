use math::{fconst, fixed};

fn main() {
    let a = fixed!(3.14159);
    let b = fixed!(2.0);

    println!("Using fixed! macro: {} + {} = {}", a, b, a + b);

    let pi = fconst!(PI);
    let e = fconst!(E);
    let half_pi = fconst!(FRAC_PI_2);

    println!("Constants: PI={}, E={}, PI/2={}", pi, e, half_pi);
}
